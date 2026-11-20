//! The kernel's lexicon and dictionary, hosted by the hub (kernel K3 and K5,
//! `.ai/dev/plans/kernel.md`): the words a person and their projects use, kept in the hub's
//! Postgres, and the dictionary Devagotchi ships, served to their AI as MCP tools. The rules
//! (states, what a change may do, how a phrase resolves) are `devagotchi-kernel`'s; this
//! domain keeps what it decides and says who is asking.
//!
//! - **Owns:** the tables `kernel_lexicon_entries`, `kernel_lexicon_events` and
//!   `kernel_lexicon_receipts` (migration 0032) and `kernel_lexicon_authority` (0033: who
//!   holds each lexicon, this hub or Clyffy), one owner's rows at a time.
//! - **Serves:** the MCP tools `dictionary_get`, `lexicon_resolve`, `lexicon_propose` and
//!   `lexicon_review` (`mcp.rs`), and `lexicon_handoff` (`handoff.rs`). The owner is always
//!   the signed-in subject, never an argument. Writes are refused while Clyffy holds it.
//! - **Settings:** none. Every hub serves them.
//! - **Depends on:** `devagotchi-kernel` (the lexicon, the dictionary), the hub's store
//!   (`crate::store`), its identity (`Subject`) and its audit (writes are recorded).

pub mod authority;
pub(crate) mod handoff;
pub(crate) mod mcp;
pub(crate) mod schema;
pub mod stored;
#[cfg(test)]
mod tests;

use devagotchi_kernel::{dictionary::Dictionary, lexicon::Lexicon};
use sha2::{Digest, Sha256};

use crate::{identity::Subject, mcp::Hub};
pub use stored::Stored;

/// The tools this domain serves (the tests check every tool list against it).
#[cfg(test)]
pub const TOOLS: [&str; 5] = [
    "dictionary_get",
    "lexicon_resolve",
    "lexicon_propose",
    "lexicon_review",
    "lexicon_handoff",
];

/// The lexicon on a hub's store, with the shipped dictionary.
pub type HubLexicon = Lexicon<'static, Stored>;

impl Hub {
    /// This domain's tools.
    pub(crate) fn words_router() -> rmcp::handler::server::router::tool::ToolRouter<Self> {
        Self::lexicon_router() + Self::handoff_router()
    }

    /// The lexicon in this hub's Postgres (the store its audit keeps).
    pub(crate) fn lexicon(&self) -> HubLexicon {
        Lexicon::new(Stored::new(self.audit.store().clone()), Dictionary::seed())
    }
}

/// Whose words these are: the signed-in subject's id, as the lexicon keys an owner. An id the
/// lexicon cannot key as it is (a character outside `A-Z a-z 0-9 _ - . : @ /`, or longer
/// than 128) is keyed by its SHA-256 instead, the same every time.
pub fn owner_of(subject: &Subject) -> String {
    let id = subject.id.as_str();
    let plain = !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-.:@/".contains(&b));
    if plain {
        id.to_string()
    } else {
        format!("sha256:{}", hex::encode(Sha256::digest(id.as_bytes())))
    }
}
