//! Creating a process by hand (`CreateProcessW`), for what `std::process::Command` cannot do
//! on stable Rust (a pseudo-console, an exact handle list): its command line, environment
//! block and attribute list; and the root that makes, which the broker observes through its
//! process handle as tokio observes a child it spawned.

use super::*;
use std::ffi::OsString;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::ExitStatusExt;
use std::process::ExitStatus;
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::System::Threading::{
    DeleteProcThreadAttributeList, GetExitCodeProcess, InitializeProcThreadAttributeList,
    LPPROC_THREAD_ATTRIBUTE_LIST, RegisterWaitForSingleObject, TerminateProcess, UnregisterWaitEx,
    UpdateProcThreadAttribute, WT_EXECUTEONLYONCE,
};

fn wide(text: &OsStr) -> Vec<u16> {
    text.encode_wide().collect()
}

/// `text` as UTF-16 with its terminating NUL.
pub(super) fn terminated(text: &OsStr) -> Vec<u16> {
    text.encode_wide().chain(std::iter::once(0)).collect()
}

/// The program, quoted (a Windows path cannot hold a quote), then each argument as the
/// Microsoft C runtime reads it back; NUL-terminated and mutable, as `CreateProcessW` wants.
pub(super) fn command_line<A: AsRef<OsStr>>(
    program: &OsStr,
    args: impl IntoIterator<Item = A>,
) -> Vec<u16> {
    let mut line = vec![b'"' as u16];
    line.extend(wide(program));
    line.push(b'"' as u16);
    for argument in args {
        line.push(b' ' as u16);
        crate::execution::detached::append_windows_argument(&mut line, &wide(argument.as_ref()));
    }
    line.push(0);
    line
}

/// A Unicode environment block of exactly `variables`, sorted by name as Windows keeps it.
pub(super) fn environment_block(
    variables: impl IntoIterator<Item = (OsString, OsString)>,
) -> Vec<u16> {
    let mut variables: Vec<(Vec<u16>, Vec<u16>)> = variables
        .into_iter()
        .map(|(name, value)| (wide(&name), wide(&value)))
        .collect();
    variables.sort_by_key(|(name, _)| String::from_utf16_lossy(name).to_uppercase());
    let mut block = Vec::new();
    for (name, value) in variables {
        block.extend(name);
        block.push(b'=' as u16);
        block.extend(value);
        block.push(0);
    }
    if block.is_empty() {
        block.push(0);
    }
    block.push(0);
    block
}

/// A process-and-thread attribute list, deleted on drop. Its storage is on the heap, so the
/// list stays where it was initialized however this value moves.
pub(super) struct AttributeList {
    storage: Vec<usize>,
}

impl AttributeList {
    pub(super) fn new(count: u32) -> io::Result<Self> {
        let mut size = 0usize;
        // SAFETY: a size query; the call fails with ERROR_INSUFFICIENT_BUFFER by design.
        unsafe { InitializeProcThreadAttributeList(null_mut(), count, 0, &mut size) };
        let mut storage = vec![0usize; size.div_ceil(size_of::<usize>()).max(1)];
        let list = storage.as_mut_ptr() as LPPROC_THREAD_ATTRIBUTE_LIST;
        // SAFETY: `storage` is at least `size` bytes, pointer-aligned, and owned by the list.
        if unsafe { InitializeProcThreadAttributeList(list, count, 0, &mut size) } == FALSE {
            return Err(io::Error::last_os_error());
        }
        Ok(Self { storage })
    }

    pub(super) fn as_ptr(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.storage.as_mut_ptr() as LPPROC_THREAD_ATTRIBUTE_LIST
    }

