//! An account's passkeys: adding one (with a link or signed in), reading them, signing in with one, and removing one.

use devagotchi_contracts::{BridgeError, BridgeResult};
use sqlx::{Row, any::AnyRow};

use super::{Alg, Asserted, NewCredential};
use crate::{
    identity::accounts::{
        Account, Accounts, Authority, Ceremony, LinkPurpose, Passkey, account_disabled, account_in,
        challenges::spend_challenge, hash, invalid_ceremony, links::live_link, sessions, tenants,
    },
    machines::now_secs,
    store::{Lookup, secs, transaction},
};

macro_rules! passkey_columns {
    () => {
        "id, account, label, public_key, alg, sign_count, backup_eligible"
    };
}

fn passkey_row(row: &AnyRow) -> Result<Passkey, sqlx::Error> {
    let alg: i64 = row.try_get(4)?;
    let count: i64 = row.try_get(5)?;
    let damaged = |what: &str| sqlx::Error::Decode(format!("passkey {what} out of range").into());
    Ok(Passkey {
        id: row.try_get(0)?,
        account: row.try_get(1)?,
        label: row.try_get(2)?,
        public_key: row.try_get(3)?,
        // Only accepted algorithms are ever stored; an unknown one reads as unusable.
        alg: Alg::from_cose(alg).ok_or_else(|| damaged("algorithm"))?,
        sign_count: u32::try_from(count).map_err(|_| damaged("counter"))?,
        backup_eligible: row.try_get::<i64, _>(6)? == 1,
    })
}

impl Accounts {
    /// Store a passkey for `account`, as `verify_registration` returned it, on the
    /// `authority` of a live link for this account. The challenge it answered must be one
    /// this hub made to register a passkey on this account. The challenge and the link are
    /// spent in the same transaction as the passkey is stored: all of it happens or none
    /// does.
    #[allow(dead_code)] // the account page (32a-3)
    pub async fn add_passkey(
        &self,
        account: &str,
        credential: &NewCredential,
        label: &str,
        authority: Authority<'_>,
    ) -> BridgeResult<()> {
        let Authority::Link(purpose, token) = authority;
        self.store_passkey(
            account,
            credential,
            label,
            Some((purpose, token)),
            now_secs(),
        )
        .await
    }

    /// `add_passkey`, with the authority optional: for this module and its tests only.
    pub(in crate::identity::accounts) async fn store_passkey(
        &self,
        account: &str,
        credential: &NewCredential,
        label: &str,
        link: Option<(LinkPurpose, &str)>,
        now: u64,
    ) -> BridgeResult<()> {
        self.put_passkey(account, credential, label, link, None, now, None)
            .await
            .map(|_| ())
    }

    /// Add a passkey to the account a live browser session (`session`, its token) is
    /// signed in to: how an account made on first connect is secured. The challenge must be
    /// one this hub made to register a passkey on that account, and the session must still
    /// be its, checked in the same transaction as the passkey is stored.
    pub async fn add_passkey_signed_in(
        &self,
        account: &str,
        credential: &NewCredential,
        label: &str,
        session: &str,
    ) -> BridgeResult<()> {
        self.put_passkey(
            account,
            credential,
            label,
            None,
            Some(session),
            now_secs(),
            None,
        )
        .await
        .map(|_| ())
    }

    pub async fn passkey(&self, id: &[u8]) -> BridgeResult<Option<Passkey>> {
        transaction!(&self.store, |connection| {
            self.enter(connection, Lookup::Passkey(id)).await?;
            Ok(sqlx::query(concat!(
                "SELECT ",
                passkey_columns!(),
                " FROM auth_passkeys WHERE id = $1"
            ))
            .bind(id)
            .fetch_optional(&mut *connection)
            .await
            .and_then(|row| row.as_ref().map(passkey_row).transpose())?)
        })
    }

    pub async fn passkeys_of(&self, account: &str) -> BridgeResult<Vec<Passkey>> {
        transaction!(&self.store, |connection| {
            self.enter(connection, Lookup::Account(account)).await?;
            Ok(sqlx::query(concat!(
                "SELECT ",
                passkey_columns!(),
                " FROM auth_passkeys WHERE account = $1 ORDER BY created_at, id"
            ))
            .bind(account)
            .fetch_all(&mut *connection)
            .await
            .and_then(|rows| rows.iter().map(passkey_row).collect::<Result<Vec<_>, _>>())?)
        })
    }

