//! An interactive job's input: the steps one `operation_input` becomes, and the task that
//! applies them in order to the program's pipe or pseudo-terminal.

use super::platform;
use devagotchi_contracts::{BridgeError, BridgeResult};
use tokio::io::AsyncWriteExt;
use tokio::process::ChildStdin;
use tokio::sync::{mpsc, oneshot};

/// One step of input, applied in order: bytes to the program, a signal to its group (a
/// pipe's Ctrl-C), the end of its input (a pipe's Ctrl-D), or a new terminal size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InputStep {
    Bytes(Vec<u8>),
    Interrupt,
    Quit,
    Close,
    Resize { cols: u16, rows: u16 },
}

/// Steps for the running job, answered once all are applied or one fails.
pub(crate) struct InputRequest {
    pub(crate) steps: Vec<InputStep>,
    pub(crate) reply: oneshot::Sender<BridgeResult<()>>,
    pub(crate) permit: Option<crate::broker::ControlPermit>,
}

/// Where an interactive job's input goes.
pub(super) enum InputSink {
    Pipe {
        stdin: Option<ChildStdin>,
        signals: platform::Signaller,
    },
    Terminal {
        io: platform::TerminalIo,
        terminal: platform::Terminal,
    },
}

fn input_failed(error: std::io::Error) -> BridgeError {
    match error.kind() {
        std::io::ErrorKind::BrokenPipe => BridgeError::new(
            "input_closed",
            "The program no longer reads its input (it closed it or exited)",
        ),
        _ => BridgeError::new(
            "input_failed",
            format!("Input could not be delivered: {error}"),
        ),
    }
}

impl InputSink {
    async fn apply(&mut self, step: InputStep) -> BridgeResult<()> {
        match (self, step) {
            (Self::Pipe { stdin, .. }, InputStep::Bytes(bytes)) => {
                let Some(stdin) = stdin else {
                    return Err(BridgeError::new(
                        "input_closed",
                        "This operation's input was closed (ctrl_d)",
                    ));
                };
                stdin.write_all(&bytes).await.map_err(input_failed)?;
                stdin.flush().await.map_err(input_failed)
            }
            (Self::Pipe { signals, .. }, InputStep::Interrupt) => signals.interrupt(),
            (Self::Pipe { signals, .. }, InputStep::Quit) => signals.quit(),
            (Self::Pipe { stdin, .. }, InputStep::Close) => {
                // A timed-out Windows write can still own the pipe in Tokio's
                // blocking worker. Drain that in-flight write before acknowledging
                // EOF; shutdown alone is a no-op for that writer. Keep stdin here
                // if the close permit expires so a later close can finish it.
                if let Some(pipe) = stdin.as_mut() {
                    pipe.flush().await.map_err(input_failed)?;
                }
                stdin.take();
                Ok(())
            }
            (Self::Terminal { io, .. }, InputStep::Bytes(bytes)) => {
                io.write_all(&bytes).await.map_err(input_failed)
            }
            (Self::Terminal { terminal, .. }, InputStep::Resize { cols, rows }) => {
                terminal.resize(cols, rows).map_err(|error| {
                    BridgeError::new("input_failed", format!("Resize failed: {error}"))
                })
            }
            (_, step) => Err(BridgeError::new(
                "invalid_request",
                format!("{step:?} does not apply to this operation's input"),
            )),
        }
    }
}

/// Apply each request's steps in order, one request at a time, so two requests never
/// interleave. Ends when the broker drops its sender or the job stops it.
pub(super) async fn serve_input(mut requests: mpsc::Receiver<InputRequest>, mut sink: InputSink) {
    while let Some(request) = requests.recv().await {
        let mut result = Ok(());
        for step in request.steps {
            let applied = if let Some(permit) = &request.permit {
                match permit.check() {
                    Err(error) => Err(error),
                    Ok(()) => {
                        match tokio::time::timeout_at(permit.deadline, sink.apply(step)).await {
                            Ok(result) => result,
                            Err(_) => Err(crate::broker::ControlPermit::expired()),
                        }
                    }
                }
            } else {
                sink.apply(step).await
            };
            if let Err(error) = applied {
                result = Err(error);
                break;
            }
        }
        let _ = request.reply.send(result);
    }
}
