//! bebop, the rules devtool (`.ai/dev/plans/program.md`, §6), phase B0.
//!
//! A rule is data: `<boundary>/.ai/rules/<category>/<name>/rule.toml`, with its baseline and
//! its good and bad examples beside it. bebop walks the tree, runs each enforced rule's engine
//! over the files in its scope, holds what it finds against the rule's shrink-only baseline,
//! and reports every violation with what good looks like and how to get there, as XML for an
//! AI or as text for a person. It is a devtool: never in the official build.
//!
//! The domains, in the order a check flows through them:
//! - [`tree`]: the files and folders a rule is checked against, and a rule's scope;
//! - [`rules`]: the rule format, read strictly, and where rules live;
//! - [`engines`]: what each kind of check finds;
//! - [`baselines`]: the shrink-only lists of today's violations;
//! - [`checks`]: a rule against the tree and its baseline, its examples, and baseline upkeep;
//! - [`reports`]: one finding, two forms (XML and text);
//! - [`cli`]: the `bebop` command line.

pub mod baselines;
pub mod checks;
pub mod cli;
pub mod engines;
pub mod reports;
pub mod rules;
pub mod tree;
