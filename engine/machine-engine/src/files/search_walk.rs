//! Search traversal through held directories. Ignore text is read only within the root;
//! no global/parent ignore configuration or linked ignore file is opened ambiently.
use super::{Scope, checked::Directory, checksum, io_error};
use cap_std::fs::{Metadata, OpenOptions};
use devagotchi_contracts::{BridgeError, BridgeResult};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

pub(super) struct Entry {
    pub path: PathBuf,
    pub name: OsString,
    pub metadata: Metadata,
    parent: Arc<Directory>,
}
impl Entry {
    pub fn text(&self, scope: &Scope<'_>, limit: u64) -> BridgeResult<Option<String>> {
        scope.check()?;
        if !self.metadata.is_file() || self.metadata.len() > limit {
            return Ok(None);
        }
        let mut file = match self.parent.file(&self.name, OpenOptions::new().read(true)) {
            Ok(file) => file,
            Err(_) => return Ok(None),
        };
        let mut bytes = Vec::new();
        super::checked::Reader::new(&mut file, scope)
            .take(limit + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| io_error("search file", e))?;
        scope.check()?;
        self.parent
            .verify()
            .map_err(|e| io_error("search file", e))?;
        if bytes.len() as u64 > limit || bytes[..bytes.len().min(8192)].contains(&0) {
            return Ok(None);
        }
        Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
    }
}

struct Frame {
    directory: Arc<Directory>,
    entries: std::vec::IntoIter<OsString>,
    ignores: Vec<Arc<Gitignore>>,
}
pub(super) struct Walk<'a, 's> {
    scope: &'a Scope<'s>,
    root: PathBuf,
    include_ignored: bool,
    after: Option<PathBuf>,
    stack: Vec<Frame>,
    pending: Option<(Arc<Directory>, OsString, Vec<Arc<Gitignore>>)>,
    deadline: Instant,
}
impl<'a, 's> Walk<'a, 's> {
    pub fn new(
        scope: &'a Scope<'s>,
        root: &Path,
        include_ignored: bool,
        after: Option<PathBuf>,
    ) -> BridgeResult<Self> {
        let directory = Arc::new(Directory::open(root).map_err(|e| io_error("search root", e))?);
        let mut walk = Self {
            scope,
            root: root.to_path_buf(),
            include_ignored,
            after,
            stack: Vec::new(),
            pending: None,
            deadline: Instant::now() + checksum::IO_BUDGET,
        };
        walk.push(directory, Vec::new())?;
        Ok(walk)
    }

    fn check(&self) -> BridgeResult<()> {
        self.scope.check()?;
        if Instant::now() >= self.deadline {
            return Err(BridgeError::new(
                "file_timeout",
                "search traversal exceeded its IO deadline",
            ));
        }
        Ok(())
    }

    fn push(
        &mut self,
        directory: Arc<Directory>,
        mut ignores: Vec<Arc<Gitignore>>,
    ) -> BridgeResult<()> {
        self.check()?;
        directory
            .verify()
            .map_err(|e| io_error("search directory", e))?;
        if !self.include_ignored {
            // Later/deeper patterns take precedence. add_line parses already checked text.
            for name in [".gitignore", ".ignore"] {
                self.check()?;
                let metadata = match directory.handle.symlink_metadata(name) {
                    Ok(metadata) => metadata,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(error) => return Err(io_error("search ignore file", error)),
                };
                if !metadata.is_file() {
                    continue;
                }
                if metadata.len() > 256 * 1024 {
                    return Err(BridgeError::new(
                        "too_large",
                        "search ignore file exceeds 256 KiB",
                    ));
                }
                let mut file = directory
                    .file(name.as_ref(), OpenOptions::new().read(true))
                    .map_err(|e| io_error("search ignore file", e))?;
                let mut bytes = Vec::new();
                super::checked::Reader::new(&mut file, self.scope)
                    .take(256 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| io_error("search ignore file", e))?;
                if bytes.len() > 256 * 1024 {
                    return Err(BridgeError::new(
                        "too_large",
                        "search ignore file grew beyond 256 KiB",
                    ));
                }
                let mut builder = GitignoreBuilder::new(&directory.path);
                for line in String::from_utf8_lossy(&bytes).lines() {
                    builder
                        .add_line(Some(directory.path.join(name)), line)
                        .map_err(|e| BridgeError::new("invalid_ignore", e.to_string()))?;
                }
                ignores.push(Arc::new(
                    builder
                        .build()
                        .map_err(|e| BridgeError::new("invalid_ignore", e.to_string()))?,
                ));
            }
        }
        let mut names = Vec::new();
        for entry in directory
            .handle
            .entries()
            .map_err(|e| io_error("search directory", e))?
        {
            self.check()?;
            names.push(
                entry
                    .map_err(|e| io_error("search directory", e))?
                    .file_name(),
            );
        }
        names.sort();
        self.stack.push(Frame {
            directory,
            entries: names.into_iter(),
            ignores,
        });
        Ok(())
    }

    pub fn next(&mut self) -> BridgeResult<Option<Entry>> {
        loop {
            self.check()?;
            if let Some((parent, name, ignores)) = self.pending.take() {
                let child = parent
                    .child(&name)
                    .map_err(|e| io_error("search subdirectory", e))?;
                self.push(Arc::new(child), ignores)?;
            }
            let Some(frame) = self.stack.last_mut() else {
                return Ok(None);
            };
            let Some(name) = frame.entries.next() else {
                self.stack.pop();
                continue;
            };
            frame
                .directory
                .verify()
                .map_err(|e| io_error("search directory", e))?;
            if !self.include_ignored && name.to_string_lossy().starts_with('.') {
                continue;
            }
            let path = frame.directory.path.join(&name);
            let metadata = match frame.directory.handle.symlink_metadata(&name) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(io_error("search entry", error)),
            };
            if !self.include_ignored
                && frame
                    .ignores
                    .iter()
                    .rev()
                    .map(|rules| rules.matched(&path, metadata.is_dir()))
                    .find(|matched| !matched.is_none())
                    .is_some_and(|matched| matched.is_ignore())
            {
                continue;
            }
            let relative = path
                .strip_prefix(&self.root)
                .map_err(|e| BridgeError::new("internal_error", e.to_string()))?;
            let passed = self
                .after
                .as_ref()
                .is_some_and(|after| relative < after.as_path() && !after.starts_with(relative));
            if metadata.is_dir() && !passed {
                self.pending = Some((
                    Arc::clone(&frame.directory),
                    name.clone(),
                    frame.ignores.clone(),
                ));
            }
            if passed {
                continue;
            }
            return Ok(Some(Entry {
                path,
                name,
                metadata,
                parent: Arc::clone(&frame.directory),
            }));
        }
    }
}
