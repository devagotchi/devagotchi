//! The hub's own accounts (D25, §14): people sign in with passkeys on the hub itself, so a
//! hub works with nothing else deployed. Nomen ("Sign in with Shippin") is an extra way in.
//!
//! Passkey challenges are the hub's own and cost nothing to hand out: each is a time, a
//! random nonce and an HMAC over them, the ceremony's purpose and (to register) the
//! account, under a key kept in the store. Nothing is written until a ceremony succeeds;
//! then its challenge is recorded as spent, in the same transaction as what it did, until
//! it would have expired anyway. A flood of unanswered sign-ins fills nothing.
//!
//! The hub's pages use it in `web/passkeys.rs`; `devagotchi hub owner` makes the owner and
//! the one-time link.

mod account;
pub mod api_keys;
mod challenges;
pub mod claim;
pub mod clients;
pub mod email;
pub mod identities;
mod links;
pub mod oauth;
pub mod passkey;
mod passwords;
pub mod sessions;
pub mod sign_in_settings;
pub mod tenants;

use aws_lc_rs::hmac;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use devagotchi_contracts::{BridgeError, BridgeResult};
use sha2::{Digest, Sha256};
use sqlx::{AnyConnection, Row, any::AnyRow};

use crate::{
    machines::{now_secs, random},
    store::{Failed, Lookup, Store, secs, store_error, transaction, unsecs},
};

pub use passkey::{Alg, NewCredential};

pub(crate) use account::account_disabled;
use account::{check_name, create_in};
use challenges::stored_key;
use links::{issue_in, link_token, live_link};

/// How long a one-time link (the owner's, later invites and resets) stays valid.
pub const LINK_TTL_SECS: u64 = 15 * 60;
/// How long a passkey ceremony may take between its challenge and its answer.
pub const CHALLENGE_TTL_SECS: u64 = 5 * 60;

/// A new account's id: `acct_` and 26 lowercase base32 characters (128 random bits),
/// safe as a directory name wherever an account's files live.
pub fn new_account_id(random: [u8; 16]) -> String {
    const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
    let mut out = String::from(ACCOUNT_PREFIX);
    let (mut buffer, mut bits) = (0u32, 0u32);
    for byte in random {
        buffer = (buffer << 8) | u32::from(byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((buffer >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(ALPHABET[((buffer << (5 - bits)) & 31) as usize] as char);
    }
    out
}

const ACCOUNT_PREFIX: &str = "acct_";

/// An account id the hub accepts: a new `acct_` id, or a subject an identity provider
/// issued before the hub had accounts of its own (kept as it is, so no history is
/// rewritten). Either way 1 to 64 of `[A-Za-z0-9_-]`, safe as a directory name.
pub fn valid_account_id(id: &str) -> bool {
    if let Some(rest) = id.strip_prefix(ACCOUNT_PREFIX) {
        return rest.len() == 26
            && rest
                .bytes()
                .all(|b| b.is_ascii_lowercase() || (b'2'..=b'7').contains(&b));
    }
    crate::identity::auth::valid_org(id)
}

/// What a one-time link is for. The owner's link is the only one so far; invites and
/// reset links (decisions 5 and 7) join it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkPurpose {
    Owner,
}

impl LinkPurpose {
    fn as_str(self) -> &'static str {
        match self {
            LinkPurpose::Owner => "owner",
        }
    }
}

/// What a passkey challenge was issued for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ceremony {
    Register,
    SignIn,
}

impl Ceremony {
    fn as_str(self) -> &'static str {
        match self {
            Ceremony::Register => "register",
            Ceremony::SignIn => "sign_in",
        }
    }
}

/// What lets a passkey be added to an account: a live one-time link for it. (A signed-in
/// session of the account joins this with the account page.)
#[derive(Clone, Copy)]
#[allow(dead_code)] // the account page (32a-3) adds passkeys on a session's authority
pub enum Authority<'a> {
    Link(LinkPurpose, &'a str),
}

/// Never prints the link itself: it is a live secret until spent.
impl std::fmt::Debug for Authority<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Authority::Link(purpose, _) => write!(f, "Link({purpose:?}, <redacted>)"),
        }
    }
}

