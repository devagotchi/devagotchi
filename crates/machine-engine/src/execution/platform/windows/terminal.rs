//! A pseudo-console (ConPTY) for an interactive operation (`InputMode::Terminal`): the broker
//! holds the console and its two pipes, and the root is created attached to it; and what a
//! program on a plain pipe can be sent on Windows.

use super::process::{
    AttributeList, ConsoleRoot, Root, command_line, environment_block, terminated,
};
use super::*;
use std::ffi::OsString;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll, ready};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use windows_sys::Win32::Foundation::ERROR_BROKEN_PIPE;
use windows_sys::Win32::System::Console::{
    COORD, ClosePseudoConsole, CreatePseudoConsole, HPCON, ResizePseudoConsole,
    SetConsoleCtrlHandler,
};
use windows_sys::Win32::System::Threading::{
    CREATE_UNICODE_ENVIRONMENT, CreateProcessW, EXTENDED_STARTUPINFO_PRESENT,
    PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, PROCESS_INFORMATION, STARTF_USESTDHANDLES, STARTUPINFOEXW,
    STARTUPINFOW,
};

/// A pseudo-console the broker owns, closed on drop. Closing it is what ends the program's
/// console and, once the console host has written the last of its output, that output.
struct PseudoConsole(HPCON);

impl Drop for PseudoConsole {
    fn drop(&mut self) {
        let console = self.0;
        // SAFETY: `console` was created by CreatePseudoConsole and is closed once, here.
        let close = move || unsafe { ClosePseudoConsole(console) };
        // Closing can wait for the console host to finish writing, which the output reader
        // must be free to take: off this thread, when there is a runtime to run it.
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => drop(runtime.spawn_blocking(close)),
            Err(_) => close(),
        }
    }
}

/// The program's side of a pseudo-console: nothing to hand over, since the console is
/// attached when the root is created (`spawn`).
pub(crate) struct ProgramSide;

/// A pseudo-console an operation runs on: the console, the pipe the broker writes what is
/// typed into, and the pipe it reads what the program shows from.
pub(crate) struct Terminal {
    console: PseudoConsole,
    input: Arc<NamedPipeServer>,
    output: Arc<NamedPipeServer>,
}

fn pipe_name(direction: &str) -> String {
    static COUNT: AtomicU64 = AtomicU64::new(0);
    format!(
        r"\\.\pipe\devagotchi-console-{}-{}-{direction}",
        std::process::id(),
        COUNT.fetch_add(1, Ordering::Relaxed)
    )
}

fn size(cols: u16, rows: u16) -> COORD {
    COORD {
        X: cols as i16,
        Y: rows as i16,
    }
}

fn failed(call: &str, result: i32) -> io::Error {
    io::Error::other(format!("{call} failed (HRESULT {result:#010x})"))
}

impl Terminal {
    /// A new pseudo-console of `cols` by `rows`. Its pipes are named pipes of one instance
    /// each, reaching no other machine. The broker opens the console's ends itself at once:
    /// an instance takes one client, so a pipe something else opened first is refused here
    /// and nothing runs.
    pub(crate) async fn open(cols: u16, rows: u16) -> io::Result<(Self, ProgramSide)> {
        let (input_name, output_name) = (pipe_name("in"), pipe_name("out"));
        let input = ServerOptions::new()
            .first_pipe_instance(true)
            .reject_remote_clients(true)
            .max_instances(1)
            .access_inbound(false)
            .create(&input_name)?;
        let output = ServerOptions::new()
            .first_pipe_instance(true)
            .reject_remote_clients(true)
            .max_instances(1)
            .access_outbound(false)
            .create(&output_name)?;
        let console_input = std::fs::OpenOptions::new().read(true).open(&input_name)?;
        let console_output = std::fs::OpenOptions::new().write(true).open(&output_name)?;
        // Already connected (the opens above), so these return at once; until a server has
        // seen its connection it starts no read.
        input.connect().await?;
        output.connect().await?;
        let mut console: HPCON = 0;
        // SAFETY: both handles are open pipe ends owned here; `console` is a valid out pointer.
        let created = unsafe {
            CreatePseudoConsole(
                size(cols, rows),
                console_input.as_raw_handle(),
                console_output.as_raw_handle(),
                0,
                &mut console,
            )
        };
        if created < 0 {
            return Err(failed("CreatePseudoConsole", created));
        }
        // The console host holds its own copies now. Ours close here, so the output pipe
        // ends when the console does.
        drop((console_input, console_output));
        Ok((
            Self {
                console: PseudoConsole(console),
                input: Arc::new(input),
                output: Arc::new(output),
            },
            ProgramSide,
        ))
    }

    /// Reads what the program shows; writes what is typed to it.
    pub(crate) fn io(&self) -> TerminalIo {
        TerminalIo {
            input: self.input.clone(),
            output: self.output.clone(),
        }
    }

    /// The console's new size; its programs are told, as a resized window would tell them.
    pub(crate) fn resize(&self, cols: u16, rows: u16) -> io::Result<()> {
        // SAFETY: the console is open for the life of `self`.
        let resized = unsafe { ResizePseudoConsole(self.console.0, size(cols, rows)) };
        if resized < 0 {
            return Err(failed("ResizePseudoConsole", resized));
        }
        Ok(())
    }

