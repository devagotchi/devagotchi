//! Rules as data: the format of `rule.toml`, read strictly, and where rules live.
//!
//! # Owns
//!
//! The rule format (`model`) and its reading (`load`). A rule is
//! `<boundary>/.ai/rules/<category>/<name>/rule.toml`, its id is `<category>.<name>`, and its
//! folder holds its `baseline.txt` and its `examples/good/` and `examples/bad/` trees. Every
//! field is required unless marked, unknown fields are refused, and a malformed rule names
//! its file and field.
//!
//! # Serves
//!
//! [`load`] and the [`Rule`] type to the checks and the command line.
//!
//! # Settings
//!
//! None. The rules themselves are the settings: `.ai/rules/` at the repository root for the
//! rules that govern the whole workspace.
//!
//! # Depends on
//!
//! `crate::tree` (where rules are found, and that a guidance example exists) and
//! `crate::engines` (the values each engine gives a guidance text).

mod load;
mod model;
#[cfg(test)]
mod tests;

pub use load::{RuleError, load, parse, placeholders};
pub use model::{Approval, Case, Check, Guidance, Rule, ScopeSpec, Severity, Status};
