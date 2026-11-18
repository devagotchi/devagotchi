//! What one `operation_input` becomes: its text and named keys turned into the steps the
//! running job applies (bytes, a signal, the end of input), and the bounded hand-off of those
//! steps to the job.

use super::*;

pub(super) fn input_closed() -> BridgeError {
    BridgeError::new(
        "input_closed",
        "This operation no longer takes input: it ended, or its input was closed",
    )
}

pub(super) fn no_caller() -> BridgeError {
    invalid("Input needs a caller: send it through devagotchi mcp or a hub")
}

/// What a key sends on a terminal (an xterm in its normal cursor mode).
pub(super) fn key_bytes(key: Key) -> &'static [u8] {
    match key {
        Key::Enter => b"\r",
        Key::Tab => b"\t",
        Key::Backspace => b"\x7f",
        Key::Escape => b"\x1b",
        Key::Up => b"\x1b[A",
        Key::Down => b"\x1b[B",
        Key::Right => b"\x1b[C",
        Key::Left => b"\x1b[D",
        Key::Home => b"\x1b[H",
        Key::End => b"\x1b[F",
        Key::PageUp => b"\x1b[5~",
        Key::PageDown => b"\x1b[6~",
        Key::Delete => b"\x1b[3~",
        Key::Space => b" ",
        Key::CtrlC => b"\x03",
        Key::CtrlD => b"\x04",
        Key::CtrlZ => b"\x1a",
        Key::CtrlL => b"\x0c",
        Key::CtrlU => b"\x15",
        Key::CtrlBackslash => b"\x1c",
    }
}

/// The steps one input takes. On a terminal everything is bytes and the terminal does the
/// rest (Ctrl-C becomes SIGINT for the foreground job). On a pipe there is no terminal:
/// Enter is a newline, Ctrl-C and Ctrl-\\ are signals, Ctrl-D closes the input, and
/// Ctrl-Z, which would stop a program nothing can continue, is refused.
pub(super) fn input_steps(
    mode: InputMode,
    text: &str,
    keys: &[Key],
) -> BridgeResult<Vec<InputStep>> {
    let mut steps = Vec::new();
    let mut bytes = text.as_bytes().to_vec();
    for &key in keys {
        let step = match (mode, key) {
            (InputMode::Terminal, key) => {
                bytes.extend_from_slice(key_bytes(key));
                continue;
            }
            (InputMode::Pipe, Key::Enter) => {
                bytes.push(b'\n');
                continue;
            }
            (InputMode::Pipe, Key::CtrlC) => InputStep::Interrupt,
            (InputMode::Pipe, Key::CtrlBackslash) => InputStep::Quit,
            (InputMode::Pipe, Key::CtrlD) => InputStep::Close,
            (InputMode::Pipe, Key::CtrlZ) => {
                return Err(BridgeError::new(
                    "no_terminal",
                    "ctrl_z needs a terminal: on a pipe it would stop the program with nothing to continue it",
                ));
            }
            (InputMode::Pipe, key) => {
                bytes.extend_from_slice(key_bytes(key));
                continue;
            }
        };
        if !bytes.is_empty() {
            steps.push(InputStep::Bytes(std::mem::take(&mut bytes)));
        }
        steps.push(step);
    }
    if !bytes.is_empty() {
        steps.push(InputStep::Bytes(bytes));
    }
    if let Some(position) = steps.iter().position(|step| *step == InputStep::Close)
        && position + 1 != steps.len()
    {
        return Err(invalid(
            "Nothing can follow ctrl_d on a pipe: it closes the input",
        ));
    }
    Ok(steps)
}

/// Hand steps to the running job and wait (bounded) until the program has them.
pub(super) async fn deliver(
    sender: &mpsc::Sender<InputRequest>,
    steps: Vec<InputStep>,
) -> BridgeResult<()> {
    let (reply, answer) = oneshot::channel();
    match sender.try_send(InputRequest { steps, reply }) {
        Ok(()) => {}
        Err(mpsc::error::TrySendError::Full(_)) => {
            return Err(BridgeError::new(
                "input_busy",
                "Earlier input to this operation is still waiting for the program to read it",
            ));
        }
        Err(mpsc::error::TrySendError::Closed(_)) => return Err(input_closed()),
    }
    match timeout(INPUT_WAIT, answer).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err(input_closed()),
        Err(_) => Err(BridgeError::new(
            "input_blocked",
            "The program has not read its input for five seconds; it stays queued and is delivered when it reads",
        )),
    }
}
