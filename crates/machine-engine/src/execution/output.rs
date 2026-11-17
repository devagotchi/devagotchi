//! A process's output: drained from its pipes into the broker's retained frames, raw bytes
//! kept and text decoded across frame boundaries. `run` asks for one pipe carrying stdout and
//! stderr, so the output keeps the order the program wrote it.

use crate::broker::Shared;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt};

const FRAME_BYTES: usize = 1024;

/// Stateful UTF-8 decoding. Raw bytes are retained separately and never transformed.
#[derive(Default)]
pub(super) struct Utf8Decoder {
    pending: Vec<u8>,
}

impl Utf8Decoder {
    fn push(&mut self, bytes: &[u8], eof: bool) -> String {
        self.pending.extend_from_slice(bytes);
        let mut output = String::new();
        let mut consumed = 0;
        while consumed < self.pending.len() {
            match std::str::from_utf8(&self.pending[consumed..]) {
                Ok(text) => {
                    output.push_str(text);
                    consumed = self.pending.len();
                }
                Err(error) => {
                    let valid = error.valid_up_to();
                    // from_utf8 validated this exact prefix.
                    output.push_str(
                        std::str::from_utf8(&self.pending[consumed..consumed + valid])
                            .expect("validated UTF-8 prefix"),
                    );
                    consumed += valid;
                    match error.error_len() {
                        Some(invalid) => {
                            output.push('\u{fffd}');
                            consumed += invalid;
                        }
                        None if eof => {
                            output.push('\u{fffd}');
                            consumed = self.pending.len();
                        }
                        None => break,
                    }
                }
            }
        }
        if consumed > 0 {
            self.pending.drain(..consumed);
        }
        output
    }
}

#[derive(Debug)]
pub(super) struct PipeResult {
    pub(super) complete: bool,
    pub(super) error: Option<String>,
}

pub(super) async fn drain_pipe<R: AsyncRead + Unpin>(
    shared: Arc<Shared>,
    id: String,
    stream: &'static str,
    mut pipe: R,
) -> PipeResult {
    let mut decoder = Utf8Decoder::default();
    let mut buffer = [0_u8; FRAME_BYTES];
    loop {
        match pipe.read(&mut buffer).await {
            Ok(0) => {
                let text = decoder.push(&[], true);
                shared.output(&id, stream, Vec::new(), text);
                return PipeResult {
                    complete: true,
                    error: None,
                };
            }
            Ok(length) => {
                let bytes = buffer[..length].to_vec();
                let text = decoder.push(&bytes, false);
                shared.output(&id, stream, bytes, text);
            }
            Err(error) => {
                return PipeResult {
                    complete: false,
                    error: Some(format!("{stream} read failed: {error}")),
                };
            }
        }
    }
}

/// A pipe whose write end goes to both stdout and stderr.
pub(super) fn one_pipe() -> std::io::Result<(std::io::PipeReader, Stdio, Stdio)> {
    let (reader, writer) = std::io::pipe()?;
    let second = writer.try_clone()?;
    Ok((reader, Stdio::from(writer), Stdio::from(second)))
}

/// Drain the one pipe of a combined operation. On Linux it is read asynchronously; on
/// Windows an anonymous pipe cannot be, so a blocking thread reads it.
#[cfg(unix)]
pub(super) async fn drain_combined(
    shared: Arc<Shared>,
    id: String,
    reader: std::io::PipeReader,
) -> PipeResult {
    use std::os::fd::OwnedFd;
    match tokio::net::unix::pipe::Receiver::from_owned_fd(OwnedFd::from(reader)) {
        Ok(pipe) => drain_pipe(shared, id, "stdout", pipe).await,
        Err(error) => PipeResult {
            complete: false,
            error: Some(format!("Output pipe could not be read: {error}")),
        },
    }
}

#[cfg(windows)]
pub(super) async fn drain_combined(
    shared: Arc<Shared>,
    id: String,
    mut reader: std::io::PipeReader,
) -> PipeResult {
    use std::io::Read as _;
    let read = tokio::task::spawn_blocking(move || {
        let mut decoder = Utf8Decoder::default();
        let mut buffer = [0_u8; FRAME_BYTES];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => {
                    shared.output(&id, "stdout", Vec::new(), decoder.push(&[], true));
                    return PipeResult {
                        complete: true,
                        error: None,
                    };
                }
                Ok(length) => {
                    let bytes = buffer[..length].to_vec();
                    let text = decoder.push(&bytes, false);
                    shared.output(&id, "stdout", bytes, text);
                }
                // A closed pipe on Windows reads as broken, not as end of file.
                Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => {
                    shared.output(&id, "stdout", Vec::new(), decoder.push(&[], true));
                    return PipeResult {
                        complete: true,
                        error: None,
                    };
                }
                Err(error) => {
                    return PipeResult {
                        complete: false,
                        error: Some(format!("stdout read failed: {error}")),
                    };
                }
            }
        }
    });
    read.await.unwrap_or_else(|error| PipeResult {
        complete: false,
        error: Some(format!("Output reader did not settle normally: {error}")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_is_identical_across_every_split_boundary() {
        let original = "ASCII € 🐄 日本語\n";
        for first in 0..=original.len() {
            for second in first..=original.len() {
                let mut decoder = Utf8Decoder::default();
                let bytes = original.as_bytes();
                let mut result = decoder.push(&bytes[..first], false);
                result.push_str(&decoder.push(&bytes[first..second], false));
                result.push_str(&decoder.push(&bytes[second..], true));
                assert_eq!(result, original, "split at {first}/{second}");
                assert!(decoder.pending.is_empty());
            }
        }
    }

    #[test]
    fn utf8_invalid_and_truncated_sequences_are_explicit() {
        let mut decoder = Utf8Decoder::default();
        assert_eq!(decoder.push(&[0xe2], false), "");
        assert_eq!(decoder.push(&[0x82], false), "");
        assert_eq!(decoder.push(&[], true), "�");
        assert_eq!(decoder.push(&[0xff, 0, b'x'], true), "�\0x");
    }
}
