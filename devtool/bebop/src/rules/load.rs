//! Reading rules: where they live in the tree, and `rule.toml` read strictly. A field that is
//! missing, unknown, of the wrong type or out of its range is an error that names the file and
//! the field, and no rule loads while one is malformed.

use std::{fmt, path::Path};

use serde::{Deserialize, de::DeserializeOwned};

use super::model::{Approval, Case, Check, Guidance, Rule, ScopeSpec, Severity, Status};
use crate::{
    engines,
    tree::{self, Tree},
};

/// A malformed rule: the file, and what is wrong with which field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleError {
    pub file: String,
    pub message: String,
}

impl fmt::Display for RuleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.file, self.message)
    }
}

/// Every rule under every `.ai/rules/` in the tree, sorted by id.
pub fn load(tree: &Tree) -> Result<Vec<Rule>, Vec<RuleError>> {
    let mut rules = Vec::new();
    let mut errors = Vec::new();
    let homes: Vec<&str> = tree
        .dirs()
        .filter(|dir| *dir == ".ai/rules" || dir.ends_with("/.ai/rules"))
        .collect();
    for home in homes {
        for category in tree.child_dirs(home) {
            for folder in tree.child_dirs(category) {
                let file = format!("{folder}/rule.toml");
                if !tree.is_file(&file) {
                    errors.push(RuleError {
                        file: folder.to_owned(),
                        message: "a folder under .ai/rules/<category>/ is a rule and holds \
                                  rule.toml: add it, or move the folder out of .ai/rules"
                            .to_owned(),
                    });
                    continue;
                }
                let text = match tree.read(&file) {
                    Ok(bytes) => String::from_utf8_lossy(&bytes).replace("\r\n", "\n"),
                    Err(e) => {
                        errors.push(RuleError {
                            file: file.clone(),
                            message: e,
                        });
                        continue;
                    }
                };
                match parse(&text, folder, tree) {
                    Ok(rule) => rules.push(rule),
                    Err(found) => errors.extend(found),
                }
            }
        }
    }
    rules.sort_by(|a, b| a.id.cmp(&b.id));
    for pair in rules.windows(2) {
        if pair[0].id == pair[1].id {
            errors.push(RuleError {
                file: pair[1].file(),
                message: format!(
                    "id: {} is also the id of {}; every rule has its own id",
                    pair[1].id,
                    pair[0].file()
                ),
            });
        }
    }
    if errors.is_empty() {
        Ok(rules)
    } else {
        Err(errors)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRule {
    id: String,
    category: String,
    statement: String,
    why: String,
    sources: Vec<String>,
    severity: Severity,
    status: Status,
    version: u32,
    scope: ScopeSpec,
    check: toml::Table,
    guidance: Guidance,
    approval: Option<Approval>,
}

/// One `rule.toml`, from the rule folder `folder` (a path from the tree's root). `tree` is where
/// the guidance's example must exist.
pub fn parse(text: &str, folder: &str, tree: &Tree) -> Result<Rule, Vec<RuleError>> {
    let file = format!("{folder}/rule.toml");
    let error = |message: String| RuleError {
        file: file.clone(),
        message,
    };
    let raw: RawRule = toml::from_str(text).map_err(|e| {
        vec![error(
            e.to_string()
                .trim_end()
                .replace('\n', " ")
                .replace("  ", " "),
        )]
    })?;
    let check = parse_check(raw.check).map_err(|m| vec![error(m)])?;
    let mut problems = Vec::new();

    let name = tree::name(folder);
    let category = tree::name(tree::parent(folder));
    if raw.category != category {
        problems.push(format!(
            "category: {:?}, but the rule sits in the folder {category:?}: they are the same",
            raw.category
        ));
    }
    let expected_id = format!("{category}.{name}");
    if raw.id != expected_id {
        problems.push(format!(
            "id: {:?}, but its folder makes it {expected_id:?} (<category>.<folder name>)",
            raw.id
        ));
    }
    if !engines::is_kebab(name) || !engines::is_kebab(category) {
        problems.push(format!(
            "id: {expected_id:?}: the category and the rule folder are kebab-case"
        ));
    }
    for (field, value) in [
        ("statement", &raw.statement),
        ("why", &raw.why),
        ("guidance.expected", &raw.guidance.expected),
        ("guidance.fix", &raw.guidance.fix),
        ("guidance.example", &raw.guidance.example),
    ] {
        if value.trim().is_empty() {
            problems.push(format!("{field}: empty; say it in full"));
        }
    }
    if raw.sources.is_empty() && !raw.why.to_lowercase().contains("no upstream source") {
        problems.push(
            "sources: empty. Name the upstream practice the rule follows, or say in why that \
             there is no upstream source and why"
                .to_owned(),
        );
    }
    if raw.version == 0 {
        problems.push("version: starts at 1 and goes up with each change".to_owned());
    }
    match (&raw.approval, raw.status) {
        (None, Status::Approved | Status::Enforced) => problems.push(
            "approval: an approved or enforced rule records who approved it ([approval] by, date)"
                .to_owned(),
        ),
        (Some(approval), _) => {
            if approval.by.trim().is_empty() {
                problems.push("approval.by: empty; name who approved the rule".to_owned());
            }
            if !is_date(&approval.date) {
                problems.push(format!(
                    "approval.date: {:?} is not a date as YYYY-MM-DD",
                    approval.date
                ));
            }
        }
        (None, _) => {}
    }
    if raw.scope.include.is_empty() {
        problems.push("scope.include: empty; a rule covers at least one glob".to_owned());
    }
    for (field, patterns) in [
        ("scope.include", &raw.scope.include),
        ("scope.exclude", &raw.scope.exclude),
    ] {
        if let Err(e) = tree::globs(patterns) {
            problems.push(format!("{field}: {e}"));
        }
    }
    let example = tree::normalize(&raw.guidance.example);
    if example.starts_with('/') || example.split('/').any(|part| part == "..") {
        problems.push(format!(
            "guidance.example: {example:?} is a path from the repository root, inside it"
        ));
    } else if !example.is_empty() && !tree.is_file(&example) && !tree.is_dir(&example) {
        problems.push(format!(
            "guidance.example: {example:?} does not exist; name a real path that shows the rule kept"
        ));
    }
    let known = engines::vars(&check);
    for (field, text) in [
        ("guidance.expected", &raw.guidance.expected),
        ("guidance.fix", &raw.guidance.fix),
    ] {
        for var in placeholders(text) {
            if !known.contains(&var) {
                problems.push(format!(
                    "{field}: {{{var}}} is not a value the {} engine gives; it gives {}",
                    check.engine(),
                    known.join(", ")
                ));
            }
        }
    }
    problems.extend(check_params(&check));

    if !problems.is_empty() {
        return Err(problems.into_iter().map(error).collect());
    }
    let folder_on_disk = tree.root().join(Path::new(folder));
    Ok(Rule {
        id: raw.id,
        category: raw.category,
        statement: raw.statement.trim().to_owned(),
        why: raw.why.trim().to_owned(),
        sources: raw.sources,
        severity: raw.severity,
        status: raw.status,
        version: raw.version,
        scope: raw.scope,
        check,
        guidance: Guidance {
            expected: raw.guidance.expected.trim().to_owned(),
            fix: raw.guidance.fix.trim().to_owned(),
            example,
        },
        approval: raw.approval,
        folder: folder_on_disk,
        folder_path: folder.to_owned(),
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NoParams {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MaxLines {
    max: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Patterns {
    patterns: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Names {
    names: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FolderCase {
    case: Case,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Prefix {
    prefix: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Allowed {
    allowed: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Roles {
    roots: Vec<String>,
    allowed_files: Vec<String>,
    allowed_dirs: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BuildContext {
    manifest: String,
    ignore_file: String,
}

const ENGINES: &str = "max_lines, module_home, forbidden_file_name, forbidden_folder_name, \
                       folder_case, crate_name_matches_folder, layer_folders, domain_role_files, \
                       members_in_build_context";

fn parse_check(mut table: toml::Table) -> Result<Check, String> {
    let engine = match table.remove("engine") {
        Some(toml::Value::String(engine)) => engine,
        Some(other) => {
            return Err(format!(
                "check.engine: {other} is not an engine name; one of {ENGINES}"
            ));
        }
        None => return Err(format!("check.engine: missing; one of {ENGINES}")),
    };
    Ok(match engine.as_str() {
        "max_lines" => {
            let MaxLines { max } = params(table, &engine)?;
            Check::MaxLines { max }
        }
        "module_home" => {
            let NoParams {} = params(table, &engine)?;
            Check::ModuleHome
        }
        "forbidden_file_name" => {
            let Patterns { patterns } = params(table, &engine)?;
            Check::ForbiddenFileName { patterns }
        }
        "forbidden_folder_name" => {
            let Names { names } = params(table, &engine)?;
            Check::ForbiddenFolderName { names }
        }
        "folder_case" => {
            let FolderCase { case } = params(table, &engine)?;
            Check::FolderCase { case }
        }
        "crate_name_matches_folder" => {
            let Prefix { prefix } = params(table, &engine)?;
            Check::CrateNameMatchesFolder { prefix }
        }
        "layer_folders" => {
            let Allowed { allowed } = params(table, &engine)?;
            Check::LayerFolders { allowed }
        }
        "members_in_build_context" => {
            let BuildContext {
                manifest,
                ignore_file,
            } = params(table, &engine)?;
            Check::MembersInBuildContext {
                manifest,
                ignore_file,
            }
        }
        "domain_role_files" => {
            let Roles {
                roots,
                allowed_files,
                allowed_dirs,
            } = params(table, &engine)?;
            Check::DomainRoleFiles {
                roots,
                allowed_files,
                allowed_dirs,
            }
        }
        other => {
            return Err(format!(
                "check.engine: {other:?} is not an engine; one of {ENGINES}"
            ));
        }
    })
}

fn params<T: DeserializeOwned>(table: toml::Table, engine: &str) -> Result<T, String> {
    toml::Value::Table(table)
        .try_into()
        .map_err(|e| format!("check ({engine}): {}", e.to_string().trim_end()))
}

fn check_params(check: &Check) -> Vec<String> {
    let mut problems = Vec::new();
    let mut nonempty = |field: &str, list: &[String]| {
        if list.is_empty() || list.iter().any(|item| item.trim().is_empty()) {
            problems.push(format!("check.{field}: empty, or holds an empty entry"));
        }
    };
    match check {
        Check::MaxLines { max } if *max == 0 => {
            problems.push("check.max: at least 1".to_owned());
        }
        Check::ForbiddenFileName { patterns } => {
            nonempty("patterns", patterns);
            if let Err(e) = tree::globs(patterns) {
                problems.push(format!("check.patterns: {e}"));
            }
        }
        Check::ForbiddenFolderName { names } => nonempty("names", names),
        Check::CrateNameMatchesFolder { prefix } if prefix.trim().is_empty() => {
            problems.push("check.prefix: empty".to_owned());
        }
        Check::LayerFolders { allowed } => nonempty("allowed", allowed),
        Check::DomainRoleFiles {
            roots,
            allowed_files,
            allowed_dirs,
        } => {
            nonempty("roots", roots);
            nonempty("allowed_files", allowed_files);
            nonempty("allowed_dirs", allowed_dirs);
        }
        _ => {}
    }
    problems
}

/// The `{name}`s in a guidance text.
pub fn placeholders(text: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close)
                if close > 0
                    && after[..close]
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c == '_') =>
            {
                found.push(&after[..close]);
                rest = &after[close + 1..];
            }
            _ => rest = after,
        }
    }
    found
}

fn is_date(text: &str) -> bool {
    let parts: Vec<&str> = text.split('-').collect();
    let number = |part: &str, len: usize| {
        (part.len() == len && part.chars().all(|c| c.is_ascii_digit()))
            .then(|| part.parse::<u32>().ok())
            .flatten()
    };
    match parts.as_slice() {
        [year, month, day] => {
            number(year, 4).is_some()
                && number(month, 2).is_some_and(|m| (1..=12).contains(&m))
                && number(day, 2).is_some_and(|d| (1..=31).contains(&d))
        }
        _ => false,
    }
}
