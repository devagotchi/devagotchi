//! Metadata and optional whole-file checksum from checked handles.
use super::*;
use crate::files::text_prefix;
use devagotchi_contracts::FileInfoInput;
use std::io::Read;
use std::time::UNIX_EPOCH;

fn created_ms(metadata: &Metadata) -> Option<u64> {
    let since = metadata
        .created()
        .ok()?
        .into_std()
        .duration_since(UNIX_EPOCH)
        .ok()?;
    Some(since.as_millis().min(u64::MAX as u128) as u64)
}

fn kind(metadata: &Metadata) -> &'static str {
    match metadata.file_type() {
        t if t.is_symlink() => "symlink",
        t if t.is_dir() => "dir",
        t if t.is_file() => "file",
        _ => "other",
    }
}

#[cfg(unix)]
fn mode(metadata: &Metadata) -> Option<String> {
    use cap_std::fs::PermissionsExt;
    Some(format!("{:04o}", metadata.permissions().mode() & 0o7777))
}

#[cfg(not(unix))]
fn mode(_: &Metadata) -> Option<String> {
    None
}

/// Windows keeps a read-only attribute on folders that it ignores for writing into them.
fn readonly(metadata: &Metadata) -> bool {
    metadata.permissions().readonly() && !(cfg!(windows) && metadata.is_dir())
}

/// Lines in a text file small enough to count; `None` for anything else.
fn modified_ms(metadata: &Metadata) -> Option<u64> {
    let since = metadata
        .modified()
        .ok()?
        .into_std()
        .duration_since(UNIX_EPOCH)
        .ok()?;
    Some(since.as_millis().min(u64::MAX as u128) as u64)
}

fn lines(
    scope: &Scope<'_>,
    entry: &checked::Entry,
    metadata: &Metadata,
) -> BridgeResult<Option<u64>> {
    if !metadata.is_file() || metadata.len() > crate::files::lines::COUNTED_BYTES {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    let mut file = entry.read().map_err(|e| io_error("file line count", e))?;
    checked::Reader::new(&mut file, scope)
        .take(crate::files::lines::COUNTED_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| io_error("file line count", e))?;
    let after = file
        .metadata()
        .map_err(|e| io_error("file line count", e))?;
    if bytes.len() as u64 != metadata.len()
        || after.modified().ok() != metadata.modified().ok().map(|v| v.into_std())
    {
        return Err(checksum::changed());
    }
    Ok((text_prefix(&bytes) == Some(bytes.len())).then(|| crate::files::lines::count(&bytes)))
}

impl Scope<'_> {
    pub(super) fn describe(&self, path: &Path) -> BridgeResult<FileInfo> {
        let shown = display(path)?;
        self.check()?;
        let entry = if path.file_name().is_some() {
            Some(checked::Entry::at(path).map_err(|e| io_error(&shown, e))?)
        } else {
            None
        };
        let metadata = match &entry {
            Some(entry) => entry.parent.handle.symlink_metadata(&entry.name),
            None => checked::Directory::open(path).and_then(|dir| dir.handle.dir_metadata()),
        }
        .map_err(|e| io_error(&shown, e))?;
        let link_target = if metadata.is_symlink() {
            self.checked_link(path)?;
            let entry = entry
                .as_ref()
                .ok_or_else(|| invalid("a root cannot be a link"))?;
            Some(
                entry
                    .parent
                    .handle
                    .read_link_contents(&entry.name)
                    .map_err(|e| io_error(&shown, e))?,
            )
        } else {
            None
        };
        let lines = match &entry {
            Some(entry) => lines(self, entry, &metadata)?,
            None => None,
        };
        self.check()?;
        if let Some(entry) = &entry {
            entry.parent.verify().map_err(|e| io_error(&shown, e))?;
        }
        Ok(FileInfo {
            sha256: None,
            kind: kind(&metadata).to_owned(),
            size: metadata.len(),
            modified_ms: modified_ms(&metadata),
            created_ms: created_ms(&metadata),
            readonly: readonly(&metadata),
            mode: mode(&metadata),
            link_target: link_target
                .map(|target| dunce::simplified(&target).to_string_lossy().into_owned()),
            lines,
            made: None,
            replaced: None,
            path: shown,
        })
    }

    pub(crate) fn info(&self, input: FileInfoInput) -> BridgeResult<FileInfo> {
        let path = self.place(&input.path)?;
        let mut info = self.describe(&path)?;
        if input.sha256 {
            if info.kind != "file" {
                return Err(BridgeError::new(
                    "not_a_file",
                    "a checksum requires a regular file, not a link or directory",
                ));
            }
            let entry = checked::Entry::at(&path).map_err(|e| io_error(&input.path, e))?;
            let mut file = entry.read().map_err(|e| io_error(&input.path, e))?;
            info.sha256 = Some(checksum::file(self, &mut file, &input.path)?);
            entry
                .parent
                .verify()
                .map_err(|e| io_error(&input.path, e))?;
        }
        self.check()?;
        Ok(info)
    }
}
