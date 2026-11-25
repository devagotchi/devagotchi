//! Owns: no persistent state; checked filesystem handles remain local to each call.
//! Serves: directory creation, metadata and moves.
//! Settings: the caller scope and existing file size/deadline bounds.
//! Depends on: shared contracts, checked handles and checksum helpers.
//! Managing files: what a path is (`info`), making directories (`mkdir`, and the parents a
//! write asks for) and moving or renaming (`moved`). Reach is the read policy: inside the
//! workspace under `granted`, wherever the account can under `account`. A symbolic link is
//! reported and moved as itself; under `granted` what it points to must be inside the
//! workspace too, judged from where the link is, or would be after a move, and from its
//! text when its target is missing.

use cap_std::fs::Metadata;
use std::fs;
use std::path::{Component, Path, PathBuf};

use devagotchi_contracts::{BridgeError, BridgeResult, DirectoryInput, FileInfo, FileMoveInput};

use super::{Scope, display, invalid, io_error};
use super::{checked, checksum};
use crate::broker::ExecPolicy;
mod info;

/// Most entries a directory move checks for links under `granted`.
const MAX_CHECKED_ENTRIES: usize = 10_000;

fn outside() -> BridgeError {
    BridgeError::new(
        "path_outside_workspace",
        "This machine's broker reaches files only inside its workspace",
    )
}

