//! Checks: every enforced rule against the tree and its baseline, each rule's examples as its
//! own tests, and the upkeep of baselines.
//!
//! # Owns
//!
//! How a finding becomes a violation: the rule's guidance filled with the finding's values,
//! and a baseline applied (`baselines` decides what breaks it). A stale baseline entry is a
//! violation of its own, reported on the baseline file's line, that says to remove the line.
//! `--changed` keeps the violations about the changed paths (a changed folder, a file inside a
//! flagged folder, or the rule's own folder, which shows all of that rule's).
//!
//! # Serves
//!
//! [`Workspace::load`], [`check`], [`examples::test_examples`], [`upkeep::tighten`] and
//! [`upkeep::seed`] to the command line and the crate's tests.
//!
//! # Settings
//!
//! None.
//!
//! # Depends on
//!
//! `crate::tree`, `crate::rules`, `crate::engines`, `crate::baselines` and `crate::reports`.

pub mod examples;
#[cfg(test)]
pub(crate) mod tests;
pub mod upkeep;

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use crate::{
    baselines::{self, Baseline, Breach, Entries, Kind},
    engines::{self, Finding, Outcome},
    reports::{Report, Violation},
    rules::{self, Check, Rule},
    tree::{Scope, Tree},
};

/// What stops bebop before it can give an answer: a malformed rule or baseline, an unreadable
/// tree, a usage error. The command exits 2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Broken(pub Vec<String>);

impl Broken {
    pub fn one(message: impl Into<String>) -> Broken {
        Broken(vec![message.into()])
    }
}

/// Which violations a check reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    All,
    /// Paths from the repository root.
    Changed(Vec<String>),
}

/// The repository: its tree and its rules.
pub struct Workspace {
    pub root: PathBuf,
    pub tree: Tree,
    pub rules: Vec<Rule>,
}

impl Workspace {
    pub fn load(root: &Path) -> Result<Workspace, Broken> {
        let tree = Tree::walk(root, true).map_err(Broken::one)?;
        let rules = rules::load(&tree)
            .map_err(|errors| Broken(errors.iter().map(ToString::to_string).collect()))?;
        Ok(Workspace {
            root: root.to_path_buf(),
            tree,
            rules,
        })
    }

