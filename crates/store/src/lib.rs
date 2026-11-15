//! The Devagotchi store (`devagotchi-store`), shared by the hub and AgentHub: the hub's
//! Postgres (ADR 0001). Its migrations are `migrations/postgres/`, Postgres only, every table
//! in schema `devagotchi` named after its domain (`.ai/patterns/layout.md`, rule 8).
//!
//! Every write goes through [`transaction!`]: one transaction that sees nothing another
//! writer does halfway. It runs SERIALIZABLE and runs again when the server says it collided
//! with another one, so code written for one writer at a time stays right with many hub
//! instances.
//!
//! Where the store is: `DEVAGOTCHI_HUB_STORAGE_URL` when set (how a Postgres password
//! arrives, from the secret store, never from a file), else the config's `storage.url`.
//!
//! Tenants are kept apart by the database itself (migration 0007): every table of a
//! tenant's rows has row-level security forced, and a row is visible only to a transaction
//! that set `devagotchi.tenant` to its tenant ([`Store::scope`]). The hub's queries run as
//! the role `devagotchi_hub` (no superuser, no BYPASSRLS), switched to on every connection
//! when the login role may; a key that must lead to its tenant first (a session's hash, a
//! token's) goes through [`Store::tenant_of`], which answers the tenant and nothing else;
//! the operator's lists of tenants by access (migration 0008) go through
//! [`Store::tenants_in`], which answers their ids and nothing else.

use std::{path::Path, sync::Once};

use devagotchi_contracts::{BridgeError, BridgeResult};
use sqlx::{
    Any, AnyConnection, AnyPool, Connection, Executor, Transaction,
    any::{AnyPoolOptions, install_default_drivers},
    migrate::Migrator,
};

// A store a newer hub migrated still opens here: migrations add (tables, columns with
// defaults, indexes), so an older hub runs on the newer schema, and a deploy that is not
// healthy can go back to the image before it (deploy/compose/update.sh). The one exception
// is 0007 (tenant_rls), which moves and renames the tables: going back across it restores
// the backup update.sh takes before every deploy (deploy/compose/README.md).
static POSTGRES: Migrator = Migrator {
    ignore_missing: true,
    ..sqlx::migrate!("migrations/postgres")
};

/// The environment variable that names the store, over the config's `storage.url`.
pub const STORAGE_URL_ENV: &str = "DEVAGOTCHI_HUB_STORAGE_URL";

/// How many times a Postgres transaction that collided with another is run again.
const RETRIES: u32 = 8;

#[derive(Clone)]
pub struct Store {
    pool: AnyPool,
    /// Whether the hub's queries are held to row-level security (Postgres, running as a
    /// role without superuser or BYPASSRLS).
    enforced: bool,
    /// Where it is, with no credentials in it ([`place_of`]), and its Postgres schema.
    place: String,
    schema: Option<String>,
    /// The same database without the role switch, for tests that set up rows under the API.
    #[cfg(any(test, feature = "testing"))]
    admin: AnyPool,
}

