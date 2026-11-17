//! What actually runs when someone asks the broker to run something: a program name, a
//! path, or a script, resolved to the exact program, its leading arguments, and a plain
//! sentence saying how it was chosen. The sentence goes into the operation's detail and the
//! broker log, so a run that did the wrong thing, or nothing, says why.
//!
//! *Owns*: no tables. *Serves*: `Broker::normalize` (the `executable` of `operation_start`)
//! and the shell-line routing of `run` and `command` (`line`). *Settings*: none; it reads
//! `PATH`, `PATHEXT`, `SystemRoot` and `LOCALAPPDATA` from the broker's environment.
//! *Depends on*: `execution::platform::git_bash` (Git Bash for `.sh` on Windows).
//!
//! Pure over a [`Host`] (environment lookups and file facts), so the Windows rules build and
//! are tested on every OS. Paths are strings in the target OS's own form.

pub(crate) mod line;
mod script;

use devagotchi_contracts::BridgeError;

/// The target operating system's conventions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Os {
    Unix,
    Windows,
}

impl Os {
    pub(crate) const HERE: Os = if cfg!(windows) { Os::Windows } else { Os::Unix };

    fn separator(self) -> char {
        match self {
            Os::Unix => '/',
            Os::Windows => '\\',
        }
    }

    fn path_list_separator(self) -> char {
        match self {
            Os::Unix => ':',
            Os::Windows => ';',
        }
    }

    pub(crate) fn is_absolute(self, path: &str) -> bool {
        match self {
            Os::Unix => path.starts_with('/'),
            Os::Windows => {
                let bytes = path.as_bytes();
                path.starts_with("\\\\")
                    || (bytes.len() >= 3
                        && bytes[0].is_ascii_alphabetic()
                        && bytes[1] == b':'
                        && (bytes[2] == b'\\' || bytes[2] == b'/'))
            }
        }
    }

    fn has_separator(self, path: &str) -> bool {
        match self {
            Os::Unix => path.contains('/'),
            Os::Windows => path.contains(['\\', '/']),
        }
    }

    pub(crate) fn join(self, dir: &str, name: &str) -> String {
        let dir = dir.trim_end_matches(['/', '\\']);
        format!("{dir}{}{name}", self.separator())
    }
}

/// What the resolver may ask about the machine.
pub(crate) trait Host {
    fn var(&self, name: &str) -> Option<String>;
    fn kind(&self, path: &str) -> Kind;
    /// The first bytes of a file (up to 256), for its `#!` line or its image header.
    fn head(&self, path: &str) -> Vec<u8>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Missing,
    Directory,
    /// A regular file (after following links); `executable` is the Unix execute bit,
    /// always false on Windows. `size` lets the resolver spot Windows' zero-byte app
    /// execution aliases.
    File {
        executable: bool,
        size: u64,
    },
}

/// The program the broker spawns and the arguments that go before the caller's own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Launch {
    /// The path spawned, as found (not canonicalised), so a program that reads its own name
    /// (rustup's `cargo`, busybox) is the program asked for.
    pub(crate) program: String,
    /// Arguments the launch adds before the caller's: an interpreter's flags and the script.
    pub(crate) lead: Vec<String>,
    /// One sentence: what runs and why.
    pub(crate) how: String,
}

/// Resolve what `operation_start` was given as `executable` (a name, a relative or absolute
/// path, a script) in `cwd`.
pub(crate) fn resolve(
    requested: &str,
    cwd: &str,
    os: Os,
    host: &impl Host,
) -> Result<Launch, BridgeError> {
    if requested.trim().is_empty() || requested.contains('\0') {
        return Err(invalid(
            "executable must name a program: a name on PATH, or a path",
        ));
    }
    let (path, found) = find(requested, cwd, os, host)?;
    script::plan(&path, &found, os, host)
}

/// Where a requested program is: `(path, how it was found)`.
fn find(
    requested: &str,
    cwd: &str,
    os: Os,
    host: &impl Host,
) -> Result<(String, String), BridgeError> {
    let path = if os.is_absolute(requested) {
        requested.to_owned()
    } else if os.has_separator(requested) {
        let relative = requested
            .strip_prefix("./")
            .or_else(|| requested.strip_prefix(".\\"))
            .unwrap_or(requested);
        os.join(cwd, relative)
    } else {
        return on_path(requested, os, host)
            .map(|(path, dir)| (path, format!("found on PATH in {dir}")));
    };
    match host.kind(&path) {
        Kind::File { .. } => Ok((path, "as given".to_owned())),
        Kind::Directory => Err(BridgeError::new(
            "invalid_request",
            format!("{path} is a directory, not a program"),
        )),
        Kind::Missing => Err(BridgeError::new(
            "path_not_found",
            format!("{path} does not exist"),
        )),
    }
}

