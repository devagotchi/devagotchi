//! A rule's examples are its own tests: `examples/bad/` must break the rule and
//! `examples/good/` must keep it, each checked as a tree of its own with the rule's scope and
//! no baseline. A rule without both fails.

use super::{Broken, Workspace, outcome};
use crate::{
    engines::Outcome,
    rules::{Check, Rule, Status},
    tree::Tree,
};

/// How one rule's examples went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExampleOutcome {
    pub rule: String,
    /// How many findings the bad example has.
    pub bad: usize,
    /// What is wrong; empty when the examples pass.
    pub problems: Vec<String>,
}

impl ExampleOutcome {
    pub fn passed(&self) -> bool {
        self.problems.is_empty()
    }
}

/// The examples of every rule that is not retired, or of the rule `id`.
pub fn test_examples(
    workspace: &Workspace,
    id: Option<&str>,
) -> Result<Vec<ExampleOutcome>, Broken> {
    let rules: Vec<&Rule> = match id {
        Some(id) => vec![workspace.rule(id)?],
        None => workspace
            .rules
            .iter()
            .filter(|rule| rule.status != Status::Retired)
            .collect(),
    };
    Ok(rules.into_iter().map(test_rule).collect())
}

fn test_rule(rule: &Rule) -> ExampleOutcome {
    let mut problems = Vec::new();
    let mut bad = 0;
    for which in ["bad", "good"] {
        let dir = rule.examples(which);
        let shown = format!("{}/examples/{which}/", rule.folder_path);
        if !dir.is_dir() {
            problems.push(format!(
                "{shown} is missing: every rule has a {which} example, a small tree that {}",
                if which == "bad" {
                    "breaks the rule"
                } else {
                    "keeps it"
                }
            ));
            continue;
        }
        let flagged = match Tree::walk(&dir, false).and_then(|tree| outcome(rule, &tree)) {
            Ok(outcome) => flagged(&outcome, &rule.check),
            Err(e) => {
                problems.push(format!("{shown}: {e}"));
                continue;
            }
        };
        match which {
            "bad" if flagged.is_empty() => problems.push(format!(
                "{shown} breaks nothing: make it break the rule in scope ({})",
                rule.scope.include.join(", ")
            )),
            "bad" => bad = flagged.len(),
            _ if !flagged.is_empty() => problems.push(format!(
                "{shown} breaks the rule at {}: the good example keeps it",
                flagged.join(", ")
            )),
            _ => {}
        }
    }
    ExampleOutcome {
        rule: rule.id.clone(),
        bad,
        problems,
    }
}

/// The paths an outcome flags, with no baseline.
fn flagged(outcome: &Outcome, check: &Check) -> Vec<String> {
    match (outcome, check) {
        (Outcome::Measured(files), Check::MaxLines { max }) => files
            .iter()
            .filter(|(_, lines)| *lines > max)
            .map(|(path, _)| path.clone())
            .collect(),
        (Outcome::Measured(_), _) => Vec::new(),
        (Outcome::Found(found), _) => found.iter().map(|f| f.file.clone()).collect(),
    }
}
