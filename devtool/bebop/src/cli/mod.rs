//! The `bebop` command line: `check`, `rule test`, `baseline tighten` and `baseline seed`.
//!
//! # Owns
//!
//! The verbs and their flags, finding the repository (the nearest folder above the working
//! directory with `.ai/rules/`, or `--root`), and the exit codes: 0 clean, 1 a violation or a
//! failing example, 2 a malformed rule or baseline, or a usage error. Every message says what
//! to do.
//!
//! # Serves
//!
//! [`run`] to `main.rs`, and to the crate's tests.
//!
//! # Settings
//!
//! `--root <dir>` on any verb; `check` takes `--all` (the default) or `--changed <paths…>`, and
//! `--format text` (the default) or `--format xml`.
//!
//! # Depends on
//!
//! `crate::checks` (the work) and `crate::tree` (path normalization).

#[cfg(test)]
mod tests;

use std::{
    io::Write,
    path::{Path, PathBuf},
};

use crate::{
    checks::{self, Broken, Selection, Workspace, examples, upkeep},
    tree,
};

pub const USAGE: &str = "\
bebop, the rules devtool: rules as data under .ai/rules, checked against the tree.

Usage:
  bebop check [--all | --changed <path>...] [--format text|xml] [--root <dir>]
  bebop rule test [<rule-id>] [--root <dir>]
  bebop baseline tighten [<rule-id>] [--root <dir>]
  bebop baseline seed <rule-id> [--root <dir>]

check runs every enforced rule over the tree (--all, the default) and reports each violation
with the rule, why, what good looks like and the fix; --changed keeps the violations about the
given paths. rule test runs each rule's examples/bad and examples/good trees. baseline tighten
lowers numbers and drops resolved entries, and never adds one. baseline seed writes a rule's
first baseline from today's tree and refuses when it already has one.

Exit codes: 0 clean, 1 a violation or a failing example, 2 a malformed rule or a usage error.
";

pub const CLEAN: i32 = 0;
pub const FAILED: i32 = 1;
pub const BROKEN: i32 = 2;

enum Format {
    Text,
    Xml,
}

/// Runs one command line. Reports go to `out`, problems to `err`; the result is the exit code.
pub fn run(args: &[String], cwd: &Path, out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    match command(args, cwd, out) {
        Ok(code) => code,
        Err(Broken(problems)) => {
            let _ = writeln!(err, "bebop: stopped before an answer:");
            for problem in problems {
                let _ = writeln!(err, "  - {problem}");
            }
            BROKEN
        }
    }
}

fn usage(message: &str) -> Broken {
    Broken::one(format!("{message}. Run `bebop help` for the commands"))
}

