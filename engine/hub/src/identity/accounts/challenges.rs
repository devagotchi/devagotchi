//! Passkey challenges: a time, a nonce and an HMAC under the store's key, handed out for nothing and recorded as spent only when a ceremony succeeds; a clock found set back renews the key.

use aws_lc_rs::hmac;
use devagotchi_contracts::{BridgeError, BridgeResult};
use sqlx::{AnyConnection, Row};

use super::{Accounts, CHALLENGE_TTL_SECS, Ceremony, invalid_ceremony};
use crate::{
    machines::{now_secs, random},
    store::{Failed, secs, transaction, unsecs},
};

/// A challenge: format (1), issued at (8, big endian), nonce (16), HMAC-SHA-256 (32).
pub(super) const CHALLENGE_LEN: usize = 57;
pub(super) const CHALLENGE_FORMAT: u8 = 1;

impl Accounts {
    /// A challenge for one ceremony: to register a passkey on `account`, or to sign in
    /// (`None`). Making one stores nothing.
    pub fn challenge(&self, purpose: Ceremony, account: Option<&str>) -> Vec<u8> {
        self.challenge_at(purpose, account, random::<16>(), now_secs())
    }

    pub(crate) fn challenge_at(
        &self,
        purpose: Ceremony,
        account: Option<&str>,
        nonce: [u8; 16],
        now: u64,
    ) -> Vec<u8> {
        let mut challenge = Vec::with_capacity(CHALLENGE_LEN);
        challenge.push(CHALLENGE_FORMAT);
        challenge.extend_from_slice(&now.to_be_bytes());
        challenge.extend_from_slice(&nonce);
        let tag = hmac::sign(
            &self.challenge_key.read().expect("challenge key"),
            &challenge_message(&challenge, purpose, account),
        );
        challenge.extend_from_slice(tag.as_ref());
        challenge
    }

    /// Check, inside the ceremony's transaction, that `challenge` is this hub's, for
    /// `purpose` (and `account`), and unexpired; returns its MAC, the key it is spent under,
    /// and when it was issued. The key is the one the store holds now, read in the same
    /// transaction, so a key another hub instance replaced is never trusted here; this one
    /// takes it up for the challenges it makes next. Whether the challenge is spent is
    /// checked when it is.
    pub(super) async fn check_challenge(
        &self,
        connection: &mut AnyConnection,
        purpose: Ceremony,
        account: Option<&str>,
        challenge: &[u8],
        now: u64,
    ) -> Result<(Vec<u8>, u64), Failed> {
        let key = stored_key(connection).await?;
        *self.challenge_key.write().expect("challenge key") = key.clone();
        if challenge.len() != CHALLENGE_LEN || challenge[0] != CHALLENGE_FORMAT {
            return Err(invalid_ceremony().into());
        }
        let (signed, tag) = challenge.split_at(CHALLENGE_LEN - 32);
        hmac::verify(&key, &challenge_message(signed, purpose, account), tag)
            .map_err(|_| invalid_ceremony())?;
        let issued = u64::from_be_bytes(signed[1..9].try_into().expect("8 bytes"));
        if issued > now || now >= issued.saturating_add(CHALLENGE_TTL_SECS) {
            return Err(invalid_ceremony().into());
        }
        Ok((tag.to_vec(), issued))
    }

    /// Whether the clock is now more than five minutes behind the horizon of cleared spent
    /// challenges: set back after running ahead. Then a challenge made before the step could
    /// be one whose record was cleared, and refusing all of them until the clock catches up
    /// could take hours. Instead the challenge key is replaced and the records and horizon
    /// start over, in one transaction that no ceremony can interleave with: every challenge
    /// made before fails once, every one made after works.
    pub(super) async fn clock_set_back(&self, now: u64) -> BridgeResult<bool> {
        let key = random::<32>();
        let replaced = transaction!(&self.store, |connection| {
            if now.saturating_add(CHALLENGE_TTL_SECS) > spent_horizon(connection).await? {
                return Ok(false);
            }
            sqlx::query("UPDATE hub_secrets SET value = $1 WHERE name = 'challenge_key'")
                .bind(key.to_vec())
                .execute(&mut *connection)
                .await?;
            sqlx::query("DELETE FROM hub_secrets WHERE name = 'spent_horizon'")
                .execute(&mut *connection)
                .await?;
            sqlx::query("DELETE FROM hub_spent_challenges")
                .execute(&mut *connection)
                .await?;
            Ok(true)
        })?;
        if replaced {
            *self.challenge_key.write().expect("challenge key") =
                hmac::Key::new(hmac::HMAC_SHA256, &key);
            eprintln!(
                "devagotchi-hub: the clock was set back (now {now}); passkey challenges made before were withdrawn"
            );
        }
        Ok(replaced)
    }
}

