//! The kernel's lexicon [`Store`] on the hub's Postgres (migration 0032): entries, their
//! events and receipts, every query scoped to one owner. The rules that move an entry are the
//! kernel's; this keeps what it decides, with the database holding the two rules a racing
//! write could break (the revision a change read, one live entry per phrase in a scope), and
//! refusing every write while another side holds the lexicon (`authority.rs`).

use devagotchi_contracts::BridgeError;
use devagotchi_kernel::{
    lexicon::{
        Entry, Event, LexiconError, MAX_ENTRIES, Receipt, Result as Answer, Scope, State, Store,
        Write,
    },
    words::normalize,
};
use sqlx::{AnyConnection, Row};

use crate::store::{Failed, Store as Postgres, transaction};

pub(super) const CONFLICT: &str = "lexicon_conflict";
const NOT_FOUND: &str = "lexicon_not_found";

/// The lexicon's tables in the hub's store.
#[derive(Clone, Debug)]
pub struct Stored {
    pub(super) store: Postgres,
}

impl Stored {
    pub fn new(store: Postgres) -> Self {
        Self { store }
    }
}

/// How a scope is kept: its kind, and its project (empty for the person's own).
pub(super) fn scope_key(scope: &Scope) -> (&'static str, &str) {
    match scope {
        Scope::Personal => ("personal", ""),
        Scope::Project { project_id } => ("project", project_id.as_str()),
    }
}

fn revision(value: u64) -> Answer<i64> {
    i64::try_from(value).map_err(|_| LexiconError::Invalid("revision out of range".into()))
}

pub(super) fn encode<T: serde::Serialize>(value: &T) -> Answer<String> {
    serde_json::to_string(value).map_err(|e| LexiconError::Store(format!("encode: {e}")))
}

pub(super) fn decode<T: serde::de::DeserializeOwned>(text: &str) -> Answer<T> {
    serde_json::from_str(text).map_err(|e| LexiconError::Store(format!("a stored record: {e}")))
}

pub(super) fn store_failed(error: sqlx::Error) -> LexiconError {
    LexiconError::Store(error.to_string())
}

/// A refusal inside a transaction, as the kernel names it on the way out.
pub(super) fn refused(code: &str, message: impl Into<String>) -> Failed {
    Failed::Refused(BridgeError::new(code, message))
}

pub(super) fn from_bridge(error: BridgeError) -> LexiconError {
    match error.code.as_str() {
        CONFLICT => LexiconError::Conflict(error.message),
        NOT_FOUND => LexiconError::NotFound,
        _ => LexiconError::Store(error.message),
    }
}

pub(super) fn into_failed(error: LexiconError) -> Failed {
    match error {
        LexiconError::Conflict(why) => refused(CONFLICT, why),
        LexiconError::NotFound => refused(NOT_FOUND, "no such entry"),
        other => refused("lexicon_store", other.to_string()),
    }
}

