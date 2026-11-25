//! Handle-relative filesystem operations after policy resolution.
//! Owns: held directory capabilities, never persistent state.
//! Serves: the common broker filesystem owner.
//! Settings: none; paths and authority come from the caller's checked scope.
//! Depends on: cap-std and the native held-handle path provider.
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt};
use cap_std::fs::{Dir, OpenOptions};

use crate::execution::platform::file_path::opened_path;
mod reader;
pub(super) use reader::Reader;
#[cfg(test)]
mod tests;

pub(super) struct Directory {
    pub(super) handle: Dir,
    pub(super) path: PathBuf,
}

impl Directory {
    pub(super) fn open(canonical: &Path) -> io::Result<Self> {
        let handle = Dir::open_ambient_dir(canonical, cap_std::ambient_authority())?;
        let directory = Self {
            handle,
            path: canonical.to_path_buf(),
        };
        directory.verify()?;
        Ok(directory)
    }

    pub(super) fn verify(&self) -> io::Result<()> {
        let file = self.handle.try_clone()?.into_std_file();
        if opened_path(&file)? != self.path {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "directory changed while opening",
            ));
        }
        Ok(())
    }

    pub(super) fn child(&self, name: &OsStr) -> io::Result<Self> {
        self.verify()?;
        let handle = self.handle.open_dir_nofollow(name)?;
        let child = Self {
            handle,
            path: self.path.join(name),
        };
        child.verify()?;
        Ok(child)
    }

    pub(super) fn file(&self, name: &OsStr, options: &mut OpenOptions) -> io::Result<File> {
        self.verify()?;
        options.follow(FollowSymlinks::No);
        #[cfg(target_os = "linux")]
        {
            use cap_std::fs::OpenOptionsExt;
            // A replaced regular file must not block the broker by becoming a FIFO.
            options.custom_flags(libc::O_NONBLOCK);
        }
        let file = self.handle.open_with(name, options)?.into_std();
        if !file.metadata()?.is_file() || opened_path(&file)? != self.path.join(name) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "file changed while opening or is not regular",
            ));
        }
        Ok(file)
    }
}

pub(super) struct Entry {
    pub(super) parent: Directory,
    pub(super) name: OsString,
}

impl Entry {
    pub(super) fn at(canonical: &Path) -> io::Result<Self> {
        let parent = canonical.parent().ok_or(io::ErrorKind::InvalidInput)?;
        let name = canonical
            .file_name()
            .ok_or(io::ErrorKind::InvalidInput)?
            .to_owned();
        Ok(Self {
            parent: Directory::open(parent)?,
            name,
        })
    }

    pub(super) fn read(&self) -> io::Result<File> {
        self.parent.file(&self.name, OpenOptions::new().read(true))
    }
}