/// What a challenge's HMAC covers: its own bytes, the purpose, and the account.
pub(super) fn challenge_message(
    signed: &[u8],
    purpose: Ceremony,
    account: Option<&str>,
) -> Vec<u8> {
    let account = account.unwrap_or("");
    let mut message = Vec::with_capacity(64 + signed.len() + account.len());
    message.extend_from_slice(b"devagotchi passkey challenge\0");
    message.extend_from_slice(signed);
    message.extend_from_slice(purpose.as_str().as_bytes());
    message.push(0);
    message.extend_from_slice(account.as_bytes());
    message
}

/// Spend a challenge once, inside the ceremony's transaction. A record is kept until the
/// challenge expires (`issued` + five minutes). Clearing out expired records moves a
/// horizon forward to the latest expiry cleared, and a challenge expiring at or before it is
/// refused from then on: if the clock jumps ahead and back, a challenge whose record was
/// cleared while it looked expired cannot be used again.
pub(super) async fn spend_challenge(
    connection: &mut AnyConnection,
    mac: &[u8],
    issued: u64,
    now: u64,
) -> Result<(), Failed> {
    let expires = issued.saturating_add(CHALLENGE_TTL_SECS);
    let horizon = spent_horizon(connection).await?;
    if expires <= horizon {
        return Err(invalid_ceremony().into());
    }
    let cleared: Option<i64> =
        sqlx::query("SELECT MAX(expires_at) FROM hub_spent_challenges WHERE expires_at <= $1")
            .bind(secs(now))
            .fetch_one(&mut *connection)
            .await?
            .try_get(0)?;
    if let Some(cleared) = cleared.map(unsecs).filter(|cleared| *cleared > horizon) {
        sqlx::query(
            "INSERT INTO hub_secrets (name, value) VALUES ('spent_horizon', $1)
             ON CONFLICT (name) DO UPDATE SET value = excluded.value",
        )
        .bind(cleared.to_be_bytes().to_vec())
        .execute(&mut *connection)
        .await?;
        sqlx::query("DELETE FROM hub_spent_challenges WHERE expires_at <= $1")
            .bind(secs(cleared))
            .execute(&mut *connection)
            .await?;
    }
    let fresh = sqlx::query(
        "INSERT INTO hub_spent_challenges (mac, expires_at) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(mac)
    .bind(secs(expires))
    .execute(&mut *connection)
    .await?
    .rows_affected();
    if fresh == 1 {
        Ok(())
    } else {
        Err(invalid_ceremony().into())
    }
}

/// The challenge key the store holds: 32 bytes, or the store is damaged.
pub(super) async fn stored_key(connection: &mut AnyConnection) -> Result<hmac::Key, Failed> {
    let key: Vec<u8> = sqlx::query("SELECT value FROM hub_secrets WHERE name = 'challenge_key'")
        .fetch_one(&mut *connection)
        .await?
        .try_get(0)?;
    if key.len() != 32 {
        return Err(BridgeError::new(
            "account_store",
            "the challenge key in the store is not 32 bytes; restore the store from a backup",
        )
        .into());
    }
    Ok(hmac::Key::new(hmac::HMAC_SHA256, &key))
}

/// The latest expiry of the spent challenges cleared so far (0 before any). A value that is
/// not 8 bytes stops ceremonies rather than switching the protection off.
pub(super) async fn spent_horizon(connection: &mut AnyConnection) -> Result<u64, Failed> {
    let stored: Option<Vec<u8>> =
        sqlx::query("SELECT value FROM hub_secrets WHERE name = 'spent_horizon'")
            .fetch_optional(&mut *connection)
            .await?
            .map(|row| row.try_get(0))
            .transpose()?;
    match stored {
        None => Ok(0),
        Some(bytes) => bytes.try_into().map(u64::from_be_bytes).map_err(|_| {
            BridgeError::new(
                "account_store",
                "the spent-challenge horizon in the store is damaged; restore the store from a backup",
            )
            .into()
        }),
    }
}
