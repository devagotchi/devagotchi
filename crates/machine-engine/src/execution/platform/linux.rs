//! Linux process ownership: the root's inherited process group.
//!
//! The root stays unreaped while it is observed and signalled, so its PID and
//! group identity cannot be reused by an unrelated process.
//!
//! Exits are events, not polls: a pidfd (Linux 5.3+) becomes readable when the process it
//! names exits, so the broker sleeps until the root ends, then until every executing
//! member of its group has ended, re-reading /proc only when one of them did. A kernel or
//! sandbox without pidfd_open falls back to looking every 20 ms, as before.
// Native OS calls live here and nowhere else in this crate's dependents.
#![allow(unsafe_code)]

use crate::execution::GROUP_SETTLE;
use devagotchi_contracts::{BridgeError, BridgeResult, OperationState};
use std::fs::Metadata;
use std::io;
use std::io::{Read as _, Write as _};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::pin::Pin;
use std::process::Stdio;
use std::sync::Arc;
use std::task::{Context, Poll, ready};
use std::time::Duration;
use tokio::io::Interest;
use tokio::io::unix::AsyncFd;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::process::{Child, Command};
use tokio::time::{sleep, timeout};

/// Longest wait between SIGTERM and SIGKILL; it ends early once the group has left.
const TERM_GRACE: Duration = Duration::from_millis(200);
/// Members watched at once. Settlement re-reads the group once all of them have ended, so
/// a larger group is watched in turns.
const MAX_WATCHED: usize = 64;
/// The look interval when this kernel offers no pidfd.
const NO_PIDFD_INTERVAL: Duration = Duration::from_millis(20);
/// A pidfd says only when its process exits. A member can leave the group without exiting
/// (`setsid`, as a daemon does), so the group is read again at least this often.
const MEMBERSHIP_RECHECK: Duration = Duration::from_millis(100);

pub(crate) const SCOPE: &str = "Linux root process and inherited process group; excludes processes that leave the group. This is not OS containment.";
/// An operation on a pseudo-terminal: its root leads a new session, and job control may put
/// its children in process groups of their own, so the session is what is owned.
pub(crate) const SCOPE_TERMINAL: &str = "Linux root process and the session it leads on a broker-owned pseudo-terminal, every process group in it included; excludes processes that start a session of their own. This is not OS containment.";
pub(crate) const RUNNING_DETAIL: &str = "Root spawned with exact argv, cleared environment, null stdin, and a new Linux process group. Executable behavior is not sandboxed.";
pub(crate) const TERMINATION_VERIFIED_DETAIL: &str = " SIGTERM then SIGKILL were issued while the root PID was still owned; no executing members remained in its inherited process group.";
pub(crate) const OBSERVATION_FAILED_DETAIL: &str =
    "Process-group observation failed and requested cancellation. ";
pub(crate) const TERMINATION_UNVERIFIED_DETAIL: &str = " Group termination was not verified";
pub(crate) const PIPE_TIMEOUT_DETAIL: &str = " Output pipes did not close within one second of root settlement; a descendant may still hold them open. Reading was stopped, output is incomplete, and processes outside the owned group are not tracked or terminated.";
pub(crate) const RUNNING_DETAIL_ACCOUNT: &str = "Root spawned as the broker's account with its environment, null stdin, and a new Linux process group. Executable behavior is not sandboxed.";

/// Both input modes are available on Linux.
pub(crate) fn supports_interactive(_mode: devagotchi_contracts::InputMode) -> bool {
    true
}

/// What a running operation's detail says: who it runs as, and where its input comes from.
pub(crate) fn running_detail(
    account: bool,
    mode: Option<devagotchi_contracts::InputMode>,
) -> String {
    use devagotchi_contracts::InputMode;
    let who = if account {
        "as the broker's account with its environment"
    } else {
        "with exact argv and a cleared environment"
    };
    match mode {
        None if account => RUNNING_DETAIL_ACCOUNT.into(),
        None => RUNNING_DETAIL.into(),
        Some(InputMode::Pipe) => format!(
            "Root spawned {who}, stdin a broker-held pipe for operation_input, and a new Linux process group. Executable behavior is not sandboxed."
        ),
        Some(InputMode::Terminal) => format!(
            "Root spawned {who} plus TERM=xterm-256color, leading a new session whose controlling terminal is a broker-owned pseudo-terminal (stdin, stdout and stderr). Executable behavior is not sandboxed."
        ),
    }
}
pub(crate) const EXECUTABLE_RULE: &str = "";