    /// Record a sign-in, as `check_sign_in` returned it. In one transaction: the challenge
    /// is a live sign-in challenge of this hub's and is spent; the passkey exists with the
    /// key that verified the signature; its account exists and is not disabled; the user
    /// handle is that account's; and the counter moves forward from what is stored now, so
    /// two sign-ins racing with one copied key cannot both count. Returns the account.
    pub async fn sign_in(&self, asserted: &Asserted) -> BridgeResult<Account> {
        self.sign_in_at(asserted, now_secs()).await
    }

    pub(in crate::identity::accounts) async fn sign_in_at(
        &self,
        asserted: &Asserted,
        now: u64,
    ) -> BridgeResult<Account> {
        if self.clock_set_back(now).await? {
            return Err(invalid_ceremony());
        }
        transaction!(&self.store, |connection| {
            let (mac, issued) = self
                .check_challenge(connection, Ceremony::SignIn, None, &asserted.challenge, now)
                .await?;
            spend_challenge(connection, &mac, issued, now).await?;
            self.enter(connection, Lookup::Passkey(&asserted.id))
                .await?;
            let not_here =
                || BridgeError::new("invalid_passkey", "this passkey is not registered here");
            let passkey = sqlx::query(concat!(
                "SELECT ",
                passkey_columns!(),
                " FROM auth_passkeys WHERE id = $1"
            ))
            .bind(&asserted.id)
            .fetch_optional(&mut *connection)
            .await?
            .as_ref()
            .map(passkey_row)
            .transpose()?
            .ok_or_else(not_here)?;
            // The key, algorithm and backup eligibility the signature was checked with
            // must be the ones stored for this passkey.
            if passkey.public_key != asserted.public_key
                || passkey.alg != asserted.alg
                || passkey.backup_eligible != asserted.backup_eligible
            {
                return Err(not_here().into());
            }
            let account = account_in(connection, &passkey.account)
                .await?
                .ok_or_else(not_here)?;
            if account.disabled_at.is_some() {
                return Err(account_disabled(&account.id).into());
            }
            if asserted.user_handle != account.id.as_bytes() {
                return Err(BridgeError::new(
                    "user_handle",
                    "the passkey answered for another account",
                )
                .into());
            }
            // Authenticators that keep no counter (most synced passkeys) report 0 every
            // time.
            let count = asserted.sign_count;
            if (count != 0 || passkey.sign_count != 0) && count <= passkey.sign_count {
                // A copy of the key is about: whoever holds the sessions this passkey
                // opened may be the copy's holder, so they end. The challenge stays
                // spent, so the same answer cannot do it again. This is committed, and
                // the refusal returned after.
                sqlx::query("DELETE FROM auth_sessions WHERE passkey = $1")
                    .bind(&asserted.id)
                    .execute(&mut *connection)
                    .await?;
                return Ok(Err(BridgeError::new(
                    "signature_counter",
                    "this passkey's counter went backwards: it may have been copied",
                )));
            }
            sqlx::query(
                "UPDATE auth_passkeys SET sign_count = $1, last_used_at = $2 WHERE id = $3",
            )
            .bind(i64::from(count))
            .bind(secs(now))
            .bind(&asserted.id)
            .execute(&mut *connection)
            .await?;
            Ok(Ok(account))
        })?
    }

    /// Remove one of `account`'s passkeys; false when it has no such passkey. Its last one
    /// stays: removing it would lock the account out.
    #[allow(dead_code)] // the account page (32a-3)

    pub async fn remove_passkey(&self, account: &str, id: &[u8]) -> BridgeResult<bool> {
        transaction!(&self.store, |connection| {
            self.enter(connection, Lookup::Account(account)).await?;
            let held: i64 = sqlx::query("SELECT COUNT(*) FROM auth_passkeys WHERE account = $1")
                .bind(account)
                .fetch_one(&mut *connection)
                .await?
                .try_get(0)?;
            let theirs = sqlx::query("SELECT 1 FROM auth_passkeys WHERE id = $1 AND account = $2")
                .bind(id)
                .bind(account)
                .fetch_optional(&mut *connection)
                .await?
                .is_some();
            if !theirs {
                return Ok(false);
            }
            if held <= 1 {
                return Err(BridgeError::new(
                    "last_passkey",
                    "this is the account's only passkey; add another before removing it",
                )
                .into());
            }
            sqlx::query("DELETE FROM auth_passkeys WHERE id = $1 AND account = $2")
                .bind(id)
                .bind(account)
                .execute(&mut *connection)
                .await?;
            Ok(true)
        })
    }

