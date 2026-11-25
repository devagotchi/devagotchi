//! Who holds each owner's lexicon, kept in `kernel_lexicon_authority` (migration 0033), and
//! the moves that hand it over and take it back. The moves' rules are the kernel's
//! (`devagotchi_kernel::handoff`); this keeps where each owner stands and makes each move
//! atomic: the row is taken `FOR UPDATE`, checked against what the move read, and, when the
//! move brings entries (a handover's merge, Clyffy's mirror, a restore), the owner's entries
//! are replaced in the same transaction. Every lexicon write takes the row `FOR SHARE`
//! (`stored.rs`), so none lands after a move began.

use std::collections::BTreeSet;

use devagotchi_kernel::{
    handoff::{Authority, Holder, MAX_BUNDLE_ENTRIES},
    lexicon::{Entry, LexiconError, Result as Answer, State},
    words::normalize,
};
use sqlx::{AnyConnection, Row};

use super::stored::{
    CONFLICT, Stored, decode, encode, erase, from_bridge, into_failed, refused, scope_key,
    store_failed,
};
use crate::store::{Failed, transaction};

fn holder(text: &str) -> Result<Holder, LexiconError> {
    serde_json::from_value(serde_json::Value::String(text.to_string()))
        .map_err(|_| LexiconError::Store(format!("a stored holder: {text}")))
}

fn read(row: &sqlx::any::AnyRow) -> Result<Authority, LexiconError> {
    let held: String = row.try_get(0).map_err(store_failed)?;
    let epoch: i64 = row.try_get(1).map_err(store_failed)?;
    Ok(Authority {
        holder: holder(&held)?,
        epoch: u64::try_from(epoch).map_err(|_| LexiconError::Store("a stored epoch".into()))?,
    })
}

fn epoch(value: u64) -> Answer<i64> {
    i64::try_from(value).map_err(|_| LexiconError::Invalid("epoch out of range".into()))
}

/// The owner's authority inside a transaction, taken `FOR SHARE` (a write) or `FOR UPDATE`
/// (a move). An owner with no row gets one, held by this hub at epoch 1, so both lock it.
pub(super) async fn hold(
    connection: &mut AnyConnection,
    owner: &str,
    update: bool,
) -> Result<Authority, Failed> {
    sqlx::query(
        "INSERT INTO devagotchi.kernel_lexicon_authority (owner_id, holder, epoch)
         VALUES ($1, 'devagotchi', 1) ON CONFLICT (owner_id) DO NOTHING",
    )
    .bind(owner)
    .execute(&mut *connection)
    .await?;
    let query = if update {
        "SELECT holder, epoch FROM devagotchi.kernel_lexicon_authority
         WHERE owner_id = $1 FOR UPDATE"
    } else {
        "SELECT holder, epoch FROM devagotchi.kernel_lexicon_authority
         WHERE owner_id = $1 FOR SHARE"
    };
    let row = sqlx::query(query)
        .bind(owner)
        .fetch_one(&mut *connection)
        .await?;
    read(&row).map_err(into_failed)
}

/// One entry as its row is written.
struct Kept {
    id: String,
    kind: &'static str,
    project: String,
    phrase: String,
    state: State,
    revision: i64,
    json: String,
}

impl Stored {
    /// Where the owner's lexicon stands: this hub at epoch 1 until it first moves.
    pub async fn authority(&self, owner: &str) -> Answer<Authority> {
        let mut connection = self.store.read().await.map_err(from_bridge)?;
        let row = sqlx::query(
            "SELECT holder, epoch FROM devagotchi.kernel_lexicon_authority WHERE owner_id = $1",
        )
        .bind(owner)
        .fetch_optional(&mut *connection)
        .await
        .map_err(store_failed)?;
        row.as_ref()
            .map(read)
            .transpose()
            .map(Option::unwrap_or_default)
    }

