//! Which file object an open file is, for a read's `identity`: device and inode on Unix,
//! volume serial and 128-bit file ID on Windows. It stays through a rename and changes when
//! the path is replaced by another file. `None` when the system will not say.

use std::fs::{File, Metadata};

#[cfg(unix)]
pub(crate) fn file_identity(_file: &File, metadata: &Metadata) -> Option<String> {
    use std::os::unix::fs::MetadataExt;
    Some(format!("{:x}:{:x}", metadata.dev(), metadata.ino()))
}

#[cfg(windows)]
#[allow(unsafe_code)]
pub(crate) fn file_identity(file: &File, _metadata: &Metadata) -> Option<String> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ID_INFO, FileIdInfo, GetFileInformationByHandleEx,
    };
    let mut information = FILE_ID_INFO::default();
    // SAFETY: the handle is the open `file`'s, alive for this call; the buffer is a
    // FILE_ID_INFO of the size given, which FileIdInfo fills.
    let filled = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileIdInfo,
            (&mut information as *mut FILE_ID_INFO).cast(),
            size_of::<FILE_ID_INFO>() as u32,
        )
    };
    if filled == 0 {
        return None;
    }
    Some(format!(
        "{:016x}:{}",
        information.VolumeSerialNumber,
        hex::encode(information.FileId.Identifier)
    ))
}