/// The nearest part of `path` that exists (as itself, links not followed), and the plain
/// names below it that do not. `None` when the missing part is not plain names (`.`, `..`).
fn split_existing(path: &Path) -> Option<(&Path, Vec<&std::ffi::OsStr>)> {
    let mut ancestor = path;
    while fs::symlink_metadata(ancestor).is_err() {
        ancestor = ancestor.parent()?;
    }
    let rest = path.strip_prefix(ancestor).ok()?;
    rest.components()
        .map(|part| match part {
            Component::Normal(name) => Some(name),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()
        .map(|names| (ancestor, names))
}

/// Two names of one file. Unix compares the inode; elsewhere this is only known by path.
#[cfg(unix)]
fn same_file(a: &Metadata, b: &Metadata) -> bool {
    use cap_std::fs::MetadataExt;
    a.dev() == b.dev() && a.ino() == b.ino()
}

#[cfg(not(unix))]
fn same_file(_: &Metadata, _: &Metadata) -> bool {
    false
}

impl Scope<'_> {
    /// `path` canonical up to its own name, without following the name itself: the
    /// directory it is in, resolved, joined with its name (a root is itself).
    pub(super) fn place(&self, path: &str) -> BridgeResult<PathBuf> {
        self.check()?;
        let given = self.given(path)?;
        let place = match (given.parent(), given.file_name()) {
            (Some(parent), Some(name)) => dunce::canonicalize(parent)
                .map_err(|e| io_error(path, e))?
                .join(name),
            _ => dunce::canonicalize(&given).map_err(|e| io_error(path, e))?,
        };
        if self.policy == ExecPolicy::Granted && !place.starts_with(self.workspace) {
            return Err(outside());
        }
        self.check_path(&place)?;
        Ok(place)
    }

    /// Whether `target`, followed from the directory `from`, lands inside the workspace:
    /// the part that exists resolved through any links, the rest plain names. A target
    /// that cannot be shown to land inside does not.
    fn lands_inside(&self, from: &Path, target: &Path) -> bool {
        let joined = if target.is_absolute() {
            target.to_path_buf()
        } else {
            from.join(target)
        };
        let Some((existing, missing)) = split_existing(&joined) else {
            return false;
        };
        dunce::canonicalize(existing).is_ok_and(|real| {
            missing
                .iter()
                .fold(real, |path, name| path.join(name))
                .starts_with(self.workspace)
        })
    }

    /// Under `granted`, a link must point inside the workspace for the broker to touch it.
    pub(super) fn link_inside(
        &self,
        path: &Path,
        metadata: &std::fs::Metadata,
    ) -> BridgeResult<()> {
        if !metadata.file_type().is_symlink() {
            return Ok(());
        }
        self.checked_link(path)
    }

    pub(super) fn checked_link(&self, path: &Path) -> BridgeResult<()> {
        if self.policy != ExecPolicy::Granted {
            return Ok(());
        }
        let entry = checked::Entry::at(path).map_err(|e| io_error(&path.to_string_lossy(), e))?;
        let target = entry
            .parent
            .handle
            .read_link_contents(&entry.name)
            .map_err(|e| io_error(&path.to_string_lossy(), e))?;
        if self.lands_inside(&entry.parent.path, &target) {
            Ok(())
        } else {
            Err(outside())
        }
    }

    /// Make the directory `path` and any missing parents, following nothing that was not
    /// already there: the part that exists is resolved (and under `granted` must be in the
    /// workspace), the missing part must be plain names and is made one name at a time.
    /// Returns the directory, canonical, and whether anything was made.
    pub(super) fn planned_dirs(
        &self,
        path: &str,
    ) -> BridgeResult<(PathBuf, Vec<std::ffi::OsString>)> {
        self.check()?;
        let given = self.given(path)?;
        let Some((existing, missing)) = split_existing(&given) else {
            return Err(invalid(
                "the directories to make must be plain names, without . or ..",
            ));
        };
        let current = dunce::canonicalize(existing).map_err(|e| io_error(path, e))?;
        if self.policy == ExecPolicy::Granted && !current.starts_with(self.workspace) {
            return Err(outside());
        }
        if checked::Directory::open(&current).is_err() {
            return Err(BridgeError::new(
                "not_a_directory",
                format!("{} exists and is not a directory", display(&current)?),
            ));
        }
        Ok((
            current,
            missing.into_iter().map(std::ffi::OsStr::to_owned).collect(),
        ))
    }

    pub(super) fn make_dirs(&self, path: &str) -> BridgeResult<(PathBuf, bool)> {
        let (existing, missing) = self.planned_dirs(path)?;
        let mut current = checked::Directory::open(&existing).map_err(|e| io_error(path, e))?;
        for name in &missing {
            self.check()?;
            self.check_path(&current.path.join(name))?;
            current.verify().map_err(|e| io_error(path, e))?;
            match current.handle.create_dir(name) {
                Ok(()) => {}
                Err(error)
                    if error.kind() == std::io::ErrorKind::AlreadyExists
                        && current
                            .handle
                            .symlink_metadata(name)
                            .is_ok_and(|m| m.is_dir()) => {}
                Err(error) => return Err(io_error(path, error)),
            }
            current = current.child(name).map_err(|e| io_error(path, e))?;
        }
        // The directories exist now: no authority check after them.
        Ok((current.path, !missing.is_empty()))
    }

    pub(crate) fn mkdir(&self, input: DirectoryInput) -> BridgeResult<FileInfo> {
        let (path, made) = self.make_dirs(&input.path)?;
        // Once something was made, describing it never refuses; when nothing was, the
        // caller's authority still applies (a refusal then is true: nothing changed).
        let mut info = if made {
            self.committed().describe(&path)?
        } else {
            self.describe(&path)?
        };
        info.made = Some(made);
        Ok(info)
    }

    /// Under `granted`, a directory moves only when no link inside it can point elsewhere
    /// once moved: none climbs with `..`, and every absolute one lands inside.
    fn links_stay_inside(&self, dir: &Path) -> BridgeResult<()> {
        let mut pending =
            vec![checked::Directory::open(dir).map_err(|e| io_error("move directory", e))?];
        let mut seen = 0;
        while let Some(next) = pending.pop() {
            let shown = next.path.to_string_lossy().into_owned();
            self.check()?;
            next.verify().map_err(|e| io_error(&shown, e))?;
            for entry in next.handle.entries().map_err(|e| io_error(&shown, e))? {
                self.check()?;
                let name = entry.map_err(|e| io_error(&shown, e))?.file_name();
                seen += 1;
                if seen > MAX_CHECKED_ENTRIES {
                    return Err(BridgeError::new(
                        "too_large",
                        format!(
                            "A directory with more than {MAX_CHECKED_ENTRIES} entries is not checked for links here; move it with run"
                        ),
                    ));
                }
                let metadata = next
                    .handle
                    .symlink_metadata(&name)
                    .map_err(|e| io_error(&shown, e))?;
                if metadata.is_dir() {
                    pending.push(next.child(&name).map_err(|e| io_error(&shown, e))?);
                } else if metadata.file_type().is_symlink() {
                    let target = next
                        .handle
                        .read_link_contents(&name)
                        .map_err(|e| io_error(&shown, e))?;
                    let climbs = target
                        .components()
                        .any(|part| matches!(part, Component::ParentDir));
                    if climbs || (target.is_absolute() && !self.lands_inside(dir, &target)) {
                        return Err(outside());
                    }
                }
            }
        }
        Ok(())
    }

    pub(crate) fn moved(&self, input: FileMoveInput) -> BridgeResult<FileInfo> {
        let source = self.place(&input.source)?;
        let shown = display(&source)?;
        let source_entry = checked::Entry::at(&source).map_err(|e| io_error(&shown, e))?;
        let from = source_entry
            .parent
            .handle
            .symlink_metadata(&source_entry.name)
            .map_err(|e| io_error(&shown, e))?;
        if from.is_symlink() {
            self.checked_link(&source)?;
        }
        if let Some(expected) = &input.expected_sha256 {
            if !devagotchi_contracts::whole_sha(expected) {
                return Err(invalid("expected_sha256 must be a whole-file SHA-256"));
            }
            if !from.is_file() {
                return Err(BridgeError::new(
                    "not_a_file",
                    "guarded move requires a regular file",
                ));
            }
            let mut file = source_entry.read().map_err(|e| io_error(&shown, e))?;
            if !checksum::file(self, &mut file, &shown)?.eq_ignore_ascii_case(expected) {
                return Err(checksum::changed());
            }
        }
        let destination = self.place(&input.destination)?;
        if destination == source {
            return Err(invalid("source and destination are the same entry"));
        }
        if from.is_dir() && destination.starts_with(&source) {
            return Err(invalid("a directory cannot move inside itself"));
        }
        let to = display(&destination)?;
        let destination_entry = checked::Entry::at(&destination).map_err(|e| io_error(&to, e))?;
        if self.policy == ExecPolicy::Granted {
            if from.file_type().is_symlink() {
                // A relative link is followed from where it lands.
                let target = source_entry
                    .parent
                    .handle
                    .read_link_contents(&source_entry.name)
                    .map_err(|e| io_error(&shown, e))?;
                if !destination
                    .parent()
                    .is_some_and(|dir| self.lands_inside(dir, &target))
                {
                    return Err(outside());
                }
            } else if from.is_dir() {
                self.links_stay_inside(&source)?;
            }
        }
        // Checked, then renamed: an entry made at the destination in between is replaced
        // when it is a file, as overwrite would.
        let replaced = match destination_entry
            .parent
            .handle
            .symlink_metadata(&destination_entry.name)
        {
            Ok(existing) => {
                let one_file = !from.file_type().is_symlink()
                    && (same_file(&from, &existing)
                        || (cfg!(windows)
                            && from.is_file()
                            && existing.is_file()
                            && dunce::canonicalize(&source).ok()
                                == dunce::canonicalize(&destination).ok()));
                if one_file && shown.eq_ignore_ascii_case(&to) {
                    // The same name in another case, on a filesystem that ignores case.
                    false
                } else if one_file {
                    // Two names of one file: a move leaves only the destination's name.
                    if !input.overwrite {
                        return Err(BridgeError::new(
                            "already_exists",
                            format!(
                                "{to} is the same file as {shown} under another name; pass overwrite to drop {shown}"
                            ),
                        ));
                    }
                    self.check()?;
                    source_entry
                        .parent
                        .verify()
                        .map_err(|e| io_error(&shown, e))?;
                    source_entry
                        .parent
                        .handle
                        .remove_file(&source_entry.name)
                        .map_err(|e| io_error(&shown, e))?;
                    let mut info = self.committed().describe(&destination)?;
                    info.replaced = Some(false);
                    return Ok(info);
                } else if existing.is_dir() {
                    return Err(BridgeError::new(
                        "already_exists",
                        format!("{to} is a directory; a move never replaces one"),
                    ));
                } else if !input.overwrite {
                    return Err(BridgeError::new(
                        "already_exists",
                        format!("{to} exists; pass overwrite to replace it"),
                    ));
                } else if from.is_dir() {
                    return Err(BridgeError::new(
                        "already_exists",
                        format!("{to} exists; a directory never replaces a file"),
                    ));
                } else {
                    true
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(io_error(&to, error)),
        };
        self.check()?;
        source_entry
            .parent
            .verify()
            .map_err(|e| io_error(&shown, e))?;
        destination_entry
            .parent
            .verify()
            .map_err(|e| io_error(&to, e))?;
        source_entry.parent.handle.rename(&source_entry.name, &destination_entry.parent.handle, &destination_entry.name).map_err(|error| {
            if error.kind() == std::io::ErrorKind::CrossesDevices {
                BridgeError::new(
                    "cross_device",
                    format!("{shown} and {to} are on different filesystems; copy it with run, then remove the original"),
                )
            } else {
                io_error(&shown, error)
            }
        })?;
        let mut info = self.committed().describe(&destination)?;
        info.replaced = Some(replaced);
        Ok(info)
    }
}

#[cfg(test)]
mod tests;
