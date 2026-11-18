//! File requests: answered directly (no operation, nothing journaled) where the broker's
//! policy lets files be reached, on the blocking pool so a large file or a long search never
//! stalls the broker's jobs, waits or other callers. A refusal goes to the broker log.
//!
//! # Mutation ordering
//!
//! `write_file`, `edit_file`, `create_directory` and `move_file` take one gate per broker
//! ([`MutationGate`]) for the whole of their work: path resolution, the preconditions
//! (`expected_sha256`, `mode: create`, the count of `old_text`, an existing move destination),
//! the change and the answer. Two writes guarded by the same SHA-256 cannot both succeed: the
//! second sees `changed_since_read`; of two creates or two moves onto one path, one gets
//! `already_exists`; of two edits of the same text, one gets `edit_mismatch`. Disjoint edits and
//! unconditional appends both land. A write that leaves the checksum as it was does not use up
//! that checksum, so a later request guarded by it can still succeed. One gate, not one per
//! path: it also orders directory moves and two spellings of one path, so unrelated mutations
//! wait too. That is a choice for correctness, not throughput. Reads (`read_file`,
//! `list_directory`, `get_file_info`, `search_files`), operations and every other request never
//! take it, and it is never held with the job state's lock. Waiting is async, so a queued
//! mutation holds no blocking thread.
//!
//! **Cancellation.** The blocking worker owns the gate once it starts: dropping the caller's
//! future cannot let another mutation pass its checks while the first is still changing files.
//! A future dropped while it still waits for the gate changes nothing. A worker that fails or
//! panics releases the gate (the panic answers `internal_error`). The broker runs each local
//! connection's request in a task of its own (`service`), so a client that hangs up does not
//! drop it; only the broker's shutdown does.
//!
//! **No late changes.** A local client gives up on an answer after `devagotchi_ipc::IO_TIMEOUT`
//! (5 s; `held_for` adds nothing for file requests), and the machine link forwards through
//! that same client. A mutation that has not got the gate within [`MUTATION_WAIT`] is refused
//! with `mutation_busy`, having changed nothing, so a queued mutation never lands after its
//! client has stopped waiting. A mutation that got the gate but whose own work outlasts the
//! client's deadline (a very large file, a slow disk) still lands: a client that saw
//! `ipc_timeout` reads the file again rather than assuming nothing changed.
//!
//! **Limits.** This orders file tools within one broker. It is not a filesystem-wide
//! compare-and-swap: external editors, commands the broker runs and a second broker do not take
//! the gate and can still race. Reads can see a move or append half way. There is no journal,
//! retry identity or rollback for file changes; whole-file replacement is still by rename;
//! shutdown and persistence faults treat file tools as before.

use super::*;
use crate::files::Scope;
use tokio::sync::OwnedMutexGuard;

/// The gate every file mutation of this broker takes, including path changes by move and
/// mkdir. External editors, other broker instances and executed commands do not take it.
pub(super) type MutationGate = Arc<tokio::sync::Mutex<()>>;

/// Longest a mutation waits for the gate before it is refused with `mutation_busy`: below the
/// local client's deadline, leaving it two seconds for the change itself and the answer.
pub(super) const MUTATION_WAIT: Duration =
    devagotchi_ipc::IO_TIMEOUT.saturating_sub(Duration::from_secs(2));
const _: () = assert!(MUTATION_WAIT.as_millis() > 0);

impl Broker {
    pub(super) async fn on_files(&self, request: RequestBody) -> BridgeResult<ResponseBody> {
        let gate = match mutation(&request) {
            Some(tool) => match self.mutation_gate(MUTATION_WAIT).await {
                Ok(gate) => Some(gate),
                Err(error) => {
                    trace::refused(tool, &error);
                    return Err(error);
                }
            },
            None => None,
        };
        self.on_files_with(gate, move |files| answer(files, request))
            .await
    }

    /// The mutation gate, or `mutation_busy` when another mutation still holds it after
    /// `within`. Nothing has been changed when this refuses.
    pub(super) async fn mutation_gate(
        &self,
        within: Duration,
    ) -> BridgeResult<OwnedMutexGuard<()>> {
        let gate = Arc::clone(&self.shared.file_mutations);
        timeout(within, gate.lock_owned()).await.map_err(|_| {
            BridgeError::new(
                "mutation_busy",
                format!(
                    "another file change on this machine was still running after {} s; nothing was changed, try again",
                    within.as_secs_f32()
                ),
            )
        })
    }

    /// `work` on the blocking pool. A `gate` moves into the worker before any path is
    /// resolved and is released only when its answer is ready, whatever happens to the caller.
    pub(super) async fn on_files_with<F>(
        &self,
        gate: Option<OwnedMutexGuard<()>>,
        work: F,
    ) -> BridgeResult<ResponseBody>
    where
        F: FnOnce(&Scope<'_>) -> (&'static str, BridgeResult<ResponseBody>) + Send + 'static,
    {
        let workspace = self.shared.workspace.clone();
        let policy = self.shared.config.exec_policy;
        let (what, done) = tokio::task::spawn_blocking(move || {
            let _gate = gate;
            let files = Scope {
                workspace: &workspace,
                policy,
            };
            work(&files)
        })
        .await
        .map_err(|error| BridgeError::new("internal_error", error.to_string()))?;
        if let Err(error) = &done {
            trace::refused(what, error);
        }
        done
    }
}

/// The tool name of a request that changes files, which takes the gate; `None` for a read.
fn mutation(request: &RequestBody) -> Option<&'static str> {
    match request {
        RequestBody::FileWrite(_) => Some("write_file"),
        RequestBody::FileEdit(_) => Some("edit_file"),
        RequestBody::FileMkdir(_) => Some("create_directory"),
        RequestBody::FileMove(_) => Some("move_file"),
        _ => None,
    }
}

/// The request's tool name, and its answer.
fn answer(files: &Scope<'_>, request: RequestBody) -> (&'static str, BridgeResult<ResponseBody>) {
    match request {
        RequestBody::FileRead(input) => ("read_file", files.read(input).map(ResponseBody::File)),
        RequestBody::FileList(input) => (
            "list_directory",
            files.list(input).map(ResponseBody::Listing),
        ),
        RequestBody::FileWrite(input) => {
            ("write_file", files.write(input).map(ResponseBody::Written))
        }
        RequestBody::FileEdit(input) => ("edit_file", files.edit(input).map(ResponseBody::Written)),
        RequestBody::FileInfo(input) => {
            ("get_file_info", files.info(input).map(ResponseBody::Info))
        }
        RequestBody::FileMkdir(input) => (
            "create_directory",
            files.mkdir(input).map(ResponseBody::Info),
        ),
        RequestBody::FileMove(input) => ("move_file", files.moved(input).map(ResponseBody::Info)),
        RequestBody::FileSearch(input) => (
            "search_files",
            files.search(input).map(ResponseBody::Search),
        ),
        _ => (
            "file request",
            Err(BridgeError::new("internal_error", "not a file request")),
        ),
    }
}
