//! What a rule is, once read and checked: the fields of `rule.toml` as types.

use std::path::PathBuf;

use serde::Deserialize;

/// One rule: `<boundary>/.ai/rules/<category>/<name>/rule.toml`.
#[derive(Debug, Clone)]
pub struct Rule {
    /// `<category>.<name>`, the rule folder's own name after its category.
    pub id: String,
    pub category: String,
    /// The operator's words.
    pub statement: String,
    /// Why, with the upstream practice it follows.
    pub why: String,
    /// The upstream references; empty only when `why` says there is none.
    pub sources: Vec<String>,
    pub severity: Severity,
    pub status: Status,
    pub version: u32,
    pub scope: ScopeSpec,
    pub check: Check,
    pub guidance: Guidance,
    pub approval: Option<Approval>,
    /// The rule's folder, on disk.
    pub folder: PathBuf,
    /// The rule's folder from the repository root, `/`-separated.
    pub folder_path: String,
}

impl Rule {
    /// `rule.toml`, from the repository root.
    pub fn file(&self) -> String {
        format!("{}/rule.toml", self.folder_path)
    }

    /// `baseline.txt` beside `rule.toml`, from the repository root.
    pub fn baseline_file(&self) -> String {
        format!("{}/baseline.txt", self.folder_path)
    }

    pub fn baseline_path(&self) -> PathBuf {
        self.folder.join("baseline.txt")
    }

    pub fn examples(&self, which: &str) -> PathBuf {
        self.folder.join("examples").join(which)
    }

    pub fn is_enforced(&self) -> bool {
        self.status == Status::Enforced
    }

    pub fn in_folder(&self, path: &str) -> bool {
        path == self.folder_path
            || path
                .strip_prefix(self.folder_path.as_str())
                .is_some_and(|rest| rest.starts_with('/'))
    }
}

/// B0 has one severity: a violation is an error and fails the check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
        }
    }
}

/// Where a rule is in its life. Only an enforced rule fails `bebop check`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Proposed,
    Approved,
    Enforced,
    Retired,
}

/// The paths a rule covers, as globs from the repository root.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopeSpec {
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// What a finding tells its reader. `expected` and `fix` may name the finding's own values as
/// `{name}` (each engine lists the names it gives, [`crate::engines::vars`]).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Guidance {
    pub expected: String,
    pub fix: String,
    /// A real path in the repository that shows the rule kept.
    pub example: String,
}

/// Who approved the rule, and when (required once it is approved or enforced).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Approval {
    pub by: String,
    pub date: String,
}

/// The engine a rule runs, with its parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Check {
    /// A file in scope holds at most `max` lines, counted as `wc -l` counts them.
    MaxLines { max: usize },
    /// No Rust module is both `<x>.rs` and a folder `<x>/` of Rust files.
    ModuleHome,
    /// No file in scope has a name these globs match.
    ForbiddenFileName { patterns: Vec<String> },
    /// No folder in scope has one of these names.
    ForbiddenFolderName { names: Vec<String> },
    /// Every folder in scope is named in this case.
    FolderCase { case: Case },
    /// Every crate's package name is `prefix` and its folder's name.
    CrateNameMatchesFolder { prefix: String },
    /// The top-level folders in scope are exactly these.
    LayerFolders { allowed: Vec<String> },
    /// Inside each root, a domain holds only its role files and folders.
    DomainRoleFiles {
        roots: Vec<String>,
        allowed_files: Vec<String>,
        allowed_dirs: Vec<String>,
    },
}

impl Check {
    pub fn engine(&self) -> &'static str {
        match self {
            Check::MaxLines { .. } => "max_lines",
            Check::ModuleHome => "module_home",
            Check::ForbiddenFileName { .. } => "forbidden_file_name",
            Check::ForbiddenFolderName { .. } => "forbidden_folder_name",
            Check::FolderCase { .. } => "folder_case",
            Check::CrateNameMatchesFolder { .. } => "crate_name_matches_folder",
            Check::LayerFolders { .. } => "layer_folders",
            Check::DomainRoleFiles { .. } => "domain_role_files",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Case {
    Kebab,
}
