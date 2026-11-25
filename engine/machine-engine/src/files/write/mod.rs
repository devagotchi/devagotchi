//! Owns: no persistent state; checked filesystem handles remain local to each call.
//! Serves: guarded writes and exact text edits.
//! Settings: the caller scope and existing file size/deadline bounds.
//! Depends on: shared contracts, checked handles and checksum helpers.
//! Changing files: write (overwrite, create, append) and edit (exact text replacement).
//! A whole-content change is written to a new file beside the target and renamed into
//! place, so a reader sees the old content or the new, never half of each; the target's
//! permissions carry over. Reach is the read policy: inside the workspace under `granted`,
//! wherever the account can write under `account`.

use cap_std::fs::OpenOptions;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use devagotchi_contracts::{
    BridgeError, BridgeResult, FileEditInput, FileWriteInput, FileWritten, MAX_FILE_WRITE_BYTES,
    WriteMode,
};

use super::{Scope, checked, checksum, display, invalid, io_error};
use crate::broker::ExecPolicy;

/// Largest file an edit reads whole.
const MAX_EDIT_BYTES: u64 = 8 * 1024 * 1024;

/// Replace through the held parent. Create uses a hard link so a concurrent new
/// destination is never overwritten. The temporary handle supplies the receipt.
fn replace(
    scope: &Scope<'_>,
    entry: &checked::Entry,
    bytes: &[u8],
    create: bool,
) -> BridgeResult<fs::File> {
    let shown = entry.parent.path.join(&entry.name).display().to_string();
    let temporary =
        std::ffi::OsString::from(format!(".devagotchi-{}", uuid::Uuid::new_v4().simple()));
    scope.check()?;
    let mut file = entry
        .parent
        .file(
            &temporary,
            OpenOptions::new().read(true).write(true).create_new(true),
        )
        .map_err(|e| io_error(&shown, e))?;
    let written = (|| {
        file.write_all(bytes).map_err(|e| io_error(&shown, e))?;
        match entry.parent.handle.symlink_metadata(&entry.name) {
            Ok(previous) => {
                let permissions = previous
                    .permissions()
                    .into_std(&file)
                    .map_err(|e| io_error(&shown, e))?;
                file.set_permissions(permissions)
                    .map_err(|e| io_error(&shown, e))?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(io_error(&shown, error)),
        }
        file.sync_all().map_err(|e| io_error(&shown, e))?;
        scope.check()?;
        entry.parent.verify().map_err(|e| io_error(&shown, e))?;
        if create {
            match entry
                .parent
                .handle
                .hard_link(&temporary, &entry.parent.handle, &entry.name)
            {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    return Err(appeared());
                }
                // FAT32, exFAT, some SMB and FUSE mounts and Android storage have no hard
                // links: create the file itself, exclusively, so an existing one is still
                // never replaced. A reader there can see it before its last byte lands.
                Err(e) if links_unsupported(&e) => create_exclusive(entry, bytes, &shown)?,
                Err(e) => return Err(io_error(&shown, e)),
            }
            entry
                .parent
                .handle
                .remove_file(&temporary)
                .map_err(|e| io_error(&shown, e))?;
        } else {
            entry
                .parent
                .handle
                .rename(&temporary, &entry.parent.handle, &entry.name)
                .map_err(|e| io_error(&shown, e))?;
        }
        Ok(())
    })();
    if written.is_err() {
        let _ = entry.parent.handle.remove_file(&temporary);
    }
    written?;
    Ok(file)
}

fn appeared() -> BridgeError {
    BridgeError::new(
        "already_exists",
        "destination appeared before create; it was not replaced",
    )
}

/// The OS said this filesystem makes no hard links (rather than that the name exists).
fn links_unsupported(error: &std::io::Error) -> bool {
    // Linux: EPERM, EOPNOTSUPP. Windows: ERROR_INVALID_FUNCTION, ERROR_NOT_SUPPORTED.
    #[cfg(unix)]
    const CODES: [i32; 2] = [1, 95];
    #[cfg(windows)]
    const CODES: [i32; 2] = [1, 50];
    error.kind() == std::io::ErrorKind::Unsupported
        || error
            .raw_os_error()
            .is_some_and(|code| CODES.contains(&code))
}

/// Create for a filesystem with no hard links: the target opened with `create_new`, so a
/// file that exists or appears is refused, never replaced; a failed write removes it.
fn create_exclusive(entry: &checked::Entry, bytes: &[u8], shown: &str) -> BridgeResult<()> {
    let mut file = entry
        .parent
        .file(&entry.name, OpenOptions::new().write(true).create_new(true))
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                appeared()
            } else {
                io_error(shown, e)
            }
        })?;
    let written = file
        .write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| io_error(shown, e));
    if written.is_err() {
        let _ = entry.parent.handle.remove_file(&entry.name);
    }
    written
}