/// The shell a `command` line runs in: the account's `$SHELL` when it is one of the
/// Bourne family, else bash, else `/bin/sh`. A login shell of `nologin` or `false` (a
/// service account) is never used. It never fails; the `Result` matches Windows, where a
/// shell script needs Git Bash.
pub(crate) fn shell_command(command: &str) -> BridgeResult<(std::path::PathBuf, Vec<String>)> {
    let bourne = |path: &Path| {
        matches!(
            path.file_name().and_then(|name| name.to_str()),
            Some("bash" | "zsh" | "sh" | "dash" | "ksh")
        ) && std::fs::metadata(path).is_ok_and(|metadata| is_directly_executable(path, &metadata))
    };
    let shell = std::env::var_os("SHELL")
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_absolute() && bourne(path))
        .or_else(|| Some(std::path::PathBuf::from("/bin/bash")).filter(|path| bourne(path)))
        .unwrap_or_else(|| std::path::PathBuf::from("/bin/sh"));
    Ok((shell, vec!["-c".to_owned(), command.to_owned()]))
}

/// A Linux grant or request target must be a regular file with an execute bit.
pub(crate) fn is_directly_executable(_path: &Path, metadata: &Metadata) -> bool {
    metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
}

/// Starts the root in its own process group, detached from the broker's
/// terminal signals.
pub(crate) fn configure(command: &mut Command) {
    command.as_std_mut().process_group(0);
}

/// What an owned tree is: the root's process group, or (on a pseudo-terminal) the session
/// it leads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scope {
    Group,
    Session,
}

/// Why a spawned root could not be taken into ownership.
pub(crate) struct AdoptFailure {
    pub(crate) state: OperationState,
    pub(crate) output_complete: bool,
    pub(crate) detail: String,
}

/// The root's PID and inherited process group, held while the root is unreaped, and a
/// pidfd for the root when the kernel offers one.
pub(crate) struct OwnedTree {
    pid: u32,
    root: Option<AsyncFd<OwnedFd>>,
    scope: Scope,
}

impl OwnedTree {
    pub(crate) async fn adopt(child: &mut Child, scope: Scope) -> Result<Self, AdoptFailure> {
        let Some(pid) = child.id() else {
            return Err(AdoptFailure {
                state: OperationState::Unknown,
                output_complete: false,
                detail: "Spawn returned no process identity; resource status cannot be certified."
                    .into(),
            });
        };
        // The root is this broker's unreaped child, so its PID names it alone.
        let root = match pidfd(pid) {
            Ok(root) => root,
            Err(error) if pidfd_unsupported(&error) => None,
            Err(error) => {
                return Err(AdoptFailure {
                    state: OperationState::Unknown,
                    output_complete: false,
                    detail: format!("Cannot open a pidfd for the owned root: {error}"),
                });
            }
        };
        Ok(Self { pid, root, scope })
    }

    /// Signals the root's process group on the operation's behalf (Ctrl-C on a pipe). Only
    /// while the root is unreaped, which the job guarantees by stopping input first.
    pub(crate) fn signaller(&self) -> Signaller {
        Signaller(self.pid)
    }

    /// Returns when the root has exited, without reaping it.
    pub(crate) async fn root_exit(&self, _child: &mut Child) -> BridgeResult<()> {
        let Some(root) = &self.root else {
            while !root_exited_without_reaping(self.pid)? {
                sleep(NO_PIDFD_INTERVAL).await;
            }
            return Ok(());
        };
        loop {
            let mut ready = root.readable().await.map_err(observation)?;
            if root_exited_without_reaping(self.pid)? {
                return Ok(());
            }
            ready.clear_ready();
        }
    }

    /// Returns when no member of the owned group is executing. It reads /proc, sleeps
    /// until every member it found has exited, and reads again: a member may have started
    /// another before it ended.
    pub(crate) async fn members_settled(&self) -> BridgeResult<()> {
        loop {
            let members = members(self.pid, self.scope)?;
            if members.is_empty() {
                return Ok(());
            }
            if self.root.is_none() {
                sleep(NO_PIDFD_INTERVAL).await;
                continue;
            }
            let mut watched = Vec::new();
            for member in members.into_iter().take(MAX_WATCHED) {
                let Some(fd) = pidfd(member).map_err(observation)? else {
                    continue;
                };
                // Opened by number: keep it only if that number still names a member.
                if member_state(member, self.pid, self.scope)? {
                    watched.push(fd);
                }
            }
            let all_ended = async {
                for fd in &watched {
                    fd.readable().await.map_err(observation)?.retain_ready();
                }
                Ok::<_, BridgeError>(())
            };
            if let Ok(ended) = timeout(MEMBERSHIP_RECHECK, all_ended).await {
                ended?;
            }
        }
    }

