//! `members_in_build_context`: every workspace member the root `Cargo.toml` names is inside the
//! image's build context. The context is an allowlist ignore file (`*`, then `!<folder>/` for
//! each folder let in, as `.dockerignore` is written here), so a member outside every `!` line
//! is invisible to the image build, and `cargo` there fails to load the workspace. An ignore
//! file with no `*` line excludes nothing by default, so every member is inside it.

use super::Finding;
use crate::tree::{self, Tree};

pub fn run(tree: &Tree, manifest: &str, ignore_file: &str) -> Result<Vec<Finding>, String> {
    let text = String::from_utf8_lossy(&tree.read(manifest)?).into_owned();
    let root: toml::Table =
        toml::from_str(&text).map_err(|e| format!("{manifest}: not a readable Cargo.toml: {e}"))?;
    let members: Vec<String> = root
        .get("workspace")
        .and_then(|workspace| workspace.get("members"))
        .and_then(toml::Value::as_array)
        .ok_or_else(|| format!("{manifest}: has no [workspace] members"))?
        .iter()
        .map(|member| {
            member
                .as_str()
                .map(tree::normalize)
                .ok_or_else(|| format!("{manifest}: a workspace member is not a string"))
        })
        .collect::<Result<_, _>>()?;
    let ignore = String::from_utf8_lossy(&tree.read(ignore_file)?).into_owned();
    let lines: Vec<&str> = ignore
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    if !lines.contains(&"*") {
        return Ok(Vec::new());
    }
    let allowed: Vec<String> = lines
        .iter()
        .filter_map(|line| line.strip_prefix('!'))
        .map(|line| tree::normalize(line.trim_end_matches('/')))
        .collect();
    let inside = |member: &str| {
        allowed
            .iter()
            .any(|let_in| member == let_in || member.starts_with(&format!("{let_in}/")))
    };
    Ok(members
        .iter()
        .filter(|member| !inside(member))
        .map(|member| {
            Finding::new(ignore_file)
                .with("member", member.clone())
                .with("top", member.split('/').next().unwrap_or(member).to_owned())
                .with("manifest", manifest)
        })
        .collect())
}
