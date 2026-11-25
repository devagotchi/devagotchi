//! A change checked and applied to the entry it reads: pure, so every host and the Clyffy
//! handoff move entries by the same rules.

use std::collections::HashSet;

use super::model::*;
use crate::dictionary::Dictionary;

/// The entry `change` makes of `old` (none for a new one), at `now`, with `new_id` for a new
/// entry. Concepts must be the dictionary's.
pub(crate) fn next(
    old: Option<&Entry>,
    change: &Change,
    dictionary: &Dictionary,
    now: f64,
    new_id: impl FnOnce() -> String,
) -> Result<Entry> {
    check_change(change)?;
    let conflict = |m: &str| Err(LexiconError::Conflict(m.to_string()));
    if change.operation.creates() {
        if change.record_id.is_some() || change.expected_revision.is_some() {
            return invalid("a new entry names no record_id or expected_revision");
        }
        let (wording, state) = match &change.operation {
            Operation::Propose { wording } => (wording, State::Proposed),
            Operation::Define { wording } => (wording, State::Confirmed),
            Operation::Park { wording } => (wording, State::Inconclusive),
            _ => unreachable!("creates() covers these three"),
        };
        return Ok(Entry {
            id: new_id(),
            owner_id: change.owner_id.clone(),
            scope: change.scope.clone(),
            wording: checked(wording, dictionary, state)?,
            state,
            revision: 1,
            created_at: now,
            updated_at: now,
            provenance: change.provenance.clone(),
        });
    }
    let (Some(_), Some(expected)) = (&change.record_id, change.expected_revision) else {
        return invalid("a change to an entry names its record_id and expected_revision");
    };
    let Some(old) = old else {
        return Err(LexiconError::NotFound);
    };
    if old.owner_id != change.owner_id {
        return Err(LexiconError::NotFound);
    }
    if old.scope != change.scope {
        return conflict("the entry is in another scope");
    }
    if old.revision != expected {
        return conflict(&format!(
            "the entry is at revision {}, not {expected}",
            old.revision
        ));
    }
    let (wording, state) = match (&change.operation, old.state) {
        (_, State::Deleted) => return conflict("the entry is deleted"),
        (Operation::Revise { wording }, s) if s.live() => (checked(wording, dictionary, s)?, s),
        (Operation::Revise { .. }, _) => return conflict("only a live entry is revised"),
        (Operation::Confirm, State::Proposed | State::Inconclusive) => {
            let wording = old.wording.clone();
            if wording.meaning.trim().is_empty() {
                return conflict("an entry is confirmed once it says what it means");
            }
            (wording, State::Confirmed)
        }
        (Operation::Dismiss, s) if s.live() => (old.wording.clone(), State::Dismissed),
        (Operation::Retire, State::Confirmed) => (old.wording.clone(), State::Retired),
        (Operation::Delete, _) => (Wording::default(), State::Deleted),
        (operation, s) => {
            return conflict(&format!(
                "{} is not allowed on a {} entry",
                operation.name(),
                s.as_str()
            ));
        }
    };
    Ok(Entry {
        wording,
        state,
        revision: expected
            .checked_add(1)
            .ok_or_else(|| LexiconError::Invalid("revision overflow".into()))?,
        updated_at: now,
        provenance: change.provenance.clone(),
        ..old.clone()
    })
}

fn invalid<T>(m: &str) -> Result<T> {
    Err(LexiconError::Invalid(m.to_string()))
}

/// `wording` as stored: trimmed, bounded, its concepts the dictionary's and named once. An
/// inconclusive entry may leave its meaning empty; any other must say it.
fn checked(wording: &Wording, dictionary: &Dictionary, state: State) -> Result<Wording> {
    let phrase = wording
        .phrase
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if phrase.is_empty() || phrase.chars().count() > MAX_PHRASE_CHARS {
        return invalid(&format!("phrase: 1 to {MAX_PHRASE_CHARS} characters"));
    }
    let meaning = wording.meaning.trim().to_string();
    if (meaning.is_empty() && state != State::Inconclusive)
        || meaning.chars().count() > MAX_MEANING_CHARS
    {
        return invalid(&format!("meaning: 1 to {MAX_MEANING_CHARS} characters"));
    }
    for (items, label) in [
        (&wording.examples, "examples"),
        (&wording.counterexamples, "counterexamples"),
    ] {
        if items.len() > MAX_ITEMS
            || items
                .iter()
                .any(|s| s.trim().is_empty() || s.chars().count() > MAX_ITEM_CHARS)
        {
            return invalid(&format!(
                "{label}: at most {MAX_ITEMS}, each 1 to {MAX_ITEM_CHARS} characters"
            ));
        }
    }
    if wording.concepts.len() > MAX_CONCEPTS {
        return invalid(&format!("concepts: at most {MAX_CONCEPTS}"));
    }
    let mut seen = HashSet::new();
    for concept in &wording.concepts {
        if dictionary.concept(concept).is_none() {
            return invalid(&format!("concepts: {concept} is not in the dictionary"));
        }
        if !seen.insert(concept) {
            return invalid(&format!("concepts: {concept} is named twice"));
        }
    }
    Ok(Wording {
        phrase,
        meaning,
        concepts: wording.concepts.clone(),
        examples: wording
            .examples
            .iter()
            .map(|s| s.trim().to_string())
            .collect(),
        counterexamples: wording
            .counterexamples
            .iter()
            .map(|s| s.trim().to_string())
            .collect(),
    })
}

pub(crate) fn check_change(change: &Change) -> Result<()> {
    check_id(&change.owner_id, "owner_id")?;
    check_id(&change.request_id, "request_id")?;
    check_scope(&change.scope)?;
    if let Some(id) = &change.record_id {
        check_id(id, "record_id")?;
    }
    let p = &change.provenance;
    if !matches!(
        p.source_kind.as_str(),
        "user" | "conversation" | "seed" | "clyffy"
    ) {
        return invalid("provenance.source_kind: user, conversation, seed or clyffy");
    }
    for (value, label) in [
        (&p.project_id, "provenance.project_id"),
        (&p.workflow_id, "provenance.workflow_id"),
        (&p.turn_id, "provenance.turn_id"),
    ] {
        if let Some(v) = value {
            check_id(v, label)?;
        }
    }
    if let (Some(from), Scope::Project { project_id }) = (&p.project_id, &change.scope)
        && from != project_id
    {
        return invalid("provenance.project_id differs from the scope's project");
    }
    Ok(())
}

pub(crate) fn check_scope(scope: &Scope) -> Result<()> {
    match scope {
        Scope::Personal => Ok(()),
        Scope::Project { project_id } => check_id(project_id, "scope.project_id"),
    }
}

/// An id: 1 to 128 of letters, digits and `_ - . : @ /`. Wide enough for a hub subject
/// (`github:123`) and a Clyffy owner (a UUID); no spaces and nothing that needs escaping.
pub(crate) fn check_id(id: &str, label: &str) -> Result<()> {
    let ok = !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-.:@/".contains(&b));
    if ok {
        Ok(())
    } else {
        invalid(&format!("{label}: 1 to 128 letters, digits or _ - . : @ /"))
    }
}
