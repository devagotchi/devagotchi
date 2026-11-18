//! Resuming a root created with `CREATE_SUSPENDED`, once it is in its Job Object.
//!
//! Only the root's own threads are captured, with the documented process-snapshot (PSS)
//! APIs: `PssCaptureSnapshot(process, PSS_CAPTURE_THREADS)`, then a walk of that snapshot.
//! A system-wide ToolHelp thread walk made startup scale with every thread on the machine;
//! under default-parallel tests it took 6.2 seconds against a 5-second action deadline, so
//! actions timed out before they ran. PSS is supported since Windows 8.1 / Server 2012 R2.
//!
//! Every captured thread is opened (non-inheritable) and checked before any is resumed:
//! its process ID must be the root's and its creation time the one the snapshot recorded,
//! so a reused thread ID cannot select another thread. `ResumeThread` must report the
//! initial suspend count of one, so a suspension someone else holds is never consumed to
//! force the root to run. Any failure fails closed: `adopt` terminates the Job Object.
//!
//! PSS snapshots and walk markers are not kernel handles: they are freed with their own
//! calls, never `CloseHandle` or `OwnedHandle`.
//!
//! What this does not guarantee. It makes startup reliable; it is not OS sandboxing. The
//! native calls' latency is not bounded, and no final deadline or cancellation check runs
//! just before the resume, so there is no hard "no effect after the deadline" guarantee.
//! If a root has several threads and a later `ResumeThread` fails, the threads already
//! resumed may have produced effects before the job is terminated.

use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{ERROR_NO_MORE_ITEMS, ERROR_SUCCESS, FALSE, FILETIME, HANDLE};
use windows_sys::Win32::System::Diagnostics::ProcessSnapshotting::{
    HPSS, HPSSWALK, PSS_CAPTURE_THREADS, PSS_THREAD_ENTRY, PSS_THREAD_FLAGS_TERMINATED,
    PSS_WALK_THREADS, PssCaptureSnapshot, PssFreeSnapshot, PssWalkMarkerCreate, PssWalkMarkerFree,
    PssWalkSnapshot,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetProcessIdOfThread, GetThreadTimes, OpenThread, ResumeThread,
    THREAD_QUERY_LIMITED_INFORMATION, THREAD_SUSPEND_RESUME,
};

/// A thread snapshot. It belongs to this process, not the captured root.
struct ThreadSnapshot(HPSS);

impl Drop for ThreadSnapshot {
    fn drop(&mut self) {
        // SAFETY: captured here and exclusively owned; freed exactly once.
        unsafe { PssFreeSnapshot(GetCurrentProcess(), self.0) };
    }
}

/// A walk marker over a [`ThreadSnapshot`].
struct ThreadWalk(HPSSWALK);

impl Drop for ThreadWalk {
    fn drop(&mut self) {
        // SAFETY: created here and exclusively owned; freed exactly once.
        unsafe { PssWalkMarkerFree(self.0) };
    }
}

/// The suspended root's threads, each opened and identity-checked, none resumed. Fails
/// on an entry of another process or a terminated one, on a thread whose process or
/// creation time no longer matches the snapshot, and when there is no thread at all.
pub(super) fn suspended_threads(process: HANDLE, pid: u32) -> io::Result<Vec<OwnedHandle>> {
    let mut snapshot = null_mut();
    // SAFETY: `process` is the root's live process handle; `snapshot` is a valid out
    // pointer. Only thread metadata is captured: no memory, handles or process clone.
    let result = unsafe { PssCaptureSnapshot(process, PSS_CAPTURE_THREADS, 0, &mut snapshot) };
    if result != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(result as i32));
    }
    let snapshot = ThreadSnapshot(snapshot);
    let mut marker = null_mut();
    // SAFETY: null selects the default allocator; `marker` is a valid out pointer.
    let result = unsafe { PssWalkMarkerCreate(null(), &mut marker) };
    if result != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(result as i32));
    }
    let marker = ThreadWalk(marker);
    let mut threads = Vec::new();
    loop {
        let mut entry = PSS_THREAD_ENTRY::default();
        // SAFETY: both PSS objects are live; the buffer has the documented size.
        let result = unsafe {
            PssWalkSnapshot(
                snapshot.0,
                PSS_WALK_THREADS,
                marker.0,
                (&mut entry as *mut PSS_THREAD_ENTRY).cast(),
                size_of::<PSS_THREAD_ENTRY>() as u32,
            )
        };
        if result == ERROR_NO_MORE_ITEMS {
            break;
        }
        if result != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(result as i32));
        }
        if entry.ProcessId != pid || entry.Flags & PSS_THREAD_FLAGS_TERMINATED != 0 {
            return Err(io::Error::other(
                "snapshot did not contain a live thread of the suspended root",
            ));
        }
        // SAFETY: opens an ID from the process snapshot with only the rights needed to
        // check its identity and resume it; it is checked before it is used.
        let thread = unsafe {
            OpenThread(
                THREAD_SUSPEND_RESUME | THREAD_QUERY_LIMITED_INFORMATION,
                FALSE,
                entry.ThreadId,
            )
        };
        if thread.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: a successful OpenThread returns a newly owned kernel handle.
        let thread = unsafe { OwnedHandle::from_raw_handle(thread) };
        let mut created = FILETIME::default();
        let (mut exited, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        // SAFETY: the thread handle is live and all four FILETIME out pointers are valid.
        if unsafe {
            GetThreadTimes(
                thread.as_raw_handle(),
                &mut created,
                &mut exited,
                &mut kernel,
                &mut user,
            )
        } == FALSE
        {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: the handle is live and has THREAD_QUERY_LIMITED_INFORMATION.
        let owner = unsafe { GetProcessIdOfThread(thread.as_raw_handle()) };
        if owner != pid
            || created.dwLowDateTime != entry.CreateTime.dwLowDateTime
            || created.dwHighDateTime != entry.CreateTime.dwHighDateTime
        {
            return Err(io::Error::other(
                "captured thread identity changed before resume",
            ));
        }
        threads.push(thread);
    }
    if threads.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "the suspended root had no thread to resume",
        ));
    }
    Ok(threads)
}

/// Resumes the identity-checked threads of a suspended root already assigned to its Job
/// Object. On any error the caller terminates the whole job.
pub(super) fn resume_threads(process: HANDLE, pid: u32) -> io::Result<()> {
    for thread in suspended_threads(process, pid)? {
        // SAFETY: an owned, identity-checked thread handle of the job-owned root.
        let previous = unsafe { ResumeThread(thread.as_raw_handle()) };
        if previous == u32::MAX {
            return Err(io::Error::last_os_error());
        }
        if previous != 1 {
            return Err(io::Error::other(
                "root thread did not have its initial suspend count",
            ));
        }
    }
    Ok(())
}
