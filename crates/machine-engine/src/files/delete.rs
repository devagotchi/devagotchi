//! Guarded removal of a single regular file. Never recursive and never follows links.
use devagotchi_contracts::{BridgeError, BridgeResult, FileDeleteInput, FileDeleted, whole_sha};

use super::{Scope, checked, checksum, display, invalid, io_error};

impl Scope<'_> {
    pub(crate) fn delete(&self, input: FileDeleteInput) -> BridgeResult<FileDeleted> {
        if !whole_sha(&input.expected_sha256) {
            return Err(invalid("expected_sha256 must be a whole-file SHA-256"));
        }
        let path = self.place(&input.path)?;
        let shown = display(&path)?;
        let entry = checked::Entry::at(&path).map_err(|e| io_error(&shown, e))?;
        let metadata = entry
            .parent
            .handle
            .symlink_metadata(&entry.name)
            .map_err(|e| io_error(&shown, e))?;
        if !metadata.is_file() {
            return Err(BridgeError::new(
                "not_a_file",
                "delete_file only removes regular files; directories and links are refused",
            ));
        }
        let mut file = entry.read().map_err(|e| io_error(&shown, e))?;
        let sha256 = checksum::file(self, &mut file, &shown)?;
        if !sha256.eq_ignore_ascii_case(&input.expected_sha256) {
            return Err(checksum::changed());
        }
        self.check()?;
        entry.parent.verify().map_err(|e| io_error(&shown, e))?;
        entry
            .parent
            .handle
            .remove_file(&entry.name)
            .map_err(|e| io_error(&shown, e))?;
        Ok(FileDeleted {
            path: shown,
            sha256,
        })
    }
}