    /// Store a passkey and, with `session`, open the session it signs in to, in the same
    /// transaction as everything else: the link spent and the passkey stored together with
    /// the session, or none of it.
    pub(in crate::identity::accounts) async fn put_passkey(
        &self,
        account: &str,
        credential: &NewCredential,
        label: &str,
        link: Option<(LinkPurpose, &str)>,
        signed_in: Option<&str>,
        now: u64,
        session: Option<[u8; 32]>,
    ) -> BridgeResult<Option<String>> {
        let label = label.trim();
        if label.is_empty() || label.len() > 64 || label.chars().any(char::is_control) {
            return Err(BridgeError::new(
                "invalid_passkey",
                "a passkey's name is 1 to 64 printable characters",
            ));
        }
        if self.clock_set_back(now).await? {
            return Err(invalid_ceremony());
        }
        transaction!(&self.store, |connection| {
            let (mac, issued) = self
                .check_challenge(
                    connection,
                    Ceremony::Register,
                    Some(account),
                    &credential.challenge,
                    now,
                )
                .await?;
            spend_challenge(connection, &mac, issued, now).await?;
            // Taken anywhere on the hub, whichever tenant holds it.
            let taken = self
                .store
                .tenant_of(connection, Lookup::Passkey(&credential.id))
                .await?
                .is_some();
            self.enter(connection, Lookup::Account(account)).await?;
            let holder = account_in(connection, account)
                .await?
                .ok_or_else(|| BridgeError::new("no_account", format!("no account {account}")))?;
            if holder.disabled_at.is_some() {
                return Err(account_disabled(account).into());
            }
            if let Some(session) = signed_in {
                let theirs = sqlx::query(
                        "SELECT 1 FROM auth_sessions WHERE hash = $1 AND account = $2 AND expires_at > $3",
                    )
                    .bind(hash(session))
                    .bind(account)
                    .bind(secs(now))
                    .fetch_optional(&mut *connection)
                    .await?
                    .is_some();
                if !theirs {
                    return Err(BridgeError::new(
                        "invalid_link",
                        "this browser is no longer signed in to that account",
                    )
                    .into());
                }
            }
            if let Some((purpose, token)) = link {
                let live = live_link(connection, purpose, token, now).await?;
                if live.account != account {
                    return Err(BridgeError::new(
                        "invalid_link",
                        "this link is for another account",
                    )
                    .into());
                }
                // A replacing link takes the account back: every other passkey goes, and
                // with them every session they opened.
                if live.replaces {
                    sqlx::query("DELETE FROM auth_passkeys WHERE account = $1")
                        .bind(account)
                        .execute(&mut *connection)
                        .await?;
                }
                // Spent only if still unspent: two uses racing cannot both count.
                let spent = sqlx::query(
                    "UPDATE auth_links SET used_at = $1 WHERE hash = $2 AND used_at IS NULL",
                )
                .bind(secs(now))
                .bind(hash(token))
                .execute(&mut *connection)
                .await?
                .rows_affected();
                if spent != 1 {
                    return Err(BridgeError::new(
                        "invalid_link",
                        "this link is used, expired or unknown",
                    )
                    .into());
                }
                // A link spent ends the account's other sessions: whoever had them, the
                // passkey registered now is how the account signs in from here.
                sqlx::query("DELETE FROM auth_sessions WHERE account = $1")
                    .bind(account)
                    .execute(&mut *connection)
                    .await?;
            }
            if taken {
                return Err(BridgeError::new(
                    "passkey_exists",
                    "this passkey is already registered",
                )
                .into());
            }
            sqlx::query(
                    "INSERT INTO auth_passkeys (id, account, label, public_key, alg, sign_count, backup_eligible, created_at, tenant_id)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
                )
                .bind(&credential.id)
                .bind(account)
                .bind(label)
                .bind(&credential.public_key)
                .bind(credential.alg.cose())
                .bind(i64::from(credential.sign_count))
                .bind(i64::from(credential.backup_eligible))
                .bind(secs(now))
                .bind(&holder.tenant)
                .execute(&mut *connection)
                .await?;
            // An account made on first connect is secured by its first passkey.
            tenants::secure_in(connection, &holder, "passkey", self.counting, now).await?;
            // The owner can sign in now: the hub is claimed, and no claim code it
            // printed works any more (`claim.rs`).
            if holder.owner {
                sqlx::query("DELETE FROM auth_claim_codes")
                    .execute(&mut *connection)
                    .await?;
            }
            match session {
                Some(random) => Ok(Some(
                    sessions::insert_session(
                        connection,
                        &holder.tenant,
                        account,
                        Some(&credential.id),
                        random,
                        now,
                    )
                    .await?,
                )),
                None => Ok(None),
            }
        })
    }
}
