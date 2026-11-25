//! One-time links (the owner's first passkey, later invites and resets): issuing one, replacing the account's passkeys with one, and checking one is live.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use devagotchi_contracts::{BridgeError, BridgeResult};
use sqlx::{AnyConnection, Row, any::AnyRow};

use super::{Accounts, LINK_TTL_SECS, LinkPurpose, account_in, hash};
use crate::{
    machines::{now_secs, random},
    store::{Failed, Lookup, secs, store_error, transaction},
};

impl Accounts {
    /// Issue a one-time link for `account`: 256 random bits the store draws itself. Only
    /// its SHA-256 is kept; a new link of the same purpose supersedes the account's unused
    /// one.
    pub async fn issue_link(&self, purpose: LinkPurpose, account: &str) -> BridgeResult<String> {
        self.issue_link_at(purpose, account, random::<32>(), now_secs())
            .await
    }

    /// Issue a one-time link for `account` whose use also removes every other passkey of the
    /// account (and so every session they opened): how its holder takes it back from a
    /// passkey someone else made with a leaked link (`devagotchi hub owner --replace`).
    pub async fn issue_replacing_link(
        &self,
        purpose: LinkPurpose,
        account: &str,
    ) -> BridgeResult<String> {
        self.issue(purpose, account, random::<32>(), now_secs(), true)
            .await
    }

    pub(super) async fn issue_link_at(
        &self,
        purpose: LinkPurpose,
        account: &str,
        random: [u8; 32],
        now: u64,
    ) -> BridgeResult<String> {
        self.issue(purpose, account, random, now, false).await
    }

    pub(super) async fn issue(
        &self,
        purpose: LinkPurpose,
        account: &str,
        random: [u8; 32],
        now: u64,
        replaces: bool,
    ) -> BridgeResult<String> {
        let token = link_token(random);
        transaction!(&self.store, |connection| {
            self.enter(connection, Lookup::Account(account)).await?;
            issue_in(connection, purpose, account, &token, now, replaces).await
        })?;
        Ok(token)
    }

    /// The account a link is for, and whether using it replaces the account's other
    /// passkeys, when it is unused and unexpired. Checking does not spend it.
    pub async fn check_link_replacing(
        &self,
        purpose: LinkPurpose,
        token: &str,
    ) -> BridgeResult<(String, bool)> {
        let now = now_secs();
        transaction!(&self.store, |connection| {
            self.enter(connection, Lookup::Link(&hash(token))).await?;
            Ok(live_link(connection, purpose, token, now)
                .await
                .map(|live| (live.account, live.replaces))?)
        })
    }

    /// The account a link is for, when it is unused and unexpired. Checking does not
    /// spend it: a link preview cannot burn it.
    pub async fn check_link(&self, purpose: LinkPurpose, token: &str) -> BridgeResult<String> {
        self.check_link_at(purpose, token, now_secs()).await
    }

    pub(crate) async fn check_link_at(
        &self,
        purpose: LinkPurpose,
        token: &str,
        now: u64,
    ) -> BridgeResult<String> {
        transaction!(&self.store, |connection| {
            self.enter(connection, Lookup::Link(&hash(token))).await?;
            Ok(live_link(connection, purpose, token, now)
                .await
                .map(|live| live.account)?)
        })
    }
}

/// A one-time link's token: `dgl_` and 256 random bits.
pub(super) fn link_token(random: [u8; 32]) -> String {
    format!("dgl_{}", URL_SAFE_NO_PAD.encode(random))
}

/// Keep `token` as `account`'s live link for `purpose`, superseding its unused one, inside
/// a transaction the caller holds.
pub(super) async fn issue_in(
    connection: &mut AnyConnection,
    purpose: LinkPurpose,
    account: &str,
    token: &str,
    now: u64,
    replaces: bool,
) -> Result<(), Failed> {
    let holder = account_in(connection, account)
        .await?
        .ok_or_else(|| BridgeError::new("no_account", format!("no account {account}")))?;
    if purpose == LinkPurpose::Owner && !holder.owner {
        return Err(BridgeError::new("not_owner", format!("{account} is not the owner")).into());
    }
    sqlx::query(
        "UPDATE auth_links SET used_at = $1 WHERE purpose = $2 AND account = $3 AND used_at IS NULL",
    )
    .bind(secs(now))
    .bind(purpose.as_str())
    .bind(account)
    .execute(&mut *connection)
    .await?;
    sqlx::query(
        "INSERT INTO auth_links (hash, purpose, account, expires_at, replaces, tenant_id)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(hash(token))
    .bind(purpose.as_str())
    .bind(account)
    .bind(secs(now + LINK_TTL_SECS))
    .bind(i64::from(replaces))
    .bind(&holder.tenant)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

/// A link that is unused and unexpired: the account it is for, and whether using it
/// replaces the account's other passkeys.
pub(super) struct LiveLink {
    pub(super) account: String,
    pub(super) replaces: bool,
}

pub(super) async fn live_link(
    connection: &mut AnyConnection,
    purpose: LinkPurpose,
    token: &str,
    now: u64,
) -> BridgeResult<LiveLink> {
    let invalid = || BridgeError::new("invalid_link", "this link is used, expired or unknown");
    let row = sqlx::query(
        "SELECT account, expires_at, used_at, replaces FROM auth_links WHERE hash = $1 AND purpose = $2",
    )
    .bind(hash(token))
    .bind(purpose.as_str())
    .fetch_optional(&mut *connection)
    .await
    .map_err(store_error)?;
    let Some(row) = row else {
        return Err(invalid());
    };
    let decode = |row: &AnyRow| -> Result<(String, i64, Option<i64>, i64), sqlx::Error> {
        Ok((
            row.try_get(0)?,
            row.try_get(1)?,
            row.try_get(2)?,
            row.try_get(3)?,
        ))
    };
    match decode(&row).map_err(store_error)? {
        (account, expires_at, None, replaces) if secs(now) < expires_at => Ok(LiveLink {
            account,
            replaces: replaces == 1,
        }),
        _ => Err(invalid()),
    }
}
