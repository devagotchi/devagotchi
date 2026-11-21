//! The Devagotchi store (`devagotchi-store`): the Postgres engine the hub and AgentHub share
//! (ADR 0001). It owns store_request_receipts; each domain brings its own migrations, with a
//! version history of its own, and [`Store::open_in`] runs them in the order given (the hub's
//! in `crates/hub/migrations`, AgentHub's in its own repository). Every table is in schema
//! `devagotchi`, named after its domain (`.ai/patterns/layout.md`, rule 8). The store
//! installs its receipt migration last, after the caller has created any restricted role.
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

mod opening;
pub mod receipts;

use std::{path::Path, sync::Once};

use devagotchi_contracts::{BridgeError, BridgeResult};
use sqlx::{
    Any, AnyConnection, AnyPool, Connection, Executor, Transaction,
    any::{AnyPoolOptions, install_default_drivers},
};

/// A crate's migrations: `sqlx::migrate!` of its directory, with its own history table
/// (`table_name`) so two crates' versions never meet. Declared with `ignore_missing: true`, a
/// store a newer build migrated still opens: migrations add (tables, columns with defaults,
/// indexes), and a deploy that is not healthy can go back to the image before it.
pub use sqlx::migrate::Migrator;

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

impl Store {
    /// Where the store is: host, port and database; never a user name or password.
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

mod settings;
pub use settings::*;

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

#[cfg(test)]
mod tests;