    pub fn rule(&self, id: &str) -> Result<&Rule, Broken> {
        self.rules.iter().find(|rule| rule.id == id).ok_or_else(|| {
            Broken::one(format!(
                "no rule has the id {id:?}; the rules are {}",
                self.rules
                    .iter()
                    .map(|rule| rule.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })
    }
}

/// Every enforced rule against the tree.
pub fn check(workspace: &Workspace, selection: &Selection) -> Result<Report, Broken> {
    let mut report = Report::default();
    let mut errors = Vec::new();
    for rule in workspace.rules.iter().filter(|rule| rule.is_enforced()) {
        report.rules += 1;
        let mut found = match evaluate(rule, &workspace.tree) {
            Ok(found) => found,
            Err(e) => {
                errors.push(e);
                continue;
            }
        };
        if let Selection::Changed(paths) = selection
            && !paths.iter().any(|path| rule.in_folder(path))
        {
            found.retain(|v| paths.iter().any(|path| touches(&v.subject, path)));
        }
        report.violations.extend(found);
    }
    if errors.is_empty() {
        Ok(report)
    } else {
        Err(Broken(errors))
    }
}

/// `subject` and `changed` are the same path, or one holds the other.
fn touches(subject: &str, changed: &str) -> bool {
    let holds = |outer: &str, inner: &str| {
        inner
            .strip_prefix(outer)
            .is_some_and(|rest| rest.starts_with('/'))
    };
    subject == changed || holds(changed, subject) || holds(subject, changed)
}

/// The baseline kind a check keeps.
pub fn kind(check: &Check) -> Kind {
    match check {
        Check::MaxLines { max } => Kind::Counted { cap: *max },
        _ => Kind::Listed,
    }
}

/// The rule's baseline, if it has one.
pub fn read_baseline(rule: &Rule) -> Result<Option<Baseline>, String> {
    let path = rule.baseline_path();
    if !path.is_file() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", rule.baseline_file()))?;
    baselines::parse(&text, kind(&rule.check))
        .map(Some)
        .map_err(|e| format!("{}: {e}", rule.baseline_file()))
}

/// The rule's engine over `tree`, with no baseline.
pub fn outcome(rule: &Rule, tree: &Tree) -> Result<Outcome, String> {
    let scope = Scope::new(&rule.scope.include, &rule.scope.exclude)
        .map_err(|e| format!("{}: scope: {e}", rule.file()))?;
    engines::run(&rule.check, tree, &scope).map_err(|e| format!("{}: {e}", rule.id))
}

/// One rule against the tree and its baseline: its violations.
pub fn evaluate(rule: &Rule, tree: &Tree) -> Result<Vec<Violation>, String> {
    let outcome = outcome(rule, tree)?;
    let baseline = read_baseline(rule)?;
    let line_of = |path: &str| baseline.as_ref().and_then(|b| b.lines.get(path).copied());
    let mut violations = Vec::new();
    match (&outcome, &rule.check) {
        (Outcome::Measured(actual), Check::MaxLines { max }) => {
            let listed = match &baseline {
                Some(Baseline {
                    entries: Entries::Counted(listed),
                    ..
                }) => listed.clone(),
                _ => BTreeMap::new(),
            };
            for breach in baselines::counted_breaches(*max, &listed, actual) {
                let (lines, allowed) = match &breach {
                    Breach::Grew { lines, allowed, .. } => (*lines, *allowed),
                    Breach::OverCap { lines, .. } => (*lines, *max),
                    Breach::Missing { path } => {
                        violations.push(stale(rule, path, line_of(path), true));
                        continue;
                    }
                };
                let mut finding = Finding::new(breach.path())
                    .with("lines", lines.to_string())
                    .with("max", max.to_string())
                    .with("allowed", allowed.to_string());
                finding.line = Some(allowed + 1);
                violations.push(violation(rule, &finding));
            }
        }
        (Outcome::Measured(_), _) => {
            return Err(format!(
                "{}: the engine measured instead of finding",
                rule.id
            ));
        }
        (Outcome::Found(found), _) => {
            let listed = match &baseline {
                Some(Baseline {
                    entries: Entries::Listed(listed),
                    ..
                }) => listed.clone(),
                _ => BTreeSet::new(),
            };
            let paths: BTreeSet<&str> = found.iter().map(|f| f.file.as_str()).collect();
            for finding in found.iter().filter(|f| !listed.contains(&f.file)) {
                violations.push(violation(rule, finding));
            }
            for path in listed.iter().filter(|path| !paths.contains(path.as_str())) {
                violations.push(stale(rule, path, line_of(path), false));
            }
        }
    }
    violations.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
    Ok(violations)
}

fn violation(rule: &Rule, finding: &Finding) -> Violation {
    Violation {
        rule: rule.id.clone(),
        category: rule.category.clone(),
        severity: rule.severity.as_str().to_owned(),
        file: finding.file.clone(),
        line: finding.line,
        statement: rule.statement.clone(),
        why: rule.why.clone(),
        expected: fill(&rule.guidance.expected, finding),
        fix: fill(&rule.guidance.fix, finding),
        example: rule.guidance.example.clone(),
        subject: finding.file.clone(),
    }
}

/// A baseline entry that no longer matches the tree.
fn stale(rule: &Rule, path: &str, line: Option<usize>, missing: bool) -> Violation {
    let baseline = rule.baseline_file();
    let at = line.map_or_else(String::new, |line| format!(" on line {line}"));
    let fix = if missing {
        format!(
            "{path} is listed{at} of {baseline} but is not in the rule's scope. If it moved, \
             change its path on its line and keep its number, so the review shows the same count \
             at the new path; if it is gone, remove its line (`bebop baseline tighten` removes it)."
        )
    } else {
        format!(
            "{path} no longer breaks {}: remove its line{at} of {baseline} \
             (`bebop baseline tighten` removes it).",
            rule.id
        )
    };
    Violation {
        rule: rule.id.clone(),
        category: rule.category.clone(),
        severity: rule.severity.as_str().to_owned(),
        file: baseline.clone(),
        line,
        statement: rule.statement.clone(),
        why: rule.why.clone(),
        expected: format!(
            "{baseline} lists only today's violations of {}: it only shrinks",
            rule.id
        ),
        fix,
        example: rule.guidance.example.clone(),
        subject: path.to_owned(),
    }
}

/// `text` with each `{name}` the finding gives replaced by its value.
pub fn fill(text: &str, finding: &Finding) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let value = after
            .find('}')
            .and_then(|close| finding.var(&after[..close]).map(|v| (close, v)));
        match value {
            Some((close, value)) => {
                out.push_str(value);
                rest = &after[close + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}
