//! Notes that reach who they are addressed to. A note with a `to` (a seat, or an AI by name:
//! `claude`, `codex`, `grok`, `hermes` …) is delivered once, complete, the next time that
//! recipient calls any tool (`Hub::inbox_banner`), or at once to its webhook (`wake.rs`). The
//! data moves one time; after that the recipient has it. Until it marks them read
//! (`board_inbox`), it gets a short reminder at most every `REMIND_EVERY_MS`, naming how many
//! and carrying no text.
//!
//! The position of each recipient is one small JSON document in the board's settings store
//! (`inbox`), beside the notes and in the same transaction as reading them. Notes are
//! append-only with a rising `seq`, so a position is two numbers.

use std::collections::BTreeMap;

use devagotchi_contracts::BridgeResult;
use rusqlite::{OptionalExtension, params};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{Actor, Board, Note, invalid, note_from_row, store_error};

/// New notes one delivery carries at most; the rest come with the next.
pub const MAX_NOTES: usize = 5;
/// Characters of note text one delivery carries at most.
pub const MAX_CHARS: usize = 12_000;
/// Longest single note carried whole; a longer one says where the rest is.
pub const MAX_BODY: usize = 6_000;
/// A recipient that has everything but has not marked it read hears about it this often.
pub const REMIND_EVERY_MS: i64 = 15 * 60 * 1000;
/// Addressed notes one read looks at.
pub(super) const SCAN: i64 = 500;
pub(super) const KEY: &str = "inbox";

/// Where one recipient is up to on one board.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Cursor {
    /// The newest note it has been handed.
    pub delivered: i64,
    /// The newest note it marked read.
    pub acked: i64,
    pub reminded_at: i64,
}

pub type Cursors = BTreeMap<String, Cursor>;

/// One note, as its recipient gets it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct InboxNote {
    pub seq: i64,
    pub ticket: String,
    pub from: String,
    pub kind: String,
    pub to: String,
    pub body: String,
    /// Milliseconds since the epoch.
    pub at: i64,
}

impl From<&Note> for InboxNote {
    fn from(note: &Note) -> Self {
        Self {
            seq: note.seq,
            ticket: note.ticket.clone(),
            from: note.author.clone(),
            kind: note.kind.clone(),
            to: note.to_principal.clone().unwrap_or_default(),
            body: note.body.clone(),
            at: note.created_at,
        }
    }
}

/// What one recipient should hear now from one board.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Delivery {
    /// New notes, complete.
    pub notes: Vec<InboxNote>,
    /// New notes that did not fit and come next time.
    pub more: usize,
    /// Notes it already has and has not marked read.
    pub unread: usize,
    /// The newest unread note, what marking read names.
    pub through: i64,
    /// Time to say so again.
    pub remind: bool,
}

impl Delivery {
    pub fn is_quiet(&self) -> bool {
        self.notes.is_empty() && !self.remind
    }
}

/// The names an AI client answers to besides its seat: its family (`claude`, `chatgpt`,
/// `grok`, `cursor`), `codex` for Codex, and the name it registered under, so a custom agent
/// that calls itself Hermes is reached as `hermes`.
pub fn client_aliases(name: &str) -> Vec<String> {
    let mut aliases = Vec::new();
    let family = crate::analytics::funnel::client_family(name);
    if family != "other" {
        aliases.push(family.to_string());
    }
    let lower = name.to_ascii_lowercase();
    if lower.contains("codex") {
        aliases.push("codex".to_string());
    }
    let own: String = lower
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if !own.is_empty() && !aliases.contains(&own) {
        aliases.push(own);
    }
    for alias in devagotchi_connectors::aliases_for_client(name) {
        if !aliases.contains(&alias) {
            aliases.push(alias);
        }
    }
    aliases
}

fn wanted<'a>(notes: &'a [Note], who: &'a [String], me: &'a str) -> impl Iterator<Item = &'a Note> {
    notes.iter().filter(move |note| {
        note.author != me
            && note
                .to_principal
                .as_ref()
                .is_some_and(|to| who.contains(to))
    })
}

fn cursor_of(cursors: &Cursors, to: Option<&String>) -> Cursor {
    to.and_then(|to| cursors.get(to))
        .cloned()
        .unwrap_or_default()
}

/// The earliest note any of `who` could still care about.
pub(super) fn floor(cursors: &Cursors, who: &[String]) -> i64 {
    who.iter()
        .map(|name| cursors.get(name).map_or(0, |cursor| cursor.acked))
        .min()
        .unwrap_or(0)
}

/// What to tell `who` now, moving their positions as if it was told.
pub(super) fn plan(
    notes: &[Note],
    who: &[String],
    me: &str,
    cursors: &mut Cursors,
    now: i64,
) -> Delivery {
    let mut delivery = Delivery::default();
    let mut chars = 0;
    let mut already = 0;
    for note in wanted(notes, who, me) {
        let cursor = cursor_of(cursors, note.to_principal.as_ref());
        if note.seq <= cursor.acked {
            continue;
        }
        delivery.through = delivery.through.max(note.seq);
        if note.seq <= cursor.delivered {
            already += 1;
            continue;
        }
        let size = note.body.chars().count().min(MAX_BODY);
        if delivery.notes.len() >= MAX_NOTES
            || (!delivery.notes.is_empty() && chars + size > MAX_CHARS)
        {
            delivery.more += 1;
            continue;
        }
        chars += size;
        delivery.notes.push(InboxNote::from(note));
    }
    delivery.unread = already;
    for note in &delivery.notes {
        let cursor = cursors.entry(note.to.clone()).or_default();
        cursor.delivered = cursor.delivered.max(note.seq);
        cursor.reminded_at = now;
    }
    let last_reminded = who
        .iter()
        .filter_map(|name| cursors.get(name))
        .map(|cursor| cursor.reminded_at)
        .max()
        .unwrap_or(0);
    if delivery.notes.is_empty() && already > 0 && now - last_reminded >= REMIND_EVERY_MS {
        delivery.remind = true;
        for name in who {
            if cursors.contains_key(name) {
                cursors.entry(name.clone()).or_default().reminded_at = now;
            }
        }
    }
    delivery
}

