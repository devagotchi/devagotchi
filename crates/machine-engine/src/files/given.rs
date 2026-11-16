//! A path as an agent writes it, made absolute before anything else looks at it: `~` is the
//! broker account's home, a relative path is under the workspace, and an absolute path is
//! itself. Every file tool and an operation's `cwd` go through here, so they read a path the
//! same way.

use std::path::{Path, PathBuf};

use devagotchi_contracts::{BridgeError, BridgeResult};

/// `path` made absolute. `base` is where a relative path starts (the workspace); with no
/// base a relative path is refused, which is what a setting in broker.json wants.
pub(crate) fn absolute(path: &str, base: Option<&Path>) -> BridgeResult<PathBuf> {
    absolute_with(path, base, std::env::home_dir)
}

fn absolute_with(
    path: &str,
    base: Option<&Path>,
    home: impl FnOnce() -> Option<PathBuf>,
) -> BridgeResult<PathBuf> {
    if path.as_bytes().contains(&0) {
        return Err(invalid("path must not contain NUL bytes"));
    }
    if path.trim().is_empty() {
        return Err(invalid("path is empty"));
    }
    if let Some(rest) = path.strip_prefix('~') {
        let rest = match rest.chars().next() {
            None => "",
            Some('/') => &rest[1..],
            Some('\\') if cfg!(windows) => &rest[1..],
            Some(_) => {
                return Err(invalid(format!(
                    "{path}: only ~ (this broker account's home) is understood, not another account's ~name; give the full path"
                )));
            }
        };
        let home = home().ok_or_else(|| {
            BridgeError::new("invalid_request", format!("{path}: this broker's account has no home directory, so ~ means nothing here; give the full path"))
        })?;
        return Ok(if rest.is_empty() { home } else { home.join(rest) });
    }
    let given = Path::new(path);
    if given.is_absolute() {
        return Ok(given.to_path_buf());
    }
    match base {
        Some(base) => Ok(base.join(given)),
        None => Err(invalid(format!("{path} must be an absolute path (or start with ~)"))),
    }
}

fn invalid(message: impl Into<String>) -> BridgeError {
    BridgeError::new("invalid_request", message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> Option<PathBuf> {
        Some(PathBuf::from("/home/ada"))
    }

    #[test]
    fn tilde_is_the_account_home_and_relative_is_under_the_workspace() {
        let workspace = Path::new("/srv/work");
        let made = |path| absolute_with(path, Some(workspace), home).unwrap();
        assert_eq!(made("~"), PathBuf::from("/home/ada"));
        assert_eq!(made("~/notes/a.md"), PathBuf::from("/home/ada/notes/a.md"));
        assert_eq!(made("src/main.rs"), PathBuf::from("/srv/work/src/main.rs"));
        assert_eq!(made("./x"), PathBuf::from("/srv/work/./x"));
        assert_eq!(made("/etc/hosts"), PathBuf::from("/etc/hosts"));
    }

    #[test]
    fn what_cannot_be_read_says_why() {
        let refused = |path, base| absolute_with(path, base, home).unwrap_err().message;
        assert!(refused("~bob/x", Some(Path::new("/w"))).contains("not another account's ~name"));
        assert!(refused("relative", None).contains("must be an absolute path"));
        assert!(refused("", Some(Path::new("/w"))).contains("empty"));
        assert!(refused("a\0b", Some(Path::new("/w"))).contains("NUL"));
        let homeless = absolute_with("~/x", None, || None).unwrap_err().message;
        assert!(homeless.contains("no home directory"));
    }
}
