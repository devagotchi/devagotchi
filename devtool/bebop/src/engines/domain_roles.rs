//! `domain_role_files`: inside each root (`engine/hub/src`), every folder is a domain, and a
//! domain holds only its role files (N5: `mod.rs`, `model.rs`, `store.rs`, …), its role
//! folders (`web/`, `tests/`, which are not looked inside) and sub-domain folders, which follow
//! the same rule and have their own `mod.rs` front door.

use super::Finding;
use crate::tree::{self, Scope, Tree};

struct Roles<'a> {
    files: &'a [String],
    dirs: &'a [String],
    listed: String,
}

pub fn run(
    tree: &Tree,
    scope: &Scope,
    roots: &[String],
    allowed_files: &[String],
    allowed_dirs: &[String],
) -> Vec<Finding> {
    let roles = Roles {
        files: allowed_files,
        dirs: allowed_dirs,
        listed: allowed_files
            .iter()
            .map(|file| format!("`{file}`"))
            .chain(allowed_dirs.iter().map(|dir| format!("`{dir}/`")))
            .collect::<Vec<_>>()
            .join(", "),
    };
    let mut found = Vec::new();
    for root in roots {
        let root = tree::normalize(root);
        for domain in tree.child_dirs(&root) {
            if scope.contains(domain) {
                domain_files(tree, scope, domain, &roles, &mut found);
            }
        }
    }
    found
}

fn domain_files(tree: &Tree, scope: &Scope, domain: &str, roles: &Roles, found: &mut Vec<Finding>) {
    let name = tree::name(domain);
    if !tree.is_file(&format!("{domain}/mod.rs")) {
        found.push(
            Finding::new(domain)
                .with("name", format!("{name}/"))
                .with("domain", domain)
                .with("roles", roles.listed.as_str())
                .with(
                    "detail",
                    format!("`{name}/` is a domain folder without a `mod.rs` front door"),
                )
                .with(
                    "remedy",
                    format!(
                        "give `{domain}/` a `mod.rs` front door (Owns, Serves, Settings, Depends \
                         on) so it is a sub-domain, or move what it holds into `web/`, `tests/` \
                         or its domain's role files"
                    ),
                ),
        );
    }
    for file in tree.child_files(domain) {
        let file_name = tree::name(file);
        if !file_name.ends_with(".rs")
            || !scope.contains(file)
            || roles.files.iter().any(|role| role == file_name)
        {
            continue;
        }
        found.push(
            Finding::new(file)
                .with("name", file_name)
                .with("domain", domain)
                .with("roles", roles.listed.as_str())
                .with(
                    "detail",
                    format!("`{file_name}` is not one of the role files"),
                )
                .with(
                    "remedy",
                    format!(
                        "move what `{file_name}` holds into the role file it belongs to (types and \
                         rules in `model.rs`, Postgres in `store.rs`, operations in `service.rs`, \
                         MCP tools in `mcp.rs`, the JSON API in `http.rs`, pages in `web/`, verbs \
                         in `cli.rs`), or into a sub-domain folder of `{domain}/` with its own \
                         `mod.rs`"
                    ),
                ),
        );
    }
    for dir in tree.child_dirs(domain) {
        if scope.contains(dir) && !roles.dirs.iter().any(|role| role == tree::name(dir)) {
            domain_files(tree, scope, dir, roles, found);
        }
    }
}
