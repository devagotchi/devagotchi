//! Browser sessions of the Nomen web login that outlast a restart, so a phone that turned on
//! notifications stays signed in and a push opens straight onto Approve. Kept in the hub's
//! Postgres (`auth_web_sessions`, migration 0028).
//!
//! - A row is keyed by the SHA-256 of the browser's random session id. The browser holds
//!   the id; the hub never stores it.
//! - Nomen's tokens are sealed with AES-256-GCM under `session.key` (0600, made on first
//!   use, in the state directory) and bound to the row's key. The database on its own gives
//!   away no token, and a sealed blob moved to another row does not open.
//! - A session ends when its person signs out, after [`IDLE`] unused, at [`MAX`] however
//!   much it is used, or as soon as Nomen refuses to refresh it (`web.rs`).
//! - Times are Unix milliseconds.

use std::path::Path;

use aws_lc_rs::aead::{AES_256_GCM, Aad, LessSafeKey, NONCE_LEN, Nonce, UnboundKey};
use devagotchi_contracts::{BridgeError, BridgeResult};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::store::{Store, transaction};

/// A session nobody has used for this long ends.
pub const IDLE: u64 = 30 * 24 * 3600 * 1000;
/// No session lasts longer than this, however often it is used.
pub const MAX: u64 = 90 * 24 * 3600 * 1000;
const MAX_ROWS: i64 = 10_000;
const KEY_FILE: &str = "session.key";

/// What the hub holds for a session: Nomen's access token and, when Nomen gave one, the
/// refresh token that renews it.
#[derive(Clone, Serialize, Deserialize)]
pub struct Held {
    pub access: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh: Option<String>,
}

pub struct Stored {
    pub held: Held,
    /// When the access token stops being accepted.
    pub access_until: u64,
    pub last_seen: u64,
}

pub struct SessionStore {
    store: Store,
    key: LessSafeKey,
}

pub fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

fn millis(time: u64) -> i64 {
    i64::try_from(time).unwrap_or(i64::MAX)
}

