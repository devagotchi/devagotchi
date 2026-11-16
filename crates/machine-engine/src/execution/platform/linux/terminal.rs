//! A pseudo-terminal for an interactive operation (`InputMode::Terminal`): the broker holds
//! the master, the program the slave as its controlling terminal in a session of its own.

use super::*;

/// The broker's side (the master) of a pseudo-terminal an operation runs on.
pub(crate) struct Terminal {
    master: Arc<AsyncFd<std::fs::File>>,
}

impl Terminal {
    /// A new pseudo-terminal of `cols` by `rows`, and its program side for the root.
    pub(crate) async fn open(cols: u16, rows: u16) -> io::Result<(Self, OwnedFd)> {
        use std::os::unix::fs::OpenOptionsExt;
        // std opens with O_CLOEXEC: no program inherits the master.
        let master = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NOCTTY)
            .open("/dev/ptmx")?;
        let fd = master.as_raw_fd();
        // SAFETY: `fd` is the open ptmx descriptor `master` owns.
        if unsafe { libc::grantpt(fd) } != 0 || unsafe { libc::unlockpt(fd) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let mut name = [0 as libc::c_char; 128];
        // SAFETY: the buffer and its length are this array's.
        let failed = unsafe { libc::ptsname_r(fd, name.as_mut_ptr(), name.len()) };
        if failed != 0 {
            return Err(io::Error::from_raw_os_error(failed));
        }
        // SAFETY: ptsname_r succeeded, so it wrote a NUL-terminated name into the buffer.
        let path = unsafe { std::ffi::CStr::from_ptr(name.as_ptr()) };
        let program_side = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NOCTTY)
            .open(std::ffi::OsStr::from_bytes(path.to_bytes()))?;
        set_size(fd, cols, rows)?;
        // SAFETY: F_GETFL and F_SETFL on a descriptor `master` owns.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags == -1 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } == -1
        {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `master` is an open File the AsyncFd owns from here on; its descriptor stays
        // open and unchanged until the AsyncFd drops it.
        let master = unsafe { AsyncFd::register(master) }.map_err(io::Error::from)?;
        Ok((
            Self {
                master: Arc::new(master),
            },
            program_side.into(),
        ))
    }

    /// Reads what the program writes; writes what is typed to it.
    pub(crate) fn io(&self) -> TerminalIo {
        TerminalIo(self.master.clone())
    }

    /// The program's foreground process group gets SIGWINCH from the kernel.
    pub(crate) fn resize(&self, cols: u16, rows: u16) -> io::Result<()> {
        set_size(self.master.get_ref().as_raw_fd(), cols, rows)
    }
}

fn set_size(fd: i32, cols: u16, rows: u16) -> io::Result<()> {
    let size = libc::winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: TIOCSWINSZ reads one winsize through the pointer, which outlives the call.
    if unsafe { libc::ioctl(fd, libc::TIOCSWINSZ, &raw const size) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// The master side, as a stream. Once every program side has closed, a read fails with
/// EIO on Linux: that is the end of the program's output, read here as end of file.
pub(crate) struct TerminalIo(Arc<AsyncFd<std::fs::File>>);

impl AsyncRead for TerminalIo {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        loop {
            let mut guard = ready!(self.0.poll_read_ready(context))?;
            let unfilled = buffer.initialize_unfilled();
            match guard.try_io(|master| {
                let mut file: &std::fs::File = master.get_ref();
                file.read(unfilled)
            }) {
                Ok(Ok(length)) => {
                    buffer.advance(length);
                    return Poll::Ready(Ok(()));
                }
                Ok(Err(error)) if error.raw_os_error() == Some(libc::EIO) => {
                    return Poll::Ready(Ok(()));
                }
                Ok(Err(error)) => return Poll::Ready(Err(error)),
                Err(_would_block) => continue,
            }
        }
    }
}

impl AsyncWrite for TerminalIo {
    fn poll_write(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        loop {
            let mut guard = ready!(self.0.poll_write_ready(context))?;
            match guard.try_io(|master| {
                let mut file: &std::fs::File = master.get_ref();
                file.write(bytes)
            }) {
                Ok(result) => return Poll::Ready(result),
                Err(_would_block) => continue,
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

/// Puts the root on the terminal: its stdin, stdout and stderr are the program side, and
/// it leads a new session whose controlling terminal that is.
pub(crate) fn configure_terminal(command: &mut Command, program_side: &OwnedFd) -> io::Result<()> {
    command
        .stdin(Stdio::from(program_side.try_clone()?))
        .stdout(Stdio::from(program_side.try_clone()?))
        .stderr(Stdio::from(program_side.try_clone()?));
    // SAFETY: between fork and exec this calls only async-signal-safe setsid and ioctl, and
    // allocates nothing.
    unsafe {
        command.as_std_mut().pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(io::Error::last_os_error());
            }
            if libc::ioctl(0, libc::TIOCSCTTY, 0 as libc::c_int) == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    Ok(())
}

/// The root as the broker holds it. On Linux every root is a spawned child, a terminal's too:
/// the program side of its pseudo-terminal is the child's stdio.
pub(crate) type Root = Child;

/// Spawn the root `command` describes; on Linux a terminal is already in `command`
/// (`configure_terminal`).
pub(crate) fn spawn(
    command: &mut Command,
    _terminal: Option<&Terminal>,
    _account_environment: bool,
) -> io::Result<Root> {
    command.spawn()
}

/// The root's piped stdio, where it has any.
pub(crate) fn piped(root: &mut Root) -> Option<&mut Child> {
    Some(root)
}