    /// SIGTERM, up to a short grace for the group to leave, SIGKILL, then verified
    /// settlement of the group.
    pub(crate) async fn terminate(&self) -> BridgeResult<()> {
        let term_error = self.signal_all(libc::SIGTERM).err();
        let _ = timeout(TERM_GRACE, self.members_settled()).await;
        let kill_error = self.signal_all(libc::SIGKILL).err();
        if let Some(error) = term_error.or(kill_error) {
            return Err(error);
        }
        match timeout(GROUP_SETTLE, self.members_settled()).await {
            Ok(settled) => settled,
            Err(_) => Err(BridgeError::new(
                "termination_unverified",
                "Executing members remained in the owned process group after SIGKILL",
            )),
        }
    }

    /// Every owned member: the group with one call, a session member by member (job control
    /// gives a terminal's pipelines groups of their own). A member is signalled through a
    /// pidfd opened and then checked to be a member, so a number reused in between is never
    /// signalled; without pidfds, by number right after the check.
    fn signal_all(&self, signal: i32) -> BridgeResult<()> {
        if self.scope == Scope::Group {
            return signal_group(self.pid, signal);
        }
        for member in members(self.pid, self.scope)? {
            match pidfd(member) {
                Ok(Some(fd)) => {
                    if !member_state(member, self.pid, self.scope)? {
                        continue;
                    }
                    // SAFETY: the pidfd is open and names exactly that process.
                    let sent = unsafe {
                        libc::syscall(
                            libc::SYS_pidfd_send_signal,
                            fd.get_ref().as_raw_fd(),
                            signal,
                            std::ptr::null::<libc::siginfo_t>(),
                            0u32,
                        )
                    };
                    if sent == -1 {
                        let error = io::Error::last_os_error();
                        if error.raw_os_error() != Some(libc::ESRCH) {
                            return Err(BridgeError::new(
                                "process_signal_error",
                                format!("Cannot signal an owned session member: {error}"),
                            ));
                        }
                    }
                }
                Ok(None) => {}
                Err(error) if pidfd_unsupported(&error) => {
                    if member_state(member, self.pid, self.scope)? {
                        // SAFETY: plain kill(2) of a member just checked to be one.
                        unsafe { libc::kill(member as libc::pid_t, signal) };
                    }
                }
                Err(error) => return Err(observation(error)),
            }
        }
        Ok(())
    }
}

/// Signals an operation's root group for its input (Ctrl-C and Ctrl-\\ on a pipe).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Signaller(u32);

impl Signaller {
    pub(crate) fn interrupt(&self) -> BridgeResult<()> {
        signal_group(self.0, libc::SIGINT)
    }

    pub(crate) fn quit(&self) -> BridgeResult<()> {
        signal_group(self.0, libc::SIGQUIT)
    }
}

fn observation(error: io::Error) -> BridgeError {
    BridgeError::new("process_observation_error", error.to_string())
}

/// A pidfd for `pid`, registered with the runtime: readable once that process has exited,
/// and naming that process only, whatever later happens to the number. None when no such
/// process exists any more.
fn pidfd(pid: u32) -> io::Result<Option<AsyncFd<OwnedFd>>> {
    let pid =
        libc::pid_t::try_from(pid).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
    if fd < 0 {
        let error = io::Error::last_os_error();
        return match error.raw_os_error() {
            Some(libc::ESRCH) => Ok(None),
            _ => Err(error),
        };
    }
    // pidfd_open returned a new descriptor that nothing else owns, and the OwnedFd keeps
    // it open, unreplaced, for as long as the runtime has it registered.
    let fd = unsafe { OwnedFd::from_raw_fd(fd as i32) };
    Ok(Some(unsafe {
        AsyncFd::register_with_interest(fd, Interest::READABLE)
    }?))
}

/// No pidfd here: a kernel before 5.3, or a sandbox that filters the call.
fn pidfd_unsupported(error: &io::Error) -> bool {
    matches!(
        error.raw_os_error(),
        Some(libc::ENOSYS | libc::EPERM | libc::EACCES)
    )
}

// WNOWAIT observes the root without releasing its PID. The same owned child is
// not reaped until all process-group signals have been sent, preventing PID reuse
// from retargeting a later group signal at an unrelated process.
fn root_exited_without_reaping(pid: u32) -> BridgeResult<bool> {
    let mut information: libc::siginfo_t = unsafe { std::mem::zeroed() };
    let result = unsafe {
        libc::waitid(
            libc::P_PID,
            pid,
            &mut information,
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        )
    };
    if result == -1 {
        let error = std::io::Error::last_os_error();
        if error.kind() == std::io::ErrorKind::Interrupted {
            return Ok(false);
        }
        return Err(BridgeError::new(
            "process_observation_error",
            format!("Cannot observe the owned unreaped root: {error}"),
        ));
    }
    Ok(unsafe { information.si_pid() } != 0)
}

