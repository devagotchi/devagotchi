//! Whole-file checksums from one held regular file, with bounded memory and time.
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::time::{Duration, Instant};

use devagotchi_contracts::{BridgeError, BridgeResult};
use sha2::{Digest, Sha256};

use super::{Scope, io_error};

pub(super) const IO_BUDGET: Duration =
    devagotchi_ipc::IO_TIMEOUT.saturating_sub(Duration::from_secs(2));

pub(super) fn bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub(super) fn file(scope: &Scope<'_>, file: &mut File, shown: &str) -> BridgeResult<String> {
    let (digest, _) = hasher(scope, file, shown)?;
    Ok(hex::encode(digest.finalize()))
}

/// The running digest of a held regular file's whole content and its length, for a caller
/// that goes on to extend it (an append's receipt adds the appended bytes after its commit
/// point, without reading the file again).
pub(super) fn hasher(
    scope: &Scope<'_>,
    file: &mut File,
    shown: &str,
) -> BridgeResult<(Sha256, u64)> {
    let before = file.metadata().map_err(|e| io_error(shown, e))?;
    if !before.is_file() {
        return Err(BridgeError::new(
            "not_a_file",
            "a checksum requires a regular file",
        ));
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|e| io_error(shown, e))?;
    let started = Instant::now();
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut count = 0u64;
    loop {
        scope.check()?;
        if started.elapsed() >= IO_BUDGET {
            return Err(BridgeError::new(
                "file_timeout",
                "whole-file checksum exceeded its IO deadline",
            ));
        }
        let read = file.read(&mut buffer).map_err(|e| io_error(shown, e))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
        count += read as u64;
        if count > before.len() {
            return Err(changed());
        }
    }
    let after = file.metadata().map_err(|e| io_error(shown, e))?;
    if count != before.len()
        || before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
    {
        return Err(changed());
    }
    scope.check()?;
    Ok((digest, count))
}

/// A finished digest as the receipts carry it.
pub(super) fn finish(digest: Sha256) -> String {
    hex::encode(digest.finalize())
}

pub(super) fn changed() -> BridgeError {
    BridgeError::new(
        "changed_since_read",
        "file changed since its checksum was taken; obtain a new whole-file checksum",
    )
}
