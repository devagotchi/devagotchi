//! Stack detection from a repository's files: deterministic first, as Stack Category Routing
//! says. A marker is a file name (`Cargo.toml`), a file name with `*` (`next.config.*`,
//! `*.sql`), a path (`src/main.rs`, matched at the end of a path), or a directory (`app/`,
//! any path with that directory in it).

use super::{Concept, Dictionary, Scheme};

pub(super) fn stacks_for<'a>(
    dictionary: &'a Dictionary,
    files: &[&str],
    packages: &[&str],
) -> Vec<&'a Concept> {
    let files: Vec<String> = files.iter().map(|f| f.replace('\\', "/")).collect();
    let found: Vec<&Concept> = dictionary
        .concepts()
        .iter()
        .filter(|c| c.scheme == Scheme::Stack)
        .filter(|c| {
            c.markers.iter().any(|m| files.iter().any(|f| marks(m, f)))
                || c.packages.iter().any(|p| packages.contains(&p.as_str()))
        })
        .collect();
    found
        .iter()
        .copied()
        .filter(|c| !c.yields_to.iter().any(|y| found.iter().any(|f| &f.id == y)))
        .collect()
}

/// Whether `marker` marks the file at `path` (`/`-separated, relative to the repository).
pub(super) fn marks(marker: &str, path: &str) -> bool {
    let path = path.trim_start_matches("./");
    if let Some(dir) = marker.strip_suffix('/') {
        let mut parts: Vec<&str> = path.split('/').collect();
        parts.pop();
        return parts.contains(&dir);
    }
    if marker.contains('/') {
        let marker_parts: Vec<&str> = marker.split('/').collect();
        let parts: Vec<&str> = path.split('/').collect();
        return parts.len() >= marker_parts.len()
            && parts[parts.len() - marker_parts.len()..]
                .iter()
                .zip(&marker_parts)
                .all(|(p, m)| glob(m, p));
    }
    glob(marker, path.rsplit('/').next().unwrap_or(path))
}

/// `*` matches any run of characters; everything else matches itself.
fn glob(pattern: &str, name: &str) -> bool {
    let pieces: Vec<&str> = pattern.split('*').collect();
    if pieces.len() == 1 {
        return pattern == name;
    }
    let (first, last) = (pieces[0], pieces[pieces.len() - 1]);
    if !name.starts_with(first) || !name[first.len()..].ends_with(last) {
        return false;
    }
    let mut rest = &name[first.len()..name.len() - last.len()];
    for piece in &pieces[1..pieces.len() - 1] {
        match rest.find(piece) {
            Some(at) => rest = &rest[at + piece.len()..],
            None => return false,
        }
    }
    name.len() >= first.len() + last.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_match_names_globs_paths_and_directories() {
        assert!(marks("Cargo.toml", "crates/kernel/Cargo.toml"));
        assert!(!marks("Cargo.toml", "Cargo.toml.bak"));
        assert!(marks("next.config.*", "web/next.config.mjs"));
        assert!(marks("*.sql", "migrations/0001_init.sql"));
        assert!(!marks("*.sql", "notes.sqlite"));
        assert!(marks("src/main.rs", "crates/x/src/main.rs"));
        assert!(!marks("src/main.rs", "main.rs"));
        assert!(marks("app/", "web/app/page.tsx"));
        assert!(!marks("app/", "app"));
        assert!(marks("a*b*c", "aXbYc"));
        assert!(!marks("ab*ba", "aba"));
    }
}