/// The first `name` on PATH (with each PATHEXT extension on Windows when `name` has none),
/// skipping Windows' Microsoft Store aliases for Python, which open the Store instead of
/// running anything.
pub(crate) fn on_path(
    name: &str,
    os: Os,
    host: &impl Host,
) -> Result<(String, String), BridgeError> {
    let path = host
        .var("PATH")
        .or_else(|| host.var("Path"))
        .unwrap_or_default();
    let names = candidates(name, os, host);
    let mut skipped = None;
    let mut searched = 0;
    for dir in path
        .split(os.path_list_separator())
        .filter(|dir| !dir.trim().is_empty())
    {
        searched += 1;
        for candidate in &names {
            let full = os.join(dir, candidate);
            if let Kind::File { size, executable } = host.kind(&full) {
                if store_alias(&full, size, os) {
                    skipped = Some(full);
                    continue;
                }
                if os == Os::Unix && !executable {
                    continue;
                }
                return Ok((full, dir.to_owned()));
            }
        }
    }
    let alias = match skipped {
        Some(alias) => format!(
            " The Microsoft Store alias {alias} was skipped: it opens the Store instead of running Python. Install Python (winget install Python.Python.3.13) or turn the alias off in Settings > Apps > Advanced app settings > App execution aliases."
        ),
        None => String::new(),
    };
    Err(BridgeError::new(
        "program_not_found",
        format!(
            "`{name}` is not on this machine's PATH ({searched} directories searched).{alias} Give its absolute path, or install it."
        ),
    ))
}

fn candidates(name: &str, os: Os, host: &impl Host) -> Vec<String> {
    if os == Os::Unix {
        return vec![name.to_owned()];
    }
    let extensions = host
        .var("PATHEXT")
        .unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".to_owned());
    let has_extension = extensions.split(';').any(|extension| {
        !extension.is_empty()
            && name
                .to_ascii_lowercase()
                .ends_with(&extension.to_ascii_lowercase())
    });
    if has_extension {
        return vec![name.to_owned()];
    }
    extensions
        .split(';')
        .filter(|extension| !extension.is_empty())
        .map(|extension| format!("{name}{}", extension.to_ascii_lowercase()))
        .collect()
}

/// A zero-byte file under `...\Microsoft\WindowsApps\` named python*: Windows' app
/// execution alias, which opens the Microsoft Store when Python is not installed from it.
fn store_alias(path: &str, size: u64, os: Os) -> bool {
    if os != Os::Windows || size != 0 {
        return false;
    }
    let lower = path.to_ascii_lowercase();
    let name = lower.rsplit(['\\', '/']).next().unwrap_or_default();
    lower.contains("\\microsoft\\windowsapps\\") && name.starts_with("python")
}

fn invalid(message: impl Into<String>) -> BridgeError {
    BridgeError::new("invalid_request", message)
}

/// The machine itself, for the broker.
pub(crate) struct Machine;

impl Host for Machine {
    fn var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }

    fn kind(&self, path: &str) -> Kind {
        match std::fs::metadata(path) {
            Ok(metadata) if metadata.is_dir() => Kind::Directory,
            Ok(metadata) if metadata.is_file() => Kind::File {
                executable: executable_bit(&metadata),
                size: metadata.len(),
            },
            Ok(_) | Err(_) => match std::fs::symlink_metadata(path) {
                // A Windows app execution alias is a reparse point that does not follow.
                Ok(metadata) if !metadata.is_dir() => Kind::File {
                    executable: false,
                    size: 0,
                },
                _ => Kind::Missing,
            },
        }
    }

    fn head(&self, path: &str) -> Vec<u8> {
        use std::io::Read as _;
        let mut head = Vec::with_capacity(256);
        if let Ok(file) = std::fs::File::open(path) {
            let _ = file.take(256).read_to_end(&mut head);
        }
        head
    }
}

#[cfg(unix)]
fn executable_bit(metadata: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn executable_bit(_metadata: &std::fs::Metadata) -> bool {
    false
}

#[cfg(test)]
mod tests;