    /// Every entry of the owner, in every scope, ordered by id.
    pub async fn entries_of(&self, owner: &str) -> Answer<Vec<Entry>> {
        let mut connection = self.store.read().await.map_err(from_bridge)?;
        let entries: Vec<String> = sqlx::query_scalar(
            "SELECT entry FROM devagotchi.kernel_lexicon_entries WHERE owner_id = $1
             ORDER BY id COLLATE \"C\" LIMIT $2",
        )
        .bind(owner)
        .bind(MAX_BUNDLE_ENTRIES as i64 + 1)
        .fetch_all(&mut *connection)
        .await
        .map_err(store_failed)?;
        entries.iter().map(|e| decode(e)).collect()
    }

    /// Moves the owner's lexicon from `from` (what the caller read) to `to`, and, with
    /// `entries`, makes them the owner's whole lexicon. Refused when it moved since `from`
    /// was read. An entry that leaves, or arrives deleted, has its words erased from its
    /// events and receipts too.
    pub async fn moved(
        &self,
        owner: &str,
        from: Authority,
        to: Authority,
        entries: Option<&[Entry]>,
    ) -> Answer<()> {
        let kept = entries
            .map(|entries| {
                entries
                    .iter()
                    .map(|entry| {
                        if entry.owner_id != owner {
                            return Err(LexiconError::Invalid("an entry of another owner".into()));
                        }
                        let (kind, project) = scope_key(&entry.scope);
                        Ok(Kept {
                            id: entry.id.clone(),
                            kind,
                            project: project.to_string(),
                            phrase: normalize(&entry.wording.phrase),
                            state: entry.state,
                            revision: i64::try_from(entry.revision).map_err(|_| {
                                LexiconError::Invalid("revision out of range".into())
                            })?,
                            json: encode(entry)?,
                        })
                    })
                    .collect::<Answer<Vec<_>>>()
            })
            .transpose()?;
        let (holder, raised) = (to.holder.as_str(), epoch(to.epoch)?);
        transaction!(&self.store, |connection| {
            let held = hold(connection, owner, true).await?;
            if held != from {
                return Err(refused(
                    CONFLICT,
                    format!(
                        "the lexicon moved: it is {} at epoch {}",
                        held.holder.as_str(),
                        held.epoch
                    ),
                ));
            }
            if let Some(kept) = &kept {
                replace(connection, owner, kept).await?;
            }
            sqlx::query(
                "UPDATE devagotchi.kernel_lexicon_authority SET holder = $2, epoch = $3
                 WHERE owner_id = $1",
            )
            .bind(owner)
            .bind(holder)
            .bind(raised)
            .execute(&mut *connection)
            .await?;
            Ok(())
        })
        .map_err(from_bridge)
    }
}

/// The owner's entries become exactly `kept`; the words of every entry that left, or came
/// back deleted, are erased from its history.
async fn replace(connection: &mut AnyConnection, owner: &str, kept: &[Kept]) -> Result<(), Failed> {
    let before: Vec<String> =
        sqlx::query_scalar("SELECT id FROM devagotchi.kernel_lexicon_entries WHERE owner_id = $1")
            .bind(owner)
            .fetch_all(&mut *connection)
            .await?;
    sqlx::query("DELETE FROM devagotchi.kernel_lexicon_entries WHERE owner_id = $1")
        .bind(owner)
        .execute(&mut *connection)
        .await?;
    for entry in kept {
        sqlx::query(
            "INSERT INTO devagotchi.kernel_lexicon_entries
               (owner_id, id, scope_kind, project_id, phrase_key, state, revision, entry)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(owner)
        .bind(&entry.id)
        .bind(entry.kind)
        .bind(&entry.project)
        .bind(&entry.phrase)
        .bind(entry.state.as_str())
        .bind(entry.revision)
        .bind(&entry.json)
        .execute(&mut *connection)
        .await?;
    }
    let staying: BTreeSet<&str> = kept
        .iter()
        .filter(|e| e.state != State::Deleted)
        .map(|e| e.id.as_str())
        .collect();
    let erased: BTreeSet<&str> = before
        .iter()
        .map(String::as_str)
        .chain(kept.iter().map(|e| e.id.as_str()))
        .filter(|id| !staying.contains(id))
        .collect();
    for id in erased {
        erase(connection, owner, id).await?;
    }
    Ok(())
}
