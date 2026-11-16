//! The Clyffy handoff: when a person has Clyffy and Devagotchi, Clyffy takes over their
//! lexicon; when they leave Clyffy, Devagotchi takes it back. One side writes at a time.
//!
//! - [`bundle`]: `LexiconBundleV1`, the entries as they cross, in Clyffy's own entry shape.
//! - [`merge`]: two sides' entries made one, symmetric, with confirmed meanings that differ
//!   left to the person as conflicts.
//! - [`authority`]: who holds the lexicon (devagotchi, handing_over, clyffy, restoring) and
//!   the epoch that fences a write or a bundle from before the last move.
//!
//! A host keeps the authority beside the entries and calls these in order: `begin_handover`,
//! [`export`] at the new epoch, `complete_handover` once Clyffy has it; then, while Clyffy
//! holds it, [`take_in`] each bundle Clyffy sends into the mirror; to restore,
//! `begin_restore`, [`take_in`] Clyffy's last bundle, write the merged entries back, and
//! `complete_restore`.

pub mod authority;
pub mod bundle;
pub mod merge;
#[cfg(test)]
mod tests;

pub use authority::{Authority, Holder};
pub use bundle::{BundleEntry, FORMAT, LexiconBundleV1, Side};
pub use merge::{Conflict, Merged, merge};

use crate::lexicon::{Entry, Result};

/// Devagotchi's entries of `owner` as the bundle it hands Clyffy at `authority`'s epoch, once
/// it has stopped writing (`handing_over`).
pub fn export(
    authority: &Authority,
    owner: &str,
    now: f64,
    entries: &[Entry],
) -> Result<LexiconBundleV1> {
    if authority.holder != Holder::HandingOver {
        return Err(crate::lexicon::LexiconError::Conflict(format!(
            "export hands the lexicon over: begin the handover first (it is {})",
            authority.holder.as_str()
        )));
    }
    let mine: Vec<Entry> = entries
        .iter()
        .filter(|e| e.owner_id == owner)
        .cloned()
        .collect();
    Ok(LexiconBundleV1::new(
        owner,
        Side::Devagotchi,
        authority.epoch,
        now,
        &mine,
    ))
}

/// Clyffy's `bundle` taken in against what Devagotchi holds for the same owner (its mirror, or
/// its entries from before the handover): checked against the authority, then merged.
pub fn take_in(authority: &Authority, ours: &[Entry], bundle: &LexiconBundleV1) -> Result<Merged> {
    bundle.check()?;
    authority.accepts(bundle)?;
    let ours: Vec<Entry> = ours
        .iter()
        .filter(|e| e.owner_id == bundle.owner_id)
        .cloned()
        .collect();
    Ok(merge(&ours, &bundle.entries()))
}
