//! Sign-ins Nomen confirmed, kept in the hub's Postgres (`auth_nomen_sign_ins`, migration
//! 0028) so a hub restart during a Nomen outage does not sign everyone out (increment 27b). A
//! row holds the SHA-256 of the token, never the token, with the account it proved and until
//! when it may be honoured: the token's own expiry, at most 12 hours after Nomen last
//! confirmed it. The hub reads the rows only while Nomen cannot be reached; when Nomen
//! answers, its answer decides, and an answer that the token is no longer good deletes the
//! row. A store that fails is logged and ignored: sign-in never depends on it.

use devagotchi_contracts::BridgeResult;
use sqlx::Row;

use crate::{
    identity::auth::Subject,
    store::{Store, secs, transaction, unsecs},
};

/// One confirmed sign-in, as kept.
pub(super) struct Kept {
    pub digest: [u8; 32],
    pub subject: Subject,
    /// Unix seconds Nomen last confirmed it, and until when it may be honoured.
    pub confirmed_at: u64,
    pub honour_until: u64,
}

pub(super) struct SignIns {
    store: Store,
}

impl SignIns {
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// Every sign-in still honourable at `now`; lapsed rows are deleted first.
    pub async fn load(&self, now: u64) -> BridgeResult<Vec<Kept>> {
        transaction!(&self.store, |connection| {
            sqlx::query("DELETE FROM auth_nomen_sign_ins WHERE honour_until <= $1")
                .bind(secs(now))
                .execute(&mut *connection)
                .await?;
            let rows = sqlx::query(
                "SELECT digest, subject, name, org, confirmed_at, honour_until
                 FROM auth_nomen_sign_ins",
            )
            .fetch_all(&mut *connection)
            .await?;
            let mut kept = Vec::new();
            for row in &rows {
                let digest: Vec<u8> = row.try_get("digest")?;
                let Ok(digest) = <[u8; 32]>::try_from(digest.as_slice()) else {
                    continue;
                };
                kept.push(Kept {
                    digest,
                    subject: Subject {
                        id: row.try_get("subject")?,
                        via: "nomen",
                        tenant: None,
                        name: row.try_get("name")?,
                        org: row.try_get("org")?,
                        provisional_until: None,
                        agent: None,
                        claim_code: None,
                        client: None,
                        provisional_warning: false,
                        access: None,
                    },
                    confirmed_at: unsecs(row.try_get("confirmed_at")?),
                    honour_until: unsecs(row.try_get("honour_until")?),
                });
            }
            Ok(kept)
        })
    }

    /// Keep (or refresh) a confirmation; lapsed rows go at the same time.
    pub async fn keep(&self, kept: &Kept) -> BridgeResult<()> {
        transaction!(&self.store, |connection| {
            sqlx::query(
                "INSERT INTO auth_nomen_sign_ins
                   (digest, subject, name, org, confirmed_at, honour_until)
                 VALUES ($1, $2, $3, $4, $5, $6)
                 ON CONFLICT (digest) DO UPDATE SET subject = excluded.subject,
                   name = excluded.name, org = excluded.org,
                   confirmed_at = excluded.confirmed_at, honour_until = excluded.honour_until",
            )
            .bind(kept.digest.to_vec())
            .bind(&kept.subject.id)
            .bind(kept.subject.name.clone())
            .bind(kept.subject.org.clone())
            .bind(secs(kept.confirmed_at))
            .bind(secs(kept.honour_until))
            .execute(&mut *connection)
            .await?;
            sqlx::query("DELETE FROM auth_nomen_sign_ins WHERE honour_until <= $1")
                .bind(secs(kept.confirmed_at))
                .execute(&mut *connection)
                .await?;
            Ok(())
        })
    }

    /// Nomen said this token is no longer good: never honour it again.
    pub async fn forget(&self, digest: &[u8; 32]) -> BridgeResult<()> {
        transaction!(&self.store, |connection| {
            sqlx::query("DELETE FROM auth_nomen_sign_ins WHERE digest = $1")
                .bind(digest.to_vec())
                .execute(&mut *connection)
                .await?;
            Ok(())
        })
    }
}