/// Mark `who` as having been handed `notes`.
pub(super) fn mark_delivered(cursors: &mut Cursors, notes: &[InboxNote], now: i64) {
    for note in notes {
        let cursor = cursors.entry(note.to.clone()).or_default();
        cursor.delivered = cursor.delivered.max(note.seq);
        cursor.reminded_at = now;
    }
}

/// Mark everything through `through` read for `who`. What it had not been handed yet counts as
/// handed: it asked to be done with it.
pub(super) fn acknowledge(cursors: &mut Cursors, who: &[String], through: i64) {
    for name in who {
        let cursor = cursors.entry(name.clone()).or_default();
        cursor.acked = cursor.acked.max(through);
        cursor.delivered = cursor.delivered.max(through);
    }
}

/// Everything addressed to `who` that it has not marked read, whether handed yet or not.
pub(super) fn unread(
    notes: &[Note],
    who: &[String],
    me: &str,
    cursors: &Cursors,
) -> Vec<InboxNote> {
    wanted(notes, who, me)
        .filter(|note| note.seq > cursor_of(cursors, note.to_principal.as_ref()).acked)
        .map(InboxNote::from)
        .collect()
}

fn check_who(who: &[String]) -> BridgeResult<()> {
    if who.is_empty() {
        return Err(invalid("an inbox needs at least one name"));
    }
    Ok(())
}

impl Board {
    fn local_cursors(connection: &rusqlite::Connection) -> BridgeResult<Cursors> {
        let raw: Option<String> = connection
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![KEY],
                |row| row.get(0),
            )
            .optional()
            .map_err(store_error)?;
        Ok(raw
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default())
    }

    fn local_addressed(connection: &rusqlite::Connection, after: i64) -> BridgeResult<Vec<Note>> {
        let mut statement = connection
            .prepare(
                "SELECT seq, ticket, author, kind, body, to_principal, fence, created_at
                 FROM notes WHERE seq > ?1 AND to_principal IS NOT NULL ORDER BY seq LIMIT ?2",
            )
            .map_err(store_error)?;
        let rows = statement
            .query_map(params![after, SCAN], note_from_row)
            .map_err(store_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(store_error)
    }

    fn local_save(tx: &rusqlite::Transaction<'_>, cursors: &Cursors) -> BridgeResult<()> {
        tx.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            params![KEY, serde_json::to_string(cursors).expect("inbox json")],
        )
        .map_err(store_error)?;
        Ok(())
    }

    /// What `who` should hear now. With `commit`, they are counted as told; without, only
    /// `inbox_mark_delivered` moves them (a webhook marks a delivery once it was taken).
    pub fn inbox_deliver(
        &self,
        actor: &Actor<'_>,
        who: &[String],
        me: &str,
        commit: bool,
    ) -> BridgeResult<Delivery> {
        check_who(who)?;
        if let Some(hosted) = &self.hosted {
            return self.hosted_call(hosted.inbox_deliver(who, me, commit), false);
        }
        let now = self.now();
        {
            // Nothing addressed to them since they last marked read: no write at all.
            let connection = self.local()?;
            let cursors = Self::local_cursors(&connection)?;
            let notes = Self::local_addressed(&connection, floor(&cursors, who))?;
            if wanted(&notes, who, me).next().is_none() {
                return Ok(Delivery::default());
            }
            if !commit {
                let mut trial = cursors;
                return Ok(plan(&notes, who, me, &mut trial, now));
            }
        }
        self.write(actor, |w| {
            let mut cursors = Self::local_cursors(w.tx)?;
            let notes = Self::local_addressed(w.tx, floor(&cursors, who))?;
            let before = cursors.clone();
            let delivery = plan(&notes, who, me, &mut cursors, now);
            if cursors != before {
                Self::local_save(w.tx, &cursors)?;
            }
            Ok(delivery)
        })
    }

    /// `notes` were taken: they are not sent again.
    pub fn inbox_mark_delivered(&self, actor: &Actor<'_>, notes: &[InboxNote]) -> BridgeResult<()> {
        if let Some(hosted) = &self.hosted {
            return self.hosted_call(hosted.inbox_mark_delivered(notes), false);
        }
        let now = self.now();
        self.write(actor, |w| {
            let mut cursors = Self::local_cursors(w.tx)?;
            mark_delivered(&mut cursors, notes, now);
            Self::local_save(w.tx, &cursors)
        })
    }

    /// Everything addressed to `who` not yet marked read, after marking through `ack_through`.
    pub fn inbox(
        &self,
        actor: &Actor<'_>,
        who: &[String],
        ack_through: Option<i64>,
    ) -> BridgeResult<Vec<InboxNote>> {
        check_who(who)?;
        if let Some(hosted) = &self.hosted {
            return self.hosted_call(hosted.inbox(who, actor.id, ack_through), false);
        }
        self.write(actor, |w| {
            let mut cursors = Self::local_cursors(w.tx)?;
            if let Some(through) = ack_through {
                acknowledge(&mut cursors, who, through);
                Self::local_save(w.tx, &cursors)?;
            }
            let notes = Self::local_addressed(w.tx, floor(&cursors, who))?;
            Ok(unread(&notes, who, actor.id, &cursors))
        })
    }
}