/// A key that leads to the tenant whose row it names ([`Store::tenant_of`]).
#[derive(Debug, Clone, Copy)]
pub enum Lookup<'a> {
    Account(&'a str),
    /// A browser session, by its token's SHA-256.
    Session(&'a str),
    /// An access or refresh token, by its SHA-256.
    Token(&'a str),
    /// An authorization code, by its SHA-256.
    Code(&'a str),
    /// A one-time link, by its SHA-256.
    Link(&'a str),
    Passkey(&'a [u8]),
    /// A linked sign-in: provider and subject.
    Identity(&'a str, &'a str),
    /// The account a verified email already reaches.
    Email(&'a str),
    /// The hub's owner.
    Owner,
    /// A workflow's link (`/go/<token>`), by its token's SHA-256 (migration 0010).
    Workflow(&'a str),
    /// An offers link (`/offers/<token>`), by its token's SHA-256 (migration 0012).
    Offer(&'a str),
}

/// The role the hub's queries run as on Postgres (migration 0007).
const HUB_ROLE: &str = "devagotchi_hub";
/// Where the hub's tables live on Postgres.
pub const TABLES_SCHEMA: &str = "devagotchi";

impl std::fmt::Debug for Store {
    /// Never prints the URL: it may carry a password.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Store({})", self.place)
    }
}

/// Why a transaction's work stopped: the store failed (and may be run again), or the work
/// refused (a `BridgeError` for the caller, nothing written).
pub enum Failed {
    Store(sqlx::Error),
    Refused(BridgeError),
}

impl From<sqlx::Error> for Failed {
    fn from(error: sqlx::Error) -> Self {
        Failed::Store(error)
    }
}

impl From<BridgeError> for Failed {
    fn from(error: BridgeError) -> Self {
        Failed::Refused(error)
    }
}

/// Run a block as one write transaction of `store` and commit it:
/// `transaction!(&store, |connection| { … })`, where the block is async code returning
/// `Result<T, Failed>` with `connection: &mut AnyConnection`. A refusal rolls everything
/// back and comes out as the `BridgeError`; on Postgres, a block that collided with
/// another transaction runs again from the start, so it must not act outside the store.
/// The value is a `BridgeResult<T>`. (A macro rather than a method taking an async
/// closure: the block stays in its caller, so whether the caller's future is `Send` is
/// worked out there, which a closure over a borrowed connection defeats.)
#[macro_export]
macro_rules! transaction {
    ($store:expr, |$connection:ident| $body:block) => {{
        let store: &$crate::Store = $store;
        let mut tries = 0u32;
        loop {
            let mut transaction = match store.begin().await {
                Ok(transaction) => transaction,
                Err(error) => break Err(error),
            };
            let outcome: Result<_, $crate::Failed> = async {
                let $connection: &mut ::sqlx::AnyConnection = &mut transaction;
                $body
            }
            .await;
            if let Some(done) = store.finish(transaction, outcome, &mut tries).await {
                break done;
            }
        }
    }};
}

pub fn store_error(error: sqlx::Error) -> BridgeError {
    BridgeError::new("account_store", error.to_string())
}

/// A time for the store: Unix seconds fit an i64 for the next 292 billion years.
pub fn secs(time: u64) -> i64 {
    i64::try_from(time).unwrap_or(i64::MAX)
}

/// A time from the store; a negative one (never written) reads as 0.
pub fn unsecs(time: i64) -> u64 {
    u64::try_from(time).unwrap_or(0)
}

/// The Postgres a hub uses: the environment's URL, else the config's; `None` when neither
/// names one.
pub fn url_for(configured: Option<&str>) -> Option<String> {
    match std::env::var(STORAGE_URL_ENV) {
        Ok(url) if !url.trim().is_empty() => Some(url.trim().to_string()),
        _ => configured.map(str::to_string),
    }
}

/// Whether a URL is one the store opens, and keeps its password out of a file: `postgres://`
/// or `postgresql://` without a password.
pub fn check_configured_url(url: &str) -> Result<(), String> {
    if let Some(rest) = url
        .strip_prefix("postgres://")
        .or_else(|| url.strip_prefix("postgresql://"))
    {
        let authority = rest.split(['/', '?']).next().unwrap_or("");
        if authority
            .rsplit_once('@')
            .is_some_and(|(user, _)| user.contains(':'))
        {
            return Err(format!(
                "storage.url must not carry a password; set {STORAGE_URL_ENV} from the secret store instead"
            ));
        }
        return Ok(());
    }
    Err("storage.url is postgres://… (the hub keeps everything in Postgres)".to_string())
}

/// The environment variable that names the Postgres schema the hub's tables live in, over
/// the config's `storage.schema`.
pub const STORAGE_SCHEMA_ENV: &str = "DEVAGOTCHI_HUB_STORAGE_SCHEMA";

/// A Postgres schema name the hub accepts: 1 to 63 of `[a-z0-9_]`, not starting with a digit
/// or `pg_`, so it needs no quoting and is never one of Postgres's own.
pub fn valid_schema(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 63
        && !name.starts_with("pg_")
        && !name.as_bytes()[0].is_ascii_digit()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// The schema a hub's tables live in on Postgres: the environment's, else the config's.
pub fn schema_for(configured: Option<&str>) -> Option<String> {
    match std::env::var(STORAGE_SCHEMA_ENV) {
        Ok(schema) if !schema.trim().is_empty() => Some(schema.trim().to_string()),
        _ => configured.map(str::to_string),
    }
}

impl Store {
    /// Open the store at `url` and bring its schema up to date.
    #[cfg_attr(not(any(test, feature = "testing")), allow(dead_code))]
    pub async fn open(url: &str) -> BridgeResult<Self> {
        Self::open_in(url, None).await
    }

    /// `open`, with the hub's tables in their own Postgres `schema`, made if missing: on a
    /// database other things share (a Supabase project, whose `public` schema its REST API
    /// serves to anyone with the project's public key), the hub's tables stay out of
    /// `public`. Every connection sets its `search_path` to it.
    pub async fn open_in(url: &str, schema: Option<&str>) -> BridgeResult<Self> {
        if let Some(schema) = schema
            && !valid_schema(schema)
        {
            return Err(BridgeError::new(
                "account_store",
                format!(
                    "{schema:?} is not a schema name: 1 to 63 of [a-z0-9_], not starting with a digit or pg_"
                ),
            ));
        }
        static DRIVERS: Once = Once::new();
        DRIVERS.call_once(install_default_drivers);
        if !url.starts_with("postgres://") && !url.starts_with("postgresql://") {
            return Err(BridgeError::new(
                "account_store",
                "the store's URL is postgres://… (the hub keeps everything in Postgres)",
            ));
        }
        let schema = schema.map(str::to_string);
        let home_schema = schema.clone().unwrap_or_else(|| "public".to_string());
        // The migrations run on a connection of their own, as the login role (it may make
        // schemas and roles), with the schema the store began in first on the path: the
        // migration record stays where it always was, and 0007 moves the tables into
        // `devagotchi`.
        {
            let mut admin = sqlx::AnyConnection::connect(url)
                .await
                .map_err(store_error)?;
            if let Some(schema) = &schema {
                admin
                    .execute(sqlx::AssertSqlSafe(format!(
                        "CREATE SCHEMA IF NOT EXISTS {schema}"
                    )))
                    .await
                    .map_err(store_error)?;
            }
            admin
                .execute(sqlx::AssertSqlSafe(format!(
                    "CREATE SCHEMA IF NOT EXISTS {TABLES_SCHEMA}; SET search_path TO {home_schema}, {TABLES_SCHEMA}"
                )))
                .await
                .map_err(store_error)?;
            POSTGRES
                .run(&mut admin)
                .await
                .map_err(|error| BridgeError::new("account_store", error.to_string()))?;
            let _ = admin.close().await;
        }
        let set_path = format!("SET search_path TO {TABLES_SCHEMA}, {home_schema}");
        let connect = |switch_role: bool| {
            let set_path = set_path.clone();
            AnyPoolOptions::new()
                .max_connections(16)
                .after_connect(move |connection, _| {
                    let set_path = set_path.clone();
                    Box::pin(async move {
                        // Checked names (`valid_schema`): no quoting, nothing to inject.
                        connection.execute(sqlx::AssertSqlSafe(set_path)).await?;
                        {
                            if switch_role {
                                let may: bool = sqlx::query_scalar(
                                    "SELECT CASE WHEN EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'devagotchi_hub')
                                     THEN pg_has_role(current_user, 'devagotchi_hub', 'MEMBER') ELSE false END",
                                )
                                .fetch_one(&mut *connection)
                                .await?;
                                if may {
                                    connection
                                        .execute(sqlx::AssertSqlSafe(format!("SET ROLE {HUB_ROLE}")))
                                        .await?;
                                }
                            }
                        }
                        Ok(())
                    })
                })
                .connect(url)
        };
        let pool = connect(true).await.map_err(store_error)?;
        #[cfg(any(test, feature = "testing"))]
        let admin = connect(false).await.map_err(store_error)?;
        let enforced = {
            {
                let mut connection = pool.acquire().await.map_err(store_error)?;
                // Held to row-level security: neither superuser nor BYPASSRLS.
                let held: bool = sqlx::query_scalar(
                    "SELECT NOT (rolsuper OR rolbypassrls) FROM pg_roles WHERE rolname = current_user",
                )
                .fetch_one(&mut *connection)
                .await
                .map_err(store_error)?;
                // The tenant lookup must see every tenant to find one: its owner is not held.
                let finds: bool = sqlx::query_scalar(
                    "SELECT bool_or(r.rolsuper OR r.rolbypassrls) FROM pg_proc p
                     JOIN pg_roles r ON r.oid = p.proowner
                     WHERE p.proname = 'tenant_of' AND p.pronamespace = 'devagotchi'::regnamespace",
                )
                .fetch_one(&mut *connection)
                .await
                .map_err(store_error)?;
                if held && !finds {
                    return Err(BridgeError::new(
                        "account_store",
                        "the store's tables belong to a role held to row-level security, so the hub cannot find a tenant; migrate it as a role with BYPASSRLS (or superuser) and run the hub as another (crates/hub/README.md, \"The store\")",
                    ));
                }
                held
            }
        };
        Ok(Self {
            pool,
            enforced,
            place: place_of(url),
            schema,
            #[cfg(any(test, feature = "testing"))]
            admin,
        })
    }

    /// Where the store is, to show a person: Postgres's host, port and database; never a
    /// user name or password.
    pub fn place(&self) -> &str {
        &self.place
    }

    /// The Postgres schema its tables are in, when one was named (else `public`, before
    /// migration 0007 moves them to `devagotchi`).
    pub fn schema(&self) -> Option<&str> {
        self.schema.as_deref()
    }

    /// Whether the hub's queries are held to row-level security (Postgres as a role with
    /// neither superuser nor BYPASSRLS): shown in `/healthz`.
    pub fn enforced(&self) -> bool {
        self.enforced
    }

    /// Scope the transaction on `connection` to `tenant`: from here to its end it sees and
    /// writes that tenant's rows only (`set_config(…, true)`).
    pub async fn scope(
        &self,
        connection: &mut AnyConnection,
        tenant: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("SELECT set_config('devagotchi.tenant', $1, true)")
            .bind(tenant)
            .execute(&mut *connection)
            .await?;
        Ok(())
    }

    /// The tenant whose row `lookup` names, if any, through `devagotchi.tenant_of`, which
    /// answers that and nothing else.
    pub async fn tenant_of(
        &self,
        connection: &mut AnyConnection,
        lookup: Lookup<'_>,
    ) -> Result<Option<String>, sqlx::Error> {
        {
            if let Lookup::Workflow(hash) = lookup {
                return sqlx::query_scalar("SELECT devagotchi.workflow_tenant($1)")
                    .bind(hash)
                    .fetch_one(&mut *connection)
                    .await;
            }
            if let Lookup::Offer(hash) = lookup {
                return sqlx::query_scalar("SELECT devagotchi.offer_tenant($1)")
                    .bind(hash)
                    .fetch_one(&mut *connection)
                    .await;
            }
            let (kind, key, extra): (&str, String, Option<&str>) = match lookup {
                Lookup::Account(id) => ("account", id.to_string(), None),
                Lookup::Session(hash) => ("session", hash.to_string(), None),
                Lookup::Token(hash) => ("token", hash.to_string(), None),
                Lookup::Code(hash) => ("code", hash.to_string(), None),
                Lookup::Link(hash) => ("link", hash.to_string(), None),
                Lookup::Passkey(id) => ("passkey", hex::encode(id), None),
                Lookup::Identity(provider, subject) => {
                    ("identity", provider.to_string(), Some(subject))
                }
                Lookup::Email(email) => ("email", email.to_string(), None),
                Lookup::Owner => ("owner", String::new(), None),
                Lookup::Workflow(_) | Lookup::Offer(_) => unreachable!("answered above"),
            };
            return sqlx::query_scalar("SELECT devagotchi.tenant_of($1, $2, $3)")
                .bind(kind)
                .bind(key)
                .bind(extra)
                .fetch_one(&mut *connection)
                .await;
        }
    }

    /// Accounts nobody secured yet that were used after `since`, across the hub.
    pub async fn live_unsecured(
        &self,
        connection: &mut AnyConnection,
        since: i64,
    ) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar("SELECT devagotchi.live_unsecured($1)")
            .bind(since)
            .fetch_one(&mut *connection)
            .await
    }

    /// The tenants whose access is `state` (`admitted` or `waiting`), at most `most`: the
    /// longest waiting first, the latest let in first. Ids only, through
    /// `devagotchi.tenants_in` (migration 0008), so the operator's lists name tenants and
    /// each one's details are read in a transaction scoped to it.
    pub async fn tenants_in(
        &self,
        connection: &mut AnyConnection,
        state: &str,
        most: i64,
    ) -> Result<Vec<String>, sqlx::Error> {
        sqlx::query_scalar("SELECT devagotchi.tenants_in($1, $2)")
            .bind(state)
            .bind(most)
            .fetch_all(&mut *connection)
            .await
    }

    /// A connection for reads outside a transaction.
    pub async fn read(&self) -> BridgeResult<sqlx::pool::PoolConnection<Any>> {
        self.pool.acquire().await.map_err(store_error)
    }

    /// A write transaction for [`transaction!`], SERIALIZABLE.
    pub async fn begin(&self) -> BridgeResult<Transaction<'static, Any>> {
        (self.pool)
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await
            .map_err(store_error)
    }

    /// End a [`transaction!`]: commit what the work did, or roll back what it refused.
    /// `None` when Postgres found it collided with another transaction and it is to run
    /// again (counted in `tries`).
    pub async fn finish<T>(
        &self,
        transaction: Transaction<'static, Any>,
        outcome: Result<T, Failed>,
        tries: &mut u32,
    ) -> Option<BridgeResult<T>> {
        let outcome = match outcome {
            Ok(value) => transaction
                .commit()
                .await
                .map(|()| value)
                .map_err(Failed::Store),
            Err(failed) => {
                let _ = transaction.rollback().await;
                Err(failed)
            }
        };
        match outcome {
            Ok(value) => Some(Ok(value)),
            Err(Failed::Refused(error)) => Some(Err(error)),
            Err(Failed::Store(error)) if collided(&error) && *tries < RETRIES => {
                *tries += 1;
                // Run again at once, two transactions that collided collide again, until
                // the retries are gone: a short wait, longer each time, different for each.
                let jitter = u64::from(random::<1>()[0]) / 4;
                let wait = (1u64 << (*tries).min(6)) + jitter;
                tokio::time::sleep(std::time::Duration::from_millis(wait)).await;
                None
            }
            Err(Failed::Store(error)) => Some(Err(store_error(error))),
        }
    }

    /// The pool, for tests that reach under the API. Its connections run as the hub does:
    /// on Postgres, held to row-level security.
    #[cfg(any(test, feature = "testing"))]
    pub fn pool(&self) -> &AnyPool {
        &self.pool
    }

    /// The same database as the login role, without the switch to the hub's role: for
    /// tests that set rows up under the API, whatever their tenant.
    #[cfg(any(test, feature = "testing"))]
    pub fn admin(&self) -> &AnyPool {
        &self.admin
    }
}

/// Postgres refused a transaction because another one ran alongside it: serialization
/// failure (40001), deadlock (40P01), or a unique violation (23505). Running it again is the
/// remedy. The last is a collision too: a SERIALIZABLE transaction keeps the snapshot it
/// began with even after waiting on a row another transaction then committed, so a key it
/// computed from that snapshot (the next note or event `seq` of a hosted board, after
/// `SELECT … FOR UPDATE` on the board's row) may already be taken by the time it inserts.
/// Postgres reports that as the unique violation, not as 40001. Run again, the block sees
/// the committed rows: a repeat of a request it already recorded comes back as its receipt.
/// A duplicate that is no collision fails the same way after the retries.
fn collided(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|e| e.code())
        .is_some_and(|code| code == "40001" || code == "40P01" || code == "23505")
}

#[cfg(any(test, feature = "testing"))]
pub mod testing;

/// Write a new file only its owner may read (0600 on Unix), refusing one that exists.
pub fn write_private(path: &Path, bytes: &[u8]) -> BridgeResult<()> {
    let failed =
        |e: std::io::Error| BridgeError::new("state_file", format!("{}: {e}", path.display()));
    #[cfg(unix)]
    {
        use std::{io::Write, os::unix::fs::OpenOptionsExt};
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .map_err(failed)?;
        file.write_all(bytes).map_err(failed)
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, bytes).map_err(failed)
    }
}

/// `N` bytes from the operating system's random source.
pub fn random<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).expect("operating system random source");
    bytes
}

/// A tenant id the hub made: `t-` and a lowercase UUID.
pub fn valid_tenant_id(id: &str) -> bool {
    let Some(uuid) = id.strip_prefix("t-") else {
        return false;
    };
    uuid.len() == 36
        && uuid.bytes().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => b == b'-',
            _ => b.is_ascii_digit() || (b'a'..=b'f').contains(&b),
        })
}

/// A store's URL without what must not be shown: the user and password before `@`, and the
/// query (which may carry a password too).
pub fn place_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let rest = rest.split(['?', '#']).next().unwrap_or_default();
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let host = if host.is_empty() {
        "local socket"
    } else {
        host
    };
    format!("{host}/{path}")
}

#[cfg(test)]
mod tests;
