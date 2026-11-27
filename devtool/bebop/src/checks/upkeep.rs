//! Baseline upkeep: `tighten` lowers numbers and drops resolved entries and never adds one or
//! raises a number; `seed` writes a rule's first baseline from today's tree, once, and refuses
//! when the rule already has one.

use std::{collections::BTreeSet, fs};

use super::{Broken, Workspace, kind, outcome, read_baseline};
use crate::{
    baselines::{self, Entries},
    engines::Outcome,
    rules::{Check, Rule},
};

/// Tightens the baseline of every rule that has one, or of the rule `id`. One line per rule.
pub fn tighten(workspace: &Workspace, id: Option<&str>) -> Result<Vec<String>, Broken> {
    let rules: Vec<&Rule> = match id {
        Some(id) => vec![workspace.rule(id)?],
        None => workspace.rules.iter().collect(),
    };
    let mut said = Vec::new();
    let mut errors = Vec::new();
    for rule in rules {
        match tighten_rule(workspace, rule) {
            Ok(Some(line)) => said.push(line),
            Ok(None) if id.is_some() => said.push(format!("{}: no baseline", rule.id)),
            Ok(None) => {}
            Err(e) => errors.push(e),
        }
    }
    if errors.is_empty() {
        Ok(said)
    } else {
        Err(Broken(errors))
    }
}

fn tighten_rule(workspace: &Workspace, rule: &Rule) -> Result<Option<String>, String> {
    let Some(baseline) = read_baseline(rule)? else {
        return Ok(None);
    };
    let now = outcome(rule, &workspace.tree)?;
    let tightened = match (&baseline.entries, &now, &rule.check) {
        (Entries::Counted(listed), Outcome::Measured(actual), Check::MaxLines { max }) => {
            Entries::Counted(baselines::tighten_counted(*max, listed, actual))
        }
        (Entries::Listed(listed), Outcome::Found(found), _) => {
            let found: BTreeSet<String> = found.iter().map(|f| f.file.clone()).collect();
            Entries::Listed(baselines::tighten_listed(listed, &found))
        }
        _ => return Err(format!("{}: the baseline does not fit the engine", rule.id)),
    };
    if tightened == baseline.entries {
        return Ok(Some(format!(
            "{}: unchanged, {} {}",
            rule.id,
            baseline.entries.len(),
            entries(baseline.entries.len())
        )));
    }
    let lowered = match (&baseline.entries, &tightened) {
        (Entries::Counted(before), Entries::Counted(after)) => after
            .iter()
            .filter(|(path, lines)| before.get(*path).is_some_and(|was| was > lines))
            .count(),
        _ => 0,
    };
    fs::write(
        rule.baseline_path(),
        baselines::render(&baseline.header, &tightened),
    )
    .map_err(|e| format!("{}: {e}", rule.baseline_file()))?;
    let removed = baseline.entries.len() - tightened.len();
    Ok(Some(format!(
        "{}: {removed} {} removed, {lowered} lowered, {} left",
        rule.id,
        entries(removed),
        tightened.len()
    )))
}

fn entries(n: usize) -> &'static str {
    if n == 1 { "entry" } else { "entries" }
}

/// Writes the first baseline of the rule `id` from today's tree.
pub fn seed(workspace: &Workspace, id: &str) -> Result<String, Broken> {
    let rule = workspace.rule(id)?;
    let file = rule.baseline_file();
    if rule.baseline_path().exists() {
        return Err(Broken::one(format!(
            "{file} already exists. A baseline is seeded once and only shrinks after: fix the \
             code, then run `bebop baseline tighten {id}`"
        )));
    }
    let now = outcome(rule, &workspace.tree).map_err(Broken::one)?;
    let seeded = match (&now, &rule.check) {
        (Outcome::Measured(actual), Check::MaxLines { max }) => Entries::Counted(
            actual
                .iter()
                .filter(|(_, lines)| *lines > max)
                .map(|(path, lines)| (path.clone(), *lines))
                .collect(),
        ),
        (Outcome::Found(found), _) => {
            Entries::Listed(found.iter().map(|f| f.file.clone()).collect())
        }
        _ => {
            return Err(Broken::one(format!(
                "{id}: the engine does not fit a baseline"
            )));
        }
    };
    if seeded.is_empty() {
        return Ok(format!(
            "{id}: no violations on today's tree, so no baseline is needed"
        ));
    }
    let header = baselines::seed_header(id, kind(&rule.check));
    fs::write(rule.baseline_path(), baselines::render(&header, &seeded))
        .map_err(|e| Broken::one(format!("{file}: {e}")))?;
    Ok(format!(
        "{id}: seeded {file} with {} {}",
        seeded.len(),
        entries(seeded.len())
    ))
}