fn signal_group(pid: u32, signal: i32) -> BridgeResult<()> {
    if pid == 0 || pid > i32::MAX as u32 {
        return Err(BridgeError::new(
            "process_signal_error",
            "Invalid owned process group",
        ));
    }
    let result = unsafe { libc::kill(-(pid as i32), signal) };
    if result == -1 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::ESRCH) {
            return Err(BridgeError::new(
                "process_signal_error",
                format!("Cannot signal the owned process group: {error}"),
            ));
        }
    }
    Ok(())
}

/// The executing members of the owned group, read from /proc while the root remains
/// unreaped. Zombies have ended execution and cannot run or create more processes.
#[cfg(test)]
fn group_members(group: u32) -> BridgeResult<Vec<u32>> {
    members(group, Scope::Group)
}

/// The executing members of the root's group or session, read from /proc while the root
/// remains unreaped.
fn members(root: u32, scope: Scope) -> BridgeResult<Vec<u32>> {
    let entries = std::fs::read_dir("/proc")
        .map_err(|error| BridgeError::new("process_observation_error", error.to_string()))?;
    let mut members = Vec::new();
    for entry in entries {
        let entry = entry
            .map_err(|error| BridgeError::new("process_observation_error", error.to_string()))?;
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        if member_state(pid, root, scope)? {
            members.push(pid);
        }
    }
    Ok(members)
}

/// Whether `pid` is an executing member of the root's group (or session) now; a process
/// that is gone is not.
fn member_state(pid: u32, root: u32, scope: Scope) -> BridgeResult<bool> {
    let stat = match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => stat,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        // A process that exits while its stat is read reports ESRCH.
        Err(error) if error.raw_os_error() == Some(libc::ESRCH) => return Ok(false),
        Err(error) => {
            return Err(BridgeError::new(
                "process_observation_error",
                format!("Cannot inspect /proc process groups: {error}"),
            ));
        }
    };
    let Some(end) = stat.rfind(')') else {
        return Err(BridgeError::new(
            "process_observation_error",
            "Malformed /proc process stat",
        ));
    };
    // state, ppid, pgrp, session
    let fields: Vec<_> = stat[end + 1..].split_whitespace().take(4).collect();
    if fields.len() != 4 {
        return Err(BridgeError::new(
            "process_observation_error",
            "Incomplete /proc process stat",
        ));
    }
    let owner = match scope {
        Scope::Group => fields[2],
        Scope::Session => fields[3],
    };
    Ok(owner.parse::<u32>().ok() == Some(root) && fields[0] != "Z" && fields[0] != "X")
}

mod terminal;
pub(crate) use terminal::{Terminal, TerminalIo, configure_terminal, piped, spawn};

#[cfg(test)]
mod tests;

/// A process started by [`spawn_detached`].
pub(crate) struct DetachedChild(std::process::Child);

impl DetachedChild {
    pub(crate) fn try_wait(&mut self) -> std::io::Result<Option<i32>> {
        use std::os::unix::process::ExitStatusExt;
        Ok(self
            .0
            .try_wait()?
            .map(|status| status.code().unwrap_or(128 + status.signal().unwrap_or(0))))
    }
}

/// Start a process in a session of its own, with only its stdin (the null device) and its
/// log open: every other descriptor the caller had, an AI client's pipes among them, is
/// closed when it starts (`detached.rs`).
pub(crate) fn spawn_detached(
    spec: &crate::execution::detached::DetachedSpec<'_>,
) -> std::io::Result<DetachedChild> {
    use std::os::unix::fs::OpenOptionsExt;
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(spec.log)?;
    // An existing log keeps its mode on open; it is the user's alone either way.
    log.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    let mut command = std::process::Command::new(spec.program);
    command
        .args(spec.args)
        .env_clear()
        .envs(spec.env.iter().map(|(k, v)| (k, v)))
        .current_dir(spec.cwd)
        .stdin(std::process::Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    // SAFETY: between fork and exec this calls only async-signal-safe functions
    // (setsid, syscall, sysconf, fcntl) and allocates nothing.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            // Everything above stderr closes on exec. CLOSE_RANGE_CLOEXEC (Linux 5.11+)
            // marks them in one call; older kernels get the same, one descriptor at a time.
            let marked = libc::syscall(
                libc::SYS_close_range,
                3u32,
                u32::MAX,
                libc::CLOSE_RANGE_CLOEXEC as u32,
            );
            if marked != 0 {
                let max = match libc::sysconf(libc::_SC_OPEN_MAX) {
                    n if n > 0 => n.min(65_536) as libc::c_int,
                    _ => 4096,
                };
                for fd in 3..max {
                    libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
                }
            }
            Ok(())
        });
    }
    command.spawn().map(DetachedChild)
}
