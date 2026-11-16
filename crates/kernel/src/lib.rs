//! The Devagotchi kernel: the one home for Devagotchi's own AI-specific code.
//!
//! Each domain is a folder with its front door in `mod.rs` (see
//! `.ai/patterns/layout.md`). The kernel opens no network connection; the hub and the
//! machine engine give it storage and transport.
//!
//! - `capture`: the `devagotchi ai` command: operator policy, harness hooks, the
//!   machine's private capture journal and delivery status (DG-CAPTURE-1).
//! - `dictionary`: the concepts Devagotchi routes to (domain boundaries and stacks), their
//!   names and disambiguation rules, and the shipped seed.
//! - `lexicon`: the person's and their projects' own phrases, their states and revisions,
//!   the store a host implements, and the resolver.
//! - `words`: how both compare text.

pub mod capture;
pub mod dictionary;
pub mod lexicon;
pub mod words;