fn command(args: &[String], cwd: &Path, out: &mut dyn Write) -> Result<i32, Broken> {
    let mut rest = Vec::new();
    let mut root = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--root" {
            let dir = iter.next().ok_or_else(|| usage("--root needs a folder"))?;
            root = Some(cwd.join(dir));
        } else {
            rest.push(arg.as_str());
        }
    }
    let write = |out: &mut dyn Write, text: &str| {
        out.write_all(text.as_bytes())
            .map_err(|e| Broken::one(format!("writing the output: {e}")))
    };
    match rest.as_slice() {
        [] | ["help" | "--help" | "-h"] => {
            write(out, USAGE)?;
            Ok(CLEAN)
        }
        ["check", flags @ ..] => {
            let (selection, format) = check_flags(flags)?;
            let root = find_root(root, cwd)?;
            let selection = match selection {
                Selection::Changed(paths) => Selection::Changed(
                    paths
                        .iter()
                        .map(|path| from_root(&root, cwd, path))
                        .collect::<Result<_, _>>()?,
                ),
                all => all,
            };
            let workspace = Workspace::load(&root)?;
            let report = checks::check(&workspace, &selection)?;
            let text = match format {
                Format::Text => report.text(),
                Format::Xml => report.xml(),
            };
            write(out, &text)?;
            Ok(if report.is_clean() { CLEAN } else { FAILED })
        }
        ["rule", "test", id @ ..] if id.len() <= 1 && !id.iter().any(|a| a.starts_with('-')) => {
            let workspace = Workspace::load(&find_root(root, cwd)?)?;
            let outcomes = examples::test_examples(&workspace, id.first().copied())?;
            let mut text = String::new();
            for outcome in &outcomes {
                if outcome.passed() {
                    text.push_str(&format!(
                        "ok    {}: bad flagged {}, good clean\n",
                        outcome.rule, outcome.bad
                    ));
                } else {
                    text.push_str(&format!("FAIL  {}\n", outcome.rule));
                    for problem in &outcome.problems {
                        text.push_str(&format!("        {problem}\n"));
                    }
                }
            }
            let failed = outcomes.iter().filter(|o| !o.passed()).count();
            text.push_str(&format!(
                "bebop rule test: {} rules, {failed} failed\n",
                outcomes.len()
            ));
            write(out, &text)?;
            Ok(if failed == 0 { CLEAN } else { FAILED })
        }
        ["baseline", "tighten", id @ ..]
            if id.len() <= 1 && !id.iter().any(|a| a.starts_with('-')) =>
        {
            let workspace = Workspace::load(&find_root(root, cwd)?)?;
            let said = upkeep::tighten(&workspace, id.first().copied())?;
            write(out, &(said.join("\n") + "\n"))?;
            Ok(CLEAN)
        }
        ["baseline", "seed", id] if !id.starts_with('-') => {
            let workspace = Workspace::load(&find_root(root, cwd)?)?;
            let said = upkeep::seed(&workspace, id)?;
            write(out, &(said + "\n"))?;
            Ok(CLEAN)
        }
        ["baseline", "seed"] => Err(usage("baseline seed needs the rule's id")),
        other => Err(usage(&format!(
            "{:?} is not a bebop command",
            other.join(" ")
        ))),
    }
}

fn check_flags(flags: &[&str]) -> Result<(Selection, Format), Broken> {
    let mut all = false;
    let mut changed: Option<Vec<String>> = None;
    let mut format = Format::Text;
    let mut at = 0;
    while at < flags.len() {
        match flags[at] {
            "--all" => all = true,
            "--changed" => {
                let paths: Vec<String> = flags[at + 1..]
                    .iter()
                    .take_while(|arg| !arg.starts_with("--"))
                    .map(|arg| (*arg).to_owned())
                    .collect();
                at += paths.len();
                if paths.is_empty() {
                    return Err(usage("--changed needs at least one path"));
                }
                changed.get_or_insert_with(Vec::new).extend(paths);
            }
            "--format" => {
                at += 1;
                format = match flags.get(at) {
                    Some(&"text") => Format::Text,
                    Some(&"xml") => Format::Xml,
                    _ => return Err(usage("--format is text or xml")),
                };
            }
            other => return Err(usage(&format!("check does not take {other:?}"))),
        }
        at += 1;
    }
    match (all, changed) {
        (true, Some(_)) => Err(usage("check takes --all or --changed, not both")),
        (_, Some(paths)) => Ok((Selection::Changed(paths), format)),
        _ => Ok((Selection::All, format)),
    }
}

/// `--root`, or the nearest folder from the working directory up that holds `.ai/rules/`.
fn find_root(root: Option<PathBuf>, cwd: &Path) -> Result<PathBuf, Broken> {
    if let Some(root) = root {
        return if root.join(".ai/rules").is_dir() {
            Ok(root)
        } else {
            Err(usage(&format!(
                "{} has no .ai/rules folder; --root names the repository",
                root.display()
            )))
        };
    }
    cwd.ancestors()
        .find(|dir| dir.join(".ai/rules").is_dir())
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            usage(&format!(
                "no folder from {} up holds .ai/rules; run bebop inside the repository or give --root",
                cwd.display()
            ))
        })
}

/// A changed path as a path from the repository root.
fn from_root(root: &Path, cwd: &Path, path: &str) -> Result<String, Broken> {
    let given = Path::new(path);
    let absolute = if given.is_absolute() {
        given.to_path_buf()
    } else {
        cwd.join(given)
    };
    match absolute.strip_prefix(root) {
        Ok(inner) => Ok(tree::normalize(&inner.to_string_lossy())),
        Err(_) if !given.is_absolute() && !path.contains("..") => Ok(tree::normalize(path)),
        Err(_) => Err(usage(&format!(
            "{path} is outside the repository at {}",
            root.display()
        ))),
    }
}