    /// Set `attribute` to the `size` bytes at `value`.
    ///
    /// # Safety
    ///
    /// `value` must point to `size` readable bytes that outlive every `CreateProcessW` call
    /// given this list (for a value the attribute takes as itself, such as a pseudo-console,
    /// `value` is that value and nothing is read through it).
    pub(super) unsafe fn set(
        &mut self,
        attribute: u32,
        value: *const core::ffi::c_void,
        size: usize,
    ) -> io::Result<()> {
        // SAFETY: the list is initialized; the caller vouches for `value` and `size`.
        let set = unsafe {
            UpdateProcThreadAttribute(
                self.as_ptr(),
                0,
                attribute as usize,
                value,
                size,
                null_mut(),
                null(),
            )
        };
        if set == FALSE {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

impl Drop for AttributeList {
    fn drop(&mut self) {
        // SAFETY: initialized in `new`, deleted once, here.
        unsafe { DeleteProcThreadAttributeList(self.as_ptr()) };
    }
}

/// The root of an operation, as the broker holds it: a child tokio spawned (its stdio
/// piped), or a process created on a pseudo-console.
pub(crate) enum Root {
    Piped(Child),
    Console(ConsoleRoot),
}

/// The root's piped stdio, where it has any (a console root's stdio is its console).
pub(crate) fn piped(root: &mut Root) -> Option<&mut Child> {
    match root {
        Root::Piped(child) => Some(child),
        Root::Console(_) => None,
    }
}

impl Root {
    pub(crate) fn id(&self) -> Option<u32> {
        match self {
            Self::Piped(child) => child.id(),
            Self::Console(root) => Some(root.pid),
        }
    }

    pub(crate) fn raw_handle(&self) -> Option<RawHandle> {
        match self {
            Self::Piped(child) => child.raw_handle(),
            Self::Console(root) => Some(root.process.as_raw_handle()),
        }
    }

    pub(crate) fn start_kill(&mut self) -> io::Result<()> {
        match self {
            Self::Piped(child) => child.start_kill(),
            Self::Console(root) => root.kill(),
        }
    }

    pub(crate) async fn wait(&mut self) -> io::Result<ExitStatus> {
        match self {
            Self::Piped(child) => child.wait().await,
            Self::Console(root) => root.wait().await,
        }
    }
}

/// A root created on a pseudo-console. Its exit is a wait the system thread pool holds on
/// its process handle, reported through `exited`; nothing polls.
pub(crate) struct ConsoleRoot {
    // Declared first, so it unregisters before the handle it waits on closes.
    _exit: ExitWait,
    exited: watch::Receiver<bool>,
    process: OwnedHandle,
    pid: u32,
}

impl ConsoleRoot {
    /// Take ownership of a process just created (still suspended). When its exit cannot
    /// be waited on, it is terminated before it ever runs.
    pub(super) fn new(process: OwnedHandle, pid: u32) -> io::Result<Self> {
        match ExitWait::register(&process) {
            Ok((exit, exited)) => Ok(Self {
                _exit: exit,
                exited,
                process,
                pid,
            }),
            Err(error) => {
                // SAFETY: `process` is the open process handle this function owns.
                unsafe { TerminateProcess(process.as_raw_handle(), TERMINATED_EXIT_CODE) };
                Err(error)
            }
        }
    }

    fn kill(&self) -> io::Result<()> {
        // SAFETY: the process handle is open for the life of `self`.
        if unsafe { TerminateProcess(self.process.as_raw_handle(), TERMINATED_EXIT_CODE) } == FALSE
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    async fn wait(&self) -> io::Result<ExitStatus> {
        let mut exited = self.exited.clone();
        exited
            .wait_for(|done| *done)
            .await
            .map_err(|_| io::Error::other("the root's exit wait ended unreported"))?;
        let mut code = 0u32;
        // SAFETY: the process handle is open for the life of `self`; `code` is a valid out
        // pointer.
        if unsafe { GetExitCodeProcess(self.process.as_raw_handle(), &mut code) } == FALSE {
            return Err(io::Error::last_os_error());
        }
        Ok(ExitStatus::from_raw(code))
    }
}

/// A one-shot thread-pool wait on a process handle. The wait handle and the sender it
/// reports through are kept as addresses: plain integers, so the root stays `Send`.
struct ExitWait {
    wait: usize,
    sender: usize,
}

/// Runs on a thread-pool thread once the process has exited.
unsafe extern "system" fn exited(context: *mut core::ffi::c_void, _timed_out: bool) {
    // SAFETY: `context` is the sender `ExitWait` owns; it is freed only after
    // `UnregisterWaitEx` has returned, which waits for this callback to finish.
    let sender = unsafe { &*(context as *const watch::Sender<bool>) };
    sender.send_replace(true);
}

impl ExitWait {
    fn register(process: &OwnedHandle) -> io::Result<(Self, watch::Receiver<bool>)> {
        let (sender, receiver) = watch::channel(false);
        let sender = Box::into_raw(Box::new(sender));
        let mut wait: HANDLE = null_mut();
        // SAFETY: `wait` is a valid out pointer; the process handle outlives the wait (the
        // root unregisters it before closing the handle); `sender` stays allocated until the
        // wait is unregistered.
        let registered = unsafe {
            RegisterWaitForSingleObject(
                &mut wait,
                process.as_raw_handle(),
                Some(exited),
                sender.cast(),
                INFINITE,
                WT_EXECUTEONLYONCE,
            )
        };
        if registered == FALSE {
            let error = io::Error::last_os_error();
            // SAFETY: nothing was registered, so this is the only owner of `sender`.
            drop(unsafe { Box::from_raw(sender) });
            return Err(error);
        }
        Ok((
            Self {
                wait: wait as usize,
                sender: sender as usize,
            },
            receiver,
        ))
    }
}

impl Drop for ExitWait {
    fn drop(&mut self) {
        // SAFETY: `wait` was registered in `register` and is unregistered once, here;
        // INVALID_HANDLE_VALUE makes this wait for a running callback to finish, after which
        // nothing reads `sender` and it is freed.
        unsafe {
            UnregisterWaitEx(self.wait as HANDLE, INVALID_HANDLE_VALUE);
            drop(Box::from_raw(self.sender as *mut watch::Sender<bool>));
        }
    }
}