impl Store for Stored {
    async fn receipt(&self, owner: &str, request_id: &str) -> Answer<Option<Receipt>> {
        let mut connection = self.store.read().await.map_err(from_bridge)?;
        let row = sqlx::query(
            "SELECT payload_hash, entry FROM devagotchi.kernel_lexicon_receipts
             WHERE owner_id = $1 AND request_id = $2",
        )
        .bind(owner)
        .bind(request_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(store_failed)?;
        match row {
            None => Ok(None),
            Some(row) => Ok(Some(Receipt {
                request_id: request_id.to_string(),
                payload_hash: row.try_get(0).map_err(store_failed)?,
                entry: decode(&row.try_get::<String, _>(1).map_err(store_failed)?)?,
            })),
        }
    }

    async fn get(&self, owner: &str, id: &str) -> Answer<Option<Entry>> {
        let mut connection = self.store.read().await.map_err(from_bridge)?;
        let entry: Option<String> = sqlx::query_scalar(
            "SELECT entry FROM devagotchi.kernel_lexicon_entries WHERE owner_id = $1 AND id = $2",
        )
        .bind(owner)
        .bind(id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(store_failed)?;
        entry.as_deref().map(decode).transpose()
    }

    async fn list(&self, owner: &str, scope: &Scope) -> Answer<Vec<Entry>> {
        let (kind, project) = scope_key(scope);
        let mut connection = self.store.read().await.map_err(from_bridge)?;
        // Byte order, as the kernel's reference store sorts: not the database's collation.
        let entries: Vec<String> = sqlx::query_scalar(
            "SELECT entry FROM devagotchi.kernel_lexicon_entries
             WHERE owner_id = $1 AND scope_kind = $2 AND project_id = $3
             ORDER BY phrase_key COLLATE \"C\", id COLLATE \"C\" LIMIT $4",
        )
        .bind(owner)
        .bind(kind)
        .bind(project)
        .bind(MAX_ENTRIES as i64)
        .fetch_all(&mut *connection)
        .await
        .map_err(store_failed)?;
        entries.iter().map(|e| decode(e)).collect()
    }

    async fn commit(&self, write: Write) -> Answer<Event> {
        let entry_json = encode(&write.entry)?;
        let receipt_json = encode(&write.receipt.entry)?;
        let new_revision = revision(write.entry.revision)?;
        let expected = write.expected.map(revision).transpose()?;
        let (kind, project) = scope_key(&write.entry.scope);
        let phrase = normalize(&write.entry.wording.phrase);
        let owner = write.entry.owner_id.as_str();
        let id = write.entry.id.as_str();
        let state = write.entry.state;
        let event_json = encode(&Event {
            seq: 0,
            ..write.event.clone()
        })?;
        let seq = transaction!(&self.store, |connection| {
            // Only while this hub holds the lexicon, and never after a handover began.
            let holds = super::authority::hold(connection, owner, false).await?;
            holds.fence(holds.epoch).map_err(into_failed)?;
            let used: Option<String> = sqlx::query_scalar(
                "SELECT request_id FROM devagotchi.kernel_lexicon_receipts
                 WHERE owner_id = $1 AND request_id = $2",
            )
            .bind(owner)
            .bind(&write.receipt.request_id)
            .fetch_optional(&mut *connection)
            .await?;
            if used.is_some() {
                return Err(refused(CONFLICT, "the request id was used"));
            }
            let held: Option<i64> = sqlx::query_scalar(
                "SELECT revision FROM devagotchi.kernel_lexicon_entries
                 WHERE owner_id = $1 AND id = $2 FOR UPDATE",
            )
            .bind(owner)
            .bind(id)
            .fetch_optional(&mut *connection)
            .await?;
            match (held, expected) {
                (None, None) => {}
                (Some(old), Some(expected)) if old == expected => {}
                (Some(_), None) => return Err(refused(CONFLICT, "the entry id is taken")),
                (None, Some(_)) => return Err(refused(NOT_FOUND, "no such entry")),
                (Some(old), Some(expected)) => {
                    return Err(refused(
                        CONFLICT,
                        format!("the entry is at revision {old}, not {expected}"),
                    ));
                }
            }
            if state.live() {
                let taken: Option<String> = sqlx::query_scalar(
                    "SELECT id FROM devagotchi.kernel_lexicon_entries
                     WHERE owner_id = $1 AND scope_kind = $2 AND project_id = $3
                       AND phrase_key = $4 AND id <> $5
                       AND state IN ('proposed', 'confirmed', 'inconclusive')
                     LIMIT 1",
                )
                .bind(owner)
                .bind(kind)
                .bind(project)
                .bind(&phrase)
                .bind(id)
                .fetch_optional(&mut *connection)
                .await?;
                if taken.is_some() {
                    return Err(refused(
                        CONFLICT,
                        "another live entry has this phrase in this scope",
                    ));
                }
            }
            if state == State::Deleted {
                erase(connection, owner, id).await?;
            }
            sqlx::query(
                "INSERT INTO devagotchi.kernel_lexicon_entries
                   (owner_id, id, scope_kind, project_id, phrase_key, state, revision, entry)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                 ON CONFLICT (owner_id, id) DO UPDATE SET phrase_key = EXCLUDED.phrase_key,
                   state = EXCLUDED.state, revision = EXCLUDED.revision, entry = EXCLUDED.entry",
            )
            .bind(owner)
            .bind(id)
            .bind(kind)
            .bind(project)
            .bind(&phrase)
            .bind(state.as_str())
            .bind(new_revision)
            .bind(&entry_json)
            .execute(&mut *connection)
            .await?;
            let seq: i64 = sqlx::query_scalar(
                "INSERT INTO devagotchi.kernel_lexicon_events (owner_id, entry_id, event)
                 VALUES ($1, $2, $3) RETURNING seq",
            )
            .bind(owner)
            .bind(id)
            .bind(&event_json)
            .fetch_one(&mut *connection)
            .await?;
            sqlx::query(
                "INSERT INTO devagotchi.kernel_lexicon_receipts
                   (owner_id, request_id, entry_id, payload_hash, entry)
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(owner)
            .bind(&write.receipt.request_id)
            .bind(id)
            .bind(&write.receipt.payload_hash)
            .bind(&receipt_json)
            .execute(&mut *connection)
            .await?;
            Ok(seq)
        })
        .map_err(from_bridge)?;
        Ok(Event {
            seq: u64::try_from(seq).unwrap_or_default(),
            ..write.event
        })
    }

    async fn history(&self, owner: &str, id: &str) -> Answer<Vec<Event>> {
        let mut connection = self.store.read().await.map_err(from_bridge)?;
        let rows = sqlx::query(
            "SELECT seq, event FROM devagotchi.kernel_lexicon_events
             WHERE owner_id = $1 AND entry_id = $2 ORDER BY seq",
        )
        .bind(owner)
        .bind(id)
        .fetch_all(&mut *connection)
        .await
        .map_err(store_failed)?;
        rows.iter()
            .map(|row| {
                let seq: i64 = row.try_get(0).map_err(store_failed)?;
                let event: Event = decode(&row.try_get::<String, _>(1).map_err(store_failed)?)?;
                Ok(Event {
                    seq: u64::try_from(seq).unwrap_or_default(),
                    ..event
                })
            })
            .collect()
    }
}

/// A deleted entry leaves no text: its earlier events and receipts keep their ids and
/// revisions, their words erased. Inside the commit's transaction.
pub(super) async fn erase(
    connection: &mut AnyConnection,
    owner: &str,
    id: &str,
) -> Result<(), Failed> {
    let events = sqlx::query(
        "SELECT seq, event FROM devagotchi.kernel_lexicon_events
         WHERE owner_id = $1 AND entry_id = $2",
    )
    .bind(owner)
    .bind(id)
    .fetch_all(&mut *connection)
    .await?;
    for row in events {
        let seq: i64 = row.try_get(0)?;
        let mut event: Event = decode(&row.try_get::<String, _>(1)?).map_err(into_failed)?;
        event.entry.wording = Default::default();
        sqlx::query("UPDATE devagotchi.kernel_lexicon_events SET event = $1 WHERE seq = $2")
            .bind(encode(&event).map_err(into_failed)?)
            .bind(seq)
            .execute(&mut *connection)
            .await?;
    }
    let receipts = sqlx::query(
        "SELECT request_id, entry FROM devagotchi.kernel_lexicon_receipts
         WHERE owner_id = $1 AND entry_id = $2",
    )
    .bind(owner)
    .bind(id)
    .fetch_all(&mut *connection)
    .await?;
    for row in receipts {
        let request_id: String = row.try_get(0)?;
        let mut entry: Entry = decode(&row.try_get::<String, _>(1)?).map_err(into_failed)?;
        entry.wording = Default::default();
        sqlx::query(
            "UPDATE devagotchi.kernel_lexicon_receipts SET entry = $1
             WHERE owner_id = $2 AND request_id = $3",
        )
        .bind(encode(&entry).map_err(into_failed)?)
        .bind(owner)
        .bind(request_id)
        .execute(&mut *connection)
        .await?;
    }
    Ok(())
}