fn invalid_ceremony() -> BridgeError {
    BridgeError::new("invalid_ceremony", "sign-in timed out; start again")
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Account {
    pub id: String,
    pub name: String,
    pub owner: bool,
    pub disabled_at: Option<u64>,
    /// The tenant it belongs to (`tenants.rs`): `t-<uuid>`.
    pub tenant: String,
    /// Made on first connect and not secured yet: no passkey and no linked sign-in, so only
    /// the browser session and the AI tokens it approved reach it, and nothing can recover
    /// it. It ends once unused for `tenancy.unsecured.idle_days`.
    pub provisional: bool,
    /// When it was last used, kept for an account not secured yet.
    pub last_active: Option<u64>,
    /// Its tenant's place on the hub (`access.rs`), where a request read it.
    pub access: Option<crate::tenancy::access::Access>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Passkey {
    pub id: Vec<u8>,
    pub account: String,
    pub label: String,
    pub public_key: Vec<u8>,
    pub alg: Alg,
    pub sign_count: u32,
    pub backup_eligible: bool,
}

pub struct Accounts {
    /// Read by the domains that keep rows through the account store (`<domain>/stored.rs`).
    pub(crate) store: Store,
    /// How long the hub's own AI tokens live (`auth.tokens`).
    pub(crate) tokens: crate::config::TokenConfig,
    /// One owner's hub or a hosted one, and what an account not secured yet may hold.
    pub(crate) tenancy: crate::config::TenancyConfig,
    /// The identities whose sign-in is the hub's operator (`operators`).
    pub(crate) operators: Vec<String>,
    /// The site early access and plans are read from (`access.rs`), as the hub serves it.
    pub(crate) site: std::sync::Arc<crate::site::SiteCell>,
    /// This hub's address, for the links its messages give.
    pub(crate) public_url: String,
    /// Who hears that tenants were let in (`access::Accounts::on_admitted`).
    pub(crate) announce: std::sync::OnceLock<crate::tenancy::access::Announce>,
    /// Whether each tenant's milestones are recorded (`analytics::funnel`), in the
    /// transaction that does each: on a hosted hub that counts.
    pub(crate) counting: bool,
    /// Signs the passkey challenges this process makes: the store's key as last read. The
    /// store's copy is the one that counts: every ceremony checks against it, read in its
    /// own transaction, and refreshes this one. It changes only when a clock is found set
    /// back (`clock_set_back`).
    challenge_key: std::sync::RwLock<hmac::Key>,
    /// Failed password sign-ins per email, the times of those in the last 15 minutes
    /// (`passwords.rs`); in memory, like the hosted hub's other limits.
    password_failures: std::sync::Mutex<std::collections::HashMap<String, Vec<u64>>>,
    /// Tenant lifecycle and in-process events a hosted edition listens to (`extensions`).
    extensions: crate::extensions::Extensions,
}

pub(crate) fn hash(secret: &str) -> String {
    hex::encode(Sha256::digest(secret.trim().as_bytes()))
}

fn account_row(row: &AnyRow) -> Result<Account, sqlx::Error> {
    Ok(Account {
        id: row.try_get(0)?,
        name: row.try_get(1)?,
        owner: row.try_get::<i64, _>(2)? == 1,
        disabled_at: row.try_get::<Option<i64>, _>(3)?.map(unsecs),
        tenant: row.try_get::<Option<String>, _>(4)?.unwrap_or_default(),
        provisional: row.try_get::<i64, _>(5)? == 1,
        last_active: row.try_get::<Option<i64>, _>(6)?.map(unsecs),
        access: None,
    })
}

macro_rules! account_columns {
    () => {
        "id, name, owner, disabled_at, tenant_id, provisional, last_active"
    };
}
/// The account's columns as `a.…`, for a join (`account_row` reads them first).
macro_rules! account_columns_a {
    () => {
        "a.id, a.name, a.owner, a.disabled_at, a.tenant_id, a.provisional, a.last_active"
    };
}
pub(crate) use account_columns_a;

pub(crate) async fn account_in(
    connection: &mut AnyConnection,
    id: &str,
) -> Result<Option<Account>, sqlx::Error> {
    sqlx::query(concat!(
        "SELECT ",
        account_columns!(),
        " FROM auth_accounts WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(&mut *connection)
    .await?
    .as_ref()
    .map(account_row)
    .transpose()
}

async fn owner_in(connection: &mut AnyConnection) -> Result<Option<Account>, sqlx::Error> {
    sqlx::query(concat!(
        "SELECT ",
        account_columns!(),
        " FROM auth_accounts WHERE owner = 1"
    ))
    .fetch_optional(&mut *connection)
    .await?
    .as_ref()
    .map(account_row)
    .transpose()
}

impl Accounts {
    /// Scope this transaction to the tenant `lookup` leads to, and say which; `None` (and
    /// nothing visible) when no row has that key (`store/mod.rs`, row-level security).
    pub(crate) async fn enter(
        &self,
        connection: &mut AnyConnection,
        lookup: Lookup<'_>,
    ) -> Result<Option<String>, sqlx::Error> {
        let tenant = self.store.tenant_of(connection, lookup).await?;
        self.store
            .scope(connection, tenant.as_deref().unwrap_or(""))
            .await?;
        Ok(tenant)
    }

    /// The accounts in `store`. Its challenge key is made the first time, once, whichever
    /// hub instance gets there first.
    pub async fn open(store: Store) -> BridgeResult<Self> {
        let fresh = random::<32>().to_vec();
        let home = tenants::new_tenant_id(random::<16>());
        let now = now_secs();
        let key = transaction!(&store, |connection| {
            sqlx::query(
                "INSERT INTO hub_secrets (name, value) VALUES ('challenge_key', $1)
                     ON CONFLICT (name) DO NOTHING",
            )
            .bind(&fresh)
            .execute(&mut *connection)
            .await?;
            // Every account belongs to a tenant: the hub's first one is made now if
            // missing (migration 0007 put the accounts made before tenants in it).
            tenants::settle_in(&store, connection, &home, now).await?;
            Ok(stored_key(connection).await?)
        })?;
        Ok(Self {
            store,
            tokens: crate::config::TokenConfig::default(),
            tenancy: crate::config::TenancyConfig::default(),
            operators: Vec::new(),
            site: std::sync::Arc::new(crate::site::SiteCell::new(crate::site::builtin())),
            public_url: String::new(),
            announce: std::sync::OnceLock::new(),
            counting: false,
            challenge_key: std::sync::RwLock::new(key),
            password_failures: Default::default(),
            extensions: crate::extensions::Extensions::core(),
        })
    }

    /// These accounts notifying `extensions` when a tenant is created.
    pub fn with_extensions(mut self, extensions: crate::extensions::Extensions) -> Self {
        self.extensions = extensions;
        self
    }

    /// These accounts reading early access and plans from `site` (the hub's, which follows
    /// its site directory), with `public_url` in the links their messages give.
    pub fn with_site(
        mut self,
        site: std::sync::Arc<crate::site::SiteCell>,
        public_url: &str,
    ) -> Self {
        self.site = site;
        self.public_url = public_url.trim_end_matches('/').to_string();
        self
    }

    /// These accounts with the config's lifetimes and limits for AI connections.
    pub fn with_tokens(mut self, tokens: crate::config::TokenConfig) -> Self {
        self.tokens = tokens;
        self
    }

    /// These accounts recording each tenant's milestones when `counting`
    /// (`HubConfig::counts_visits`).
    pub fn with_analytics(mut self, counting: bool) -> Self {
        self.counting = counting;
        self
    }

    /// These accounts as the config's tenancy and operators say.
    pub fn with_tenancy(
        mut self,
        tenancy: crate::config::TenancyConfig,
        operators: Vec<String>,
    ) -> Self {
        self.tenancy = tenancy;
        self.operators = operators;
        self
    }

    /// An HMAC under the store's key over `data`, for `purpose`: what makes a value the
    /// hub hands a browser (a sign-in's state) its own.
    pub fn mac(&self, purpose: &str, data: &[u8]) -> Vec<u8> {
        let mut message = Vec::with_capacity(32 + purpose.len() + data.len());
        message.extend_from_slice(b"devagotchi ");
        message.extend_from_slice(purpose.as_bytes());
        message.push(0);
        message.extend_from_slice(data);
        hmac::sign(&self.challenge_key.read().expect("challenge key"), &message)
            .as_ref()
            .to_vec()
    }

    /// Whether `tag` is `mac(purpose, data)`, compared in constant time.
    pub fn mac_ok(&self, purpose: &str, data: &[u8], tag: &[u8]) -> bool {
        let mut message = Vec::with_capacity(32 + purpose.len() + data.len());
        message.extend_from_slice(b"devagotchi ");
        message.extend_from_slice(purpose.as_bytes());
        message.push(0);
        message.extend_from_slice(data);
        hmac::verify(
            &self.challenge_key.read().expect("challenge key"),
            &message,
            tag,
        )
        .is_ok()
    }

    /// The store, for the modules that keep their tables beside the accounts.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn store(&self) -> &Store {
        &self.store
    }
}

#[cfg(test)]
mod tests;
