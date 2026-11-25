//! Bounded chunks and authority checks around each read/seek on a held file.
use super::super::{Scope, checksum};
use devagotchi_contracts::BridgeError;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::time::Instant;

pub(in crate::files) struct Reader<'a, 's> {
    file: &'a mut File,
    scope: &'a Scope<'s>,
    deadline: Instant,
}
impl<'a, 's> Reader<'a, 's> {
    pub(in crate::files) fn new(file: &'a mut File, scope: &'a Scope<'s>) -> Self {
        Self {
            file,
            scope,
            deadline: Instant::now() + checksum::IO_BUDGET,
        }
    }
    fn check(&self) -> io::Result<()> {
        self.scope.check().map_err(io::Error::other)?;
        if Instant::now() >= self.deadline {
            return Err(io::Error::other(BridgeError::new(
                "file_timeout",
                "file scan exceeded its IO deadline",
            )));
        }
        Ok(())
    }
}
impl Read for Reader<'_, '_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.check()?;
        let length = buffer.len().min(64 * 1024);
        self.file.read(&mut buffer[..length])
    }
}
impl Seek for Reader<'_, '_> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.check()?;
        self.file.seek(position)
    }
}
