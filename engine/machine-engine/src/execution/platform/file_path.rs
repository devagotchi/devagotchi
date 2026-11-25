//! Native path of a held file handle. A checked directory must name the directory
//! that was resolved, rather than a replacement reached while opening its ancestors.
use std::fs::File;
use std::io;
use std::path::PathBuf;

#[cfg(target_os = "linux")]
pub(crate) fn opened_path(file: &File) -> io::Result<PathBuf> {
    use std::os::fd::AsRawFd;
    std::fs::read_link(format!("/proc/self/fd/{}", file.as_raw_fd()))
}

#[cfg(windows)]
#[allow(unsafe_code)]
pub(crate) fn opened_path(file: &File) -> io::Result<PathBuf> {
    use std::ffi::OsString;
    use std::os::windows::{ffi::OsStringExt, io::AsRawHandle};
    use windows_sys::Win32::Storage::FileSystem::GetFinalPathNameByHandleW;
    let mut buffer = vec![0u16; 512];
    loop {
        // SAFETY: file owns a live handle; buffer has the writable length supplied.
        let length = unsafe {
            GetFinalPathNameByHandleW(
                file.as_raw_handle(),
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                0,
            )
        };
        if length == 0 {
            return Err(io::Error::last_os_error());
        }
        if (length as usize) < buffer.len() {
            let path = PathBuf::from(OsString::from_wide(&buffer[..length as usize]));
            return Ok(dunce::simplified(&path).to_path_buf());
        }
        if length > 32768 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "handle path exceeds platform bound",
            ));
        }
        buffer.resize(length as usize + 1, 0);
    }
}
