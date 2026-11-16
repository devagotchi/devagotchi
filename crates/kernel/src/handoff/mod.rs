//! The Clyffy handoff: when a person has Clyffy and Devagotchi, Clyffy takes over their
//! lexicon; when they leave Clyffy, Devagotchi takes it back. One side writes at a time.
//!
//! - [`bundle`]: `LexiconBundleV1`, the entries as they cross, in Clyffy's own entry shape.
//! - [`merge`]: two sides' entries made one, symmetric, with confirmed meanings that differ
//!   left to the person as conflicts.
//! - [`authority`]: who holds the lexicon (devagotchi, handing_over, clyffy, restoring) and
//!   the epoch that fences a write or a bundle from before the last move.
//!
//! A host keeps the authority beside the entries. To hand over: [`preview`] Clyffy's bundle
//! (the conflicts the person settles first), `begin_handover`, [`export`] the merged entries
//! at the new epoch (the host keeps them too), `complete_handover` once Clyffy has them. While
//! Clyffy holds the lexicon, it is the one source of truth: [`take_in`] gives the entries of
//! each bundle it sends, and the host's mirror becomes exactly those. To restore,
//! `begin_restore`, [`take_in`] Clyffy's last bundle if it sends one (else the mirror stands),
//! `complete_restore`.

pub mod authority;
pub mod bundle;
pub mod merge;
#[cfg(test)]
mod tests;

pub use authority::{Authority, Holder};
pub use bundle::{BundleEntry, FORMAT, LexiconBundleV1, MAX_BUNDLE_ENTRIES, Side};
pub use merge::{Conflict, Merged, merge};

use crate::lexicon::{Entry, LexiconError, Result, State};

fn owned(entries: &[Entry], owner: &str) -> Vec<Entry> {
    entries
        .iter()
        .filter(|e| e.owner_id == owner)
        .cloned()
        .collect()
}

fn clyffys<'a>(bundle: &'a LexiconBundleV1, owner: &str) -> Result<&'a LexiconBundleV1> {
    bundle.check()?;
    if bundle.from != Side::Clyffy {
        return Err(LexiconError::Invalid("the bundle is not Clyffy's".into()));
    }
    if bundle.owner_id != owner {
        return Err(LexiconError::Invalid(
            "the bundle is another person's".into(),
        ));
    }
    Ok(bundle)
}

/// What handing `owner`'s entries over would come to against Clyffy's `bundle`, before
/// anything moves: the conflicts the person settles first, and what would be replaced.
pub fn preview(owner: &str, ours: &[Entry], bundle: &LexiconBundleV1) -> Result<Merged> {
    let bundle = clyffys(bundle, owner)?;
    Ok(merge(&owned(ours, owner), &bundle.entries()))
}

/// A handover's result: the bundle Clyffy takes, which is also what the host keeps.
#[derive(Debug, Clone, PartialEq)]
pub struct Handover {
    pub bundle: LexiconBundleV1,
    /// Live entries a confirmed meaning or a newer proposal replaced: dismissed in `bundle`.
    pub superseded: usize,
}

/// Devagotchi's entries of `owner`, merged with Clyffy's own `theirs` when Clyffy had a
/// lexicon too, as the bundle Clyffy takes at `authority`'s epoch. Only once Devagotchi has
/// stopped writing (`handing_over`), and only when no phrase has two confirmed meanings: a
/// handover never decides those for the person. An entry another replaced is carried as
/// dismissed (one revision on, at `now`), so both sides keep every id.
pub fn export(
    authority: &Authority,
    owner: &str,
    now: f64,
    ours: &[Entry],
    theirs: Option<&LexiconBundleV1>,
) -> Result<Handover> {
    if authority.holder != Holder::HandingOver {
        return Err(LexiconError::Conflict(format!(
            "export hands the lexicon over: begin the handover first (it is {})",
            authority.holder.as_str()
        )));
    }
    let ours = owned(ours, owner);
    let merged = match theirs {
        None => merge(&ours, &[]),
        Some(bundle) => {
            let bundle = clyffys(bundle, owner)?;
            if bundle.epoch != authority.epoch {
                return Err(LexiconError::Conflict(format!(
                    "the bundle is from epoch {}; the handover is at epoch {}",
                    bundle.epoch, authority.epoch
                )));
            }
            merge(&ours, &bundle.entries())
        }
    };
    if !merged.conflicts.is_empty() {
        return Err(LexiconError::Conflict(format!(
            "{} phrase(s) have two meanings the person confirmed; settle them first",
            merged.conflicts.len()
        )));
    }
    let superseded = merged.superseded.len();
    let mut entries = merged.entries;
    for mut gone in merged.superseded {
        gone.state = State::Dismissed;
        gone.revision += 1;
        gone.updated_at = gone.updated_at.max(now);
        entries.push(gone);
    }
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    let bundle = LexiconBundleV1::new(owner, Side::Devagotchi, authority.epoch, now, &entries);
    bundle.check()?;
    Ok(Handover { bundle, superseded })
}

/// The entries of Clyffy's `bundle` of `owner`'s lexicon, once the authority takes it: while
/// Clyffy holds the lexicon they are the whole truth, so a host's mirror becomes exactly these.
pub fn take_in(authority: &Authority, owner: &str, bundle: &LexiconBundleV1) -> Result<Vec<Entry>> {
    let bundle = clyffys(bundle, owner)?;
    authority.accepts(bundle)?;
    Ok(bundle.entries())
}