    /// Create the root `command` describes, suspended, attached to this console. The
    /// environment is exactly the command's (on top of the broker's own for an account
    /// policy). Its standard handles are given as invalid, so it takes the console's and
    /// never a handle the broker itself was started with.
    fn spawn(&self, command: &std::process::Command, account: bool) -> io::Result<ConsoleRoot> {
        let mut line = command_line(command.get_program(), command.get_args());
        let mut variables: Vec<(OsString, OsString)> = if account {
            std::env::vars_os().collect()
        } else {
            Vec::new()
        };
        for (name, value) in command.get_envs() {
            variables.retain(|(existing, _)| !existing.eq_ignore_ascii_case(name));
            if let Some(value) = value {
                variables.push((name.into(), value.into()));
            }
        }
        let environment = environment_block(variables);
        let program = terminated(command.get_program());
        let cwd = command
            .get_current_dir()
            .map(|dir| terminated(dir.as_os_str()));
        let mut list = AttributeList::new(1)?;
        // SAFETY: the pseudo-console attribute takes the console itself as its value, and the
        // console outlives the CreateProcessW call below.
        unsafe {
            list.set(
                PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE,
                self.console.0 as *const core::ffi::c_void,
                size_of::<HPCON>(),
            )?;
        }
        let mut startup = STARTUPINFOEXW::default();
        startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.StartupInfo.hStdInput = INVALID_HANDLE_VALUE;
        startup.StartupInfo.hStdOutput = INVALID_HANDLE_VALUE;
        startup.StartupInfo.hStdError = INVALID_HANDLE_VALUE;
        startup.lpAttributeList = list.as_ptr();
        let mut info = PROCESS_INFORMATION {
            hProcess: null_mut(),
            hThread: null_mut(),
            dwProcessId: 0,
            dwThreadId: 0,
        };
        process_ctrl_c();
        // SAFETY: every pointer is to a live, NUL-terminated buffer or initialized struct
        // owned by this function; `line` is mutable, as CreateProcessW requires; no handle
        // is inherited.
        let created = unsafe {
            CreateProcessW(
                program.as_ptr(),
                line.as_mut_ptr(),
                null(),
                null(),
                FALSE,
                CREATE_SUSPENDED | EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
                environment.as_ptr().cast(),
                cwd.as_ref().map_or(null(), |cwd| cwd.as_ptr()),
                &startup.StartupInfo as *const STARTUPINFOW,
                &mut info,
            )
        };
        if created == FALSE {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: both handles were just returned by CreateProcessW and are owned here; the
        // thread's is not needed (the root is resumed through its process id, `adopt`).
        let process = unsafe {
            drop(OwnedHandle::from_raw_handle(info.hThread as RawHandle));
            OwnedHandle::from_raw_handle(info.hProcess as RawHandle)
        };
        ConsoleRoot::new(process, info.dwProcessId)
    }
}

/// A process inherits from the one that created it whether it ignores Ctrl+C, and a broker
/// started in a new process group (`devagotchi start --background`) or from some shells was
/// made to ignore it: every program on its consoles would then ignore Ctrl+C typed into
/// them. The broker stops on Ctrl+C itself (settling its jobs), so it clears an ignore it
/// inherited, once, before its first console root; handlers it registered still run first.
fn process_ctrl_c() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    // SAFETY: a NULL handler with FALSE only clears this process's ignore-Ctrl+C attribute.
    ONCE.call_once(|| unsafe {
        SetConsoleCtrlHandler(None, FALSE);
    });
}

/// Spawn the root `command` describes: on `terminal`, created by hand on its pseudo-console;
/// otherwise by tokio, its stdio piped.
pub(crate) fn spawn(
    command: &mut Command,
    terminal: Option<&Terminal>,
    account_environment: bool,
) -> io::Result<Root> {
    match terminal {
        Some(terminal) => terminal
            .spawn(command.as_std(), account_environment)
            .map(Root::Console),
        None => command.spawn().map(Root::Piped),
    }
}

/// What a root on a pseudo-console needs set on its command: `SystemRoot`, which Windows
/// system libraries need (as `configure` keeps it for a piped root).
pub(crate) fn configure_terminal(
    command: &mut Command,
    _program_side: &ProgramSide,
) -> io::Result<()> {
    if let Some(system_root) = std::env::var_os("SystemRoot") {
        command.env("SystemRoot", system_root);
    }
    Ok(())
}

/// The console's pipes, as one stream. When the console closes, its output pipe breaks:
/// that is the end of what the program showed, read here as end of file.
pub(crate) struct TerminalIo {
    input: Arc<NamedPipeServer>,
    output: Arc<NamedPipeServer>,
}

impl AsyncRead for TerminalIo {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        loop {
            ready!(self.output.poll_read_ready(context))?;
            match self.output.try_read(buffer.initialize_unfilled()) {
                Ok(length) => {
                    buffer.advance(length);
                    return Poll::Ready(Ok(()));
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => continue,
                Err(error)
                    if error.kind() == io::ErrorKind::BrokenPipe
                        || error.raw_os_error() == Some(ERROR_BROKEN_PIPE as i32) =>
                {
                    return Poll::Ready(Ok(()));
                }
                Err(error) => return Poll::Ready(Err(error)),
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
            ready!(self.input.poll_write_ready(context))?;
            match self.input.try_write(bytes) {
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => continue,
                result => return Poll::Ready(result),
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

/// What a program on a plain pipe can be sent. Windows delivers Ctrl-C only to processes
/// sharing a console, and a piped root has none the broker shares, so these refuse and
/// say what works instead.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Signaller;

fn no_signal() -> BridgeError {
    BridgeError::new(
        "no_terminal",
        "On Windows a program on a pipe cannot be sent ctrl_c or ctrl_backslash: start it with interactive mode terminal for that, end its input with ctrl_d, or stop it with operation_cancel",
    )
}

impl Signaller {
    pub(crate) fn interrupt(&self) -> BridgeResult<()> {
        Err(no_signal())
    }

    pub(crate) fn quit(&self) -> BridgeResult<()> {
        Err(no_signal())
    }
}