impl SessionStore {
    /// The sessions in `store`, sealed under the key in `state_dir` (made on first use, 0600).
    pub fn open(state_dir: &Path, store: Store) -> BridgeResult<Self> {
        let key_path = state_dir.join(KEY_FILE);
        let key = match std::fs::read(&key_path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let bytes = crate::machines::random::<32>().to_vec();
                crate::notify::push::write_private(&key_path, &bytes)?;
                bytes
            }
            Err(error) => {
                return Err(BridgeError::new(
                    "state_file",
                    format!("{}: {error}", key_path.display()),
                ));
            }
        };
        let key = UnboundKey::new(&AES_256_GCM, &key).map_err(|_| {
            BridgeError::new(
                "state_file",
                format!("{} is not a 32-byte key", key_path.display()),
            )
        })?;
        Ok(Self {
            store,
            key: LessSafeKey::new(key),
        })
    }

    fn seal(&self, id_hash: &[u8; 32], held: &Held) -> BridgeResult<Vec<u8>> {
        let nonce = crate::machines::random::<NONCE_LEN>();
        let mut body = serde_json::to_vec(held).expect("tokens serialize");
        self.key
            .seal_in_place_append_tag(
                Nonce::assume_unique_for_key(nonce),
                Aad::from(id_hash),
                &mut body,
            )
            .map_err(|_| BridgeError::new("session_store", "could not seal a session"))?;
        let mut sealed = nonce.to_vec();
        sealed.extend_from_slice(&body);
        Ok(sealed)
    }

    fn unseal(&self, id_hash: &[u8; 32], sealed: &[u8]) -> Option<Held> {
        if sealed.len() <= NONCE_LEN {
            return None;
        }
        let (nonce, body) = sealed.split_at(NONCE_LEN);
        let nonce = Nonce::try_assume_unique_for_key(nonce).ok()?;
        let mut body = body.to_vec();
        let plain = self
            .key
            .open_in_place(nonce, Aad::from(id_hash), &mut body)
            .ok()?;
        serde_json::from_slice(plain).ok()
    }

    /// Start a session that ends at `expires_at` at the latest. Ended sessions are swept
    /// first, and past [`MAX_ROWS`] the least recently used one makes room.
    pub async fn put(
        &self,
        id_hash: &[u8; 32],
        held: &Held,
        access_until: u64,
        expires_at: u64,
        now: u64,
    ) -> BridgeResult<()> {
        let sealed = self.seal(id_hash, held)?;
        transaction!(&self.store, |connection| {
            sqlx::query(
                "DELETE FROM auth_web_sessions WHERE expires_at <= $1 OR last_seen + $2 <= $1",
            )
            .bind(millis(now))
            .bind(millis(IDLE))
            .execute(&mut *connection)
            .await?;
            sqlx::query(
                "DELETE FROM auth_web_sessions WHERE id_hash IN (SELECT id_hash FROM auth_web_sessions
                   ORDER BY last_seen LIMIT GREATEST(0, (SELECT count(*) FROM auth_web_sessions) - $1 + 1))",
            )
            .bind(MAX_ROWS)
            .execute(&mut *connection)
            .await?;
            sqlx::query(
                "INSERT INTO auth_web_sessions
                   (id_hash, sealed, access_until, created_at, last_seen, expires_at)
                 VALUES ($1, $2, $3, $4, $4, $5)
                 ON CONFLICT (id_hash) DO UPDATE SET sealed = excluded.sealed,
                   access_until = excluded.access_until, created_at = excluded.created_at,
                   last_seen = excluded.last_seen, expires_at = excluded.expires_at",
            )
            .bind(id_hash.to_vec())
            .bind(sealed.clone())
            .bind(millis(access_until))
            .bind(millis(now))
            .bind(millis(expires_at))
            .execute(&mut *connection)
            .await?;
            Ok(())
        })
    }

    /// The session under `id_hash` while it lasts. One that ended, or whose seal does
    /// not open, is removed and reads as none.
    pub async fn get(&self, id_hash: &[u8; 32], now: u64) -> BridgeResult<Option<Stored>> {
        let row = transaction!(&self.store, |connection| {
            let row = sqlx::query(
                "SELECT sealed, access_until, last_seen, expires_at FROM auth_web_sessions
                 WHERE id_hash = $1",
            )
            .bind(id_hash.to_vec())
            .fetch_optional(&mut *connection)
            .await?;
            let Some(row) = row else {
                return Ok(None);
            };
            let sealed: Vec<u8> = row.try_get("sealed")?;
            let at = |column: &str| row.try_get::<i64, _>(column).map(|v| v.max(0) as u64);
            Ok(Some((
                sealed,
                at("access_until")?,
                at("last_seen")?,
                at("expires_at")?,
            )))
        })?;
        let Some((sealed, access_until, last_seen, expires_at)) = row else {
            return Ok(None);
        };
        let ended = expires_at <= now || last_seen + IDLE <= now;
        let held = if ended {
            None
        } else {
            self.unseal(id_hash, &sealed)
        };
        let Some(held) = held else {
            self.delete(id_hash).await?;
            return Ok(None);
        };
        Ok(Some(Stored {
            held,
            access_until,
            last_seen,
        }))
    }

    async fn delete(&self, id_hash: &[u8; 32]) -> BridgeResult<()> {
        transaction!(&self.store, |connection| {
            sqlx::query("DELETE FROM auth_web_sessions WHERE id_hash = $1")
                .bind(id_hash.to_vec())
                .execute(&mut *connection)
                .await?;
            Ok(())
        })
    }

    /// Record a use, which keeps the session from going idle.
    pub async fn touch(&self, id_hash: &[u8; 32], now: u64) -> BridgeResult<()> {
        transaction!(&self.store, |connection| {
            sqlx::query("UPDATE auth_web_sessions SET last_seen = $2 WHERE id_hash = $1")
                .bind(id_hash.to_vec())
                .bind(millis(now))
                .execute(&mut *connection)
                .await?;
            Ok(())
        })
    }

    /// Keep the tokens a refresh returned (Nomen rotates the refresh token).
    pub async fn renew(
        &self,
        id_hash: &[u8; 32],
        held: &Held,
        access_until: u64,
        now: u64,
    ) -> BridgeResult<()> {
        let sealed = self.seal(id_hash, held)?;
        transaction!(&self.store, |connection| {
            sqlx::query(
                "UPDATE auth_web_sessions SET sealed = $2, access_until = $3, last_seen = $4
                 WHERE id_hash = $1",
            )
            .bind(id_hash.to_vec())
            .bind(sealed.clone())
            .bind(millis(access_until))
            .bind(millis(now))
            .execute(&mut *connection)
            .await?;
            Ok(())
        })
    }

    /// End the session; returns what it held, so the refresh token can be revoked.
    pub async fn remove(&self, id_hash: &[u8; 32]) -> Option<Held> {
        let sealed = transaction!(&self.store, |connection| {
            let sealed: Option<Vec<u8>> = sqlx::query_scalar(
                "DELETE FROM auth_web_sessions WHERE id_hash = $1 RETURNING sealed",
            )
            .bind(id_hash.to_vec())
            .fetch_optional(&mut *connection)
            .await?;
            Ok(sealed)
        })
        .ok()
        .flatten();
        sealed.and_then(|sealed| self.unseal(id_hash, &sealed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn held(access: &str, refresh: Option<&str>) -> Held {
        Held {
            access: access.into(),
            refresh: refresh.map(str::to_string),
        }
    }

    #[tokio::test]
    async fn a_session_survives_reopening_the_store_and_ends_idle_or_at_its_limit() {
        let test = or_skip!(crate::store::testing::postgres());
        let dir = tempfile::tempdir().unwrap();
        let (a, b, c) = ([1u8; 32], [2u8; 32], [3u8; 32]);
        let t = 1_000_000_000;
        {
            let store = SessionStore::open(dir.path(), test.store.clone()).unwrap();
            let long = t + MAX;
            store
                .put(&a, &held("acc-a", Some("ref-a")), t + 1000, long, t)
                .await
                .unwrap();
            store
                .put(&b, &held("acc-b", None), t + 1000, long, t)
                .await
                .unwrap();
            store
                .put(&c, &held("acc-c", Some("ref-c")), t + 1000, t + 5000, t)
                .await
                .unwrap();
        }
        // A restart: a new store over the same database and key reads the same sessions.
        let store = SessionStore::open(dir.path(), test.store.clone()).unwrap();
        let got = store.get(&a, t + 10).await.unwrap().unwrap();
        assert_eq!(got.held.access, "acc-a");
        assert_eq!(got.held.refresh.as_deref(), Some("ref-a"));
        assert_eq!(got.access_until, t + 1000);
        assert!(
            store.get(&c, t + 5000).await.unwrap().is_none(),
            "past its limit"
        );
        assert!(
            store.get(&c, t + 10).await.unwrap().is_none(),
            "and gone for good"
        );
        store.touch(&a, t + IDLE - 1).await.unwrap();
        assert!(
            store.get(&a, t + IDLE + 10).await.unwrap().is_some(),
            "used, so not idle"
        );
        assert!(
            store.get(&b, t + IDLE).await.unwrap().is_none(),
            "unused for IDLE ends"
        );
        let renewed = held("acc-a2", Some("ref-a2"));
        store
            .renew(&a, &renewed, t + IDLE + 5000, t + IDLE + 20)
            .await
            .unwrap();
        let renewed = store.get(&a, t + IDLE + 30).await.unwrap().unwrap();
        assert_eq!(renewed.held.access, "acc-a2");
        assert_eq!(renewed.held.refresh.as_deref(), Some("ref-a2"));
        let removed = store.remove(&a).await.unwrap();
        assert_eq!(removed.refresh.as_deref(), Some("ref-a2"));
        assert!(store.get(&a, t + IDLE + 40).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn tokens_are_sealed_at_rest_and_bound_to_their_row() {
        let test = or_skip!(crate::store::testing::postgres());
        let dir = tempfile::tempdir().unwrap();
        let store = SessionStore::open(dir.path(), test.store.clone()).unwrap();
        let (a, b) = ([7u8; 32], [8u8; 32]);
        let t = now_millis();
        let secret = held("ACCESS-SECRET-A", Some("REFRESH-SECRET-A"));
        store
            .put(&a, &secret, t + 60_000, t + MAX, t)
            .await
            .unwrap();
        store
            .put(&b, &held("ACCESS-SECRET-B", None), t + 60_000, t + MAX, t)
            .await
            .unwrap();
        let rows: Vec<Vec<u8>> =
            sqlx::query_scalar("SELECT sealed FROM devagotchi.auth_web_sessions")
                .fetch_all(test.store.admin())
                .await
                .unwrap();
        assert!(
            rows.iter()
                .all(|row| !String::from_utf8_lossy(row).contains("SECRET")),
            "a token is kept in the clear"
        );
        // Move a's sealed blob onto b's row: it does not open there, and b is dropped.
        sqlx::query(
            "UPDATE devagotchi.auth_web_sessions SET sealed =
               (SELECT sealed FROM devagotchi.auth_web_sessions WHERE id_hash = $1) WHERE id_hash = $2",
        )
        .bind(a.to_vec())
        .bind(b.to_vec())
        .execute(test.store.admin())
        .await
        .unwrap();
        assert!(store.get(&b, t + 1).await.unwrap().is_none());
        let opened = store.get(&a, t + 1).await.unwrap().unwrap();
        assert_eq!(opened.held.access, "ACCESS-SECRET-A");
        // Another key cannot open the store's sessions.
        std::fs::remove_file(dir.path().join(KEY_FILE)).unwrap();
        crate::notify::push::write_private(&dir.path().join(KEY_FILE), &[9u8; 32]).unwrap();
        let rekeyed = SessionStore::open(dir.path(), test.store.clone()).unwrap();
        assert!(rekeyed.get(&a, t + 1).await.unwrap().is_none());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.path().join(KEY_FILE))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "{KEY_FILE}");
        }
    }
}