impl Scope<'_> {
    /// Where a write lands: an existing file resolves through any links; a new one is its
    /// canonical parent directory joined with its name.
    fn target(&self, path: &str, create_dirs: bool) -> BridgeResult<(PathBuf, bool)> {
        let given = &self.given(path)?;
        let (canonical, exists) = match dunce::canonicalize(given) {
            Ok(canonical) => (canonical, true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let parent = given
                    .parent()
                    .ok_or_else(|| invalid("path must name a file"))?;
                let name = given
                    .file_name()
                    .ok_or_else(|| invalid("path must name a file"))?;
                // A link whose target is missing: writing through it makes the target.
                if let Ok(link) = fs::symlink_metadata(given) {
                    self.link_inside(given, &link)?;
                }
                let parent = if create_dirs {
                    let parent = parent
                        .to_str()
                        .ok_or_else(|| invalid("path must be UTF-8"))?;
                    {
                        let (existing, missing) = self.planned_dirs(parent)?;
                        missing.iter().fold(existing, |path, name| path.join(name))
                    }
                } else {
                    dunce::canonicalize(parent).map_err(|e| io_error(path, e))?
                };
                (parent.join(name), false)
            }
            Err(error) => return Err(io_error(path, error)),
        };
        if self.policy == ExecPolicy::Granted && !canonical.starts_with(self.workspace) {
            return Err(outside());
        }
        if exists && !fs::metadata(&canonical).is_ok_and(|m| m.is_file()) {
            return Err(BridgeError::new(
                "not_a_file",
                format!("{path} exists and is not a regular file"),
            ));
        }
        self.check_path(&canonical)?;
        Ok((canonical, exists))
    }

    pub(crate) fn write(&self, input: FileWriteInput) -> BridgeResult<FileWritten> {
        let bytes = match (&input.text, &input.bytes_base64) {
            (Some(text), None) => text.as_bytes().to_vec(),
            (None, Some(encoded)) => BASE64
                .decode(encoded)
                .map_err(|_| invalid("bytes_base64 is not valid base64"))?,
            _ => return Err(invalid("Give exactly one of text or bytes_base64")),
        };
        if bytes.len() > MAX_FILE_WRITE_BYTES {
            return Err(invalid(format!(
                "At most {MAX_FILE_WRITE_BYTES} bytes per call; write the rest with mode append"
            )));
        }
        let (target, exists) = self.target(&input.path, input.create_dirs)?;
        let shown = display(&target)?;
        if exists && input.mode == WriteMode::Create {
            return Err(BridgeError::new(
                "already_exists",
                format!("{shown} exists; use mode overwrite to replace it"),
            ));
        }
        if let Some(expected) = &input.expected_sha256 {
            let current = if exists {
                {
                    let entry = checked::Entry::at(&target).map_err(|e| io_error(&shown, e))?;
                    let mut file = entry.read().map_err(|e| io_error(&shown, e))?;
                    checksum::file(self, &mut file, &shown)?
                }
            } else {
                String::new()
            };
            if !current.eq_ignore_ascii_case(expected) {
                return Err(BridgeError::new(
                    "changed_since_read",
                    format!(
                        "{shown} changed since that SHA-256 was taken; read it again before writing"
                    ),
                ));
            }
        }
        // All guards precede directory creation. A failed CAS creates nothing.
        if input.create_dirs && !exists {
            self.make_dirs(
                target
                    .parent()
                    .and_then(Path::to_str)
                    .ok_or_else(|| invalid("path must name a file"))?,
            )?;
        }
        let entry = checked::Entry::at(&target).map_err(|e| io_error(&shown, e))?;
        self.check()?;
        // Past the commit point (the rename, the link or the append) nothing may fail the
        // call: the receipt comes from what is already known, never from another read under
        // the caller's authority, so a change that happened is never reported as not made.
        let (digest, size) = match input.mode {
            WriteMode::Overwrite | WriteMode::Create => {
                replace(self, &entry, &bytes, input.mode == WriteMode::Create)?;
                (checksum::bytes(&bytes), bytes.len() as u64)
            }
            WriteMode::Append => {
                let mut file = entry
                    .parent
                    .file(
                        &entry.name,
                        OpenOptions::new().read(true).append(true).create(true),
                    )
                    .map_err(|e| io_error(&shown, e))?;
                // The content so far, hashed before the commit point: a refusal here changes
                // nothing. Another process appending in between shows up as
                // `changed_since_read` on the next guarded write, which is the file's truth.
                let (mut digest, before) = checksum::hasher(self, &mut file, &shown)?;
                self.check()?;
                file.write_all(&bytes).map_err(|e| io_error(&shown, e))?;
                file.sync_all().map_err(|e| io_error(&shown, e))?;
                sha2::Digest::update(&mut digest, &bytes);
                (checksum::finish(digest), before + bytes.len() as u64)
            }
        };
        Ok(FileWritten {
            path: shown,
            size,
            sha256: digest,
            created: !exists,
            replacements: None,
        })
    }

    pub(crate) fn edit(&self, input: FileEditInput) -> BridgeResult<FileWritten> {
        if input.old_text.is_empty() {
            return Err(invalid("old_text must not be empty"));
        }
        if input.old_text.len() + input.new_text.len() > MAX_FILE_WRITE_BYTES {
            return Err(invalid(format!(
                "old_text and new_text together must be at most {MAX_FILE_WRITE_BYTES} bytes"
            )));
        }
        let (target, exists) = self.target(&input.path, false)?;
        let shown = display(&target)?;
        if !exists {
            return Err(BridgeError::new(
                "path_not_found",
                format!("{shown} does not exist"),
            ));
        }
        let entry = checked::Entry::at(&target).map_err(|e| io_error(&shown, e))?;
        let mut file = entry.read().map_err(|e| io_error(&shown, e))?;
        let size = file.metadata().map_err(|e| io_error(&shown, e))?.len();
        if size > MAX_EDIT_BYTES {
            return Err(invalid(format!(
                "{shown} is larger than {MAX_EDIT_BYTES} bytes; edit it with run"
            )));
        }
        let mut current = Vec::new();
        checked::Reader::new(&mut file, self)
            .take(MAX_EDIT_BYTES + 1)
            .read_to_end(&mut current)
            .map_err(|e| io_error(&shown, e))?;
        if current.len() as u64 > MAX_EDIT_BYTES {
            return Err(invalid("file grew beyond the edit bound"));
        }
        self.check()?;
        let text = String::from_utf8(current)
            .map_err(|_| invalid(format!("{shown} is not UTF-8 text; use write_file")))?;
        let mut found = text.matches(&input.old_text).count() as u32;
        let (mut old_text, mut new_text) = (input.old_text, input.new_text);
        if found != input.expected_replacements
            && let Some((old, new)) = in_line_endings_of(&text, &old_text, &new_text)
        {
            // The text was sent with the other line ending (an agent writes \n; the file
            // has \r\n, or the reverse). Match it as the file has it and keep the file's.
            let again = text.matches(&old).count() as u32;
            if again == input.expected_replacements {
                (found, old_text, new_text) = (again, old, new);
            }
        }
        if found != input.expected_replacements {
            return Err(BridgeError::new(
                "edit_mismatch",
                format!(
                    "old_text occurs {found} times in {shown}, not {}; nothing was changed. Include more surrounding text to make it unique, or set expected_replacements.",
                    input.expected_replacements
                ),
            ));
        }
        let changed = text.replace(&old_text, &new_text);
        replace(self, &entry, changed.as_bytes(), false)?;
        Ok(FileWritten {
            path: shown,
            size: changed.len() as u64,
            sha256: checksum::bytes(changed.as_bytes()),
            created: false,
            replacements: Some(found),
        })
    }
}

/// `old` and `new` rewritten in the line ending `text` uses, when they use the other one:
/// `None` when the file mixes both, or the edit already matches its ending.
fn in_line_endings_of(text: &str, old: &str, new: &str) -> Option<(String, String)> {
    let crlf = text.matches("\r\n").count();
    let lf = text.matches('\n').count() - crlf;
    let lf_only = |s: &str| s.replace("\r\n", "\n");
    match (crlf, lf) {
        (1.., 0) if old.contains('\n') && lf_only(old).len() == old.len() => {
            let to_crlf = |s: &str| lf_only(s).replace('\n', "\r\n");
            Some((to_crlf(old), to_crlf(new)))
        }
        (0, 1..) if old.contains("\r\n") => Some((lf_only(old), lf_only(new))),
        _ => None,
    }
}

fn outside() -> BridgeError {
    BridgeError::new(
        "path_outside_workspace",
        "This machine's broker reaches files only inside its workspace",
    )
}

#[cfg(test)]
mod tests;
