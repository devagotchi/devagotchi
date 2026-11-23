//! Local file requests are answered directly. Account-backed remote file calls use the
//! durable admission/result journal in `remote::files` and this same IO owner. The broker's
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
//! connection's request in a task of its own (`service`), so neither a client that hangs up
//! nor an MCP cancellation drops it; only the broker's shutdown does.
//!
//! **No change after the IPC deadline.** A local client gives up on an answer after
//! `devagotchi_ipc::IO_TIMEOUT` (5 s; `held_for` adds nothing for file requests) and reports
//! `ipc_timeout`; the machine link forwards through that same client. A mutation that has not
//! got the gate within [`MUTATION_WAIT`] is refused with `mutation_busy`, having changed
//! nothing, so a queued mutation never lands after that deadline. A mutation that got the gate
//! but whose own work outlasts the deadline (a very large file, a slow disk) still lands: a
//! client that saw `ipc_timeout` reads the file again rather than assuming nothing changed.
//!
//! **MCP cancellation does not withdraw a mutation.** An MCP client that cancels a call
//! (through `devagotchi mcp` or a hub) gets `request_cancelled`, which only stops that
//! adapter's wait for the answer; the request already sent to the broker goes on. A mutation
//! cancelled while it was queued still gets the gate if it frees within [`MUTATION_WAIT`], and
//! then lands after its client stopped waiting, with no one to tell. After
//! `request_cancelled`, read the file again before assuming the change did not happen.
//!
//! **Queue.** At most [`MAX_QUEUED_MUTATIONS`] mutations wait for the gate at once; one more is
//! refused with `mutation_busy` at once. Each queued mutation holds one of the broker's 32 local
//! connections for up to [`MUTATION_WAIT`]: the running one, the queue and the 16 held
//! `operation_wait`s together stay under that, so ordinary requests still get a connection.
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
/// mkdir, and the bound on how many wait for it. External editors, other broker instances and
/// executed commands do not take it.
pub(super) struct MutationGate {
    pub(super) gate: Arc<tokio::sync::Mutex<()>>,
    /// One permit per mutation waiting for `gate`; the holder has given its permit back.
    pub(super) queue: Semaphore,
}

impl Default for MutationGate {
    fn default() -> Self {
        Self {
            gate: Arc::default(),
            queue: Semaphore::new(MAX_QUEUED_MUTATIONS),
        }
    }
}

/// Longest a mutation waits for the gate before it is refused with `mutation_busy`: below the
/// local client's deadline, leaving it two seconds for the change itself and the answer.
pub(super) const MUTATION_WAIT: Duration =
    devagotchi_ipc::IO_TIMEOUT.saturating_sub(Duration::from_secs(2));
const _: () = assert!(MUTATION_WAIT.as_millis() > 0);

/// Mutations waiting for the gate at once. With the running one and the broker's
/// `MAX_WAITERS` (16) held waits, they stay well under its 32 local connections.
pub(super) const MAX_QUEUED_MUTATIONS: usize = 8;
const _: () = assert!(MAX_QUEUED_MUTATIONS + 1 + MAX_WAITERS < 32);

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

    /// The mutation gate, or `mutation_busy` when [`MAX_QUEUED_MUTATIONS`] already wait for it
    /// (at once) or another mutation still holds it after `within`. Nothing has been changed
    /// when this refuses.
    pub(super) async fn mutation_gate(
        &self,
        within: Duration,
    ) -> BridgeResult<OwnedMutexGuard<()>> {
        let mutations = &self.shared.file_mutations;
        let Ok(_queued) = mutations.queue.try_acquire() else {
            return Err(BridgeError::new(
                "mutation_busy",
                format!(
                    "{MAX_QUEUED_MUTATIONS} file changes are already waiting on this machine; nothing was changed, try again"
                ),
            ));
        };
        let gate = Arc::clone(&mutations.gate);
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
                authority: None,
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

/// The tool name of a request that changes files, which takes the gate; `None` for a read or
/// any other request. Every variant is named, so a new request must decide here.
fn mutation(request: &RequestBody) -> Option<&'static str> {
    match request {
        RequestBody::FileWrite(_) => Some("write_file"),
        RequestBody::FileEdit(_) => Some("edit_file"),
        RequestBody::FileMkdir(_) => Some("create_directory"),
        RequestBody::FileMove(_) => Some("move_file"),
        RequestBody::FileDelete(_) => Some("delete_file"),
        RequestBody::FileRead(_)
        | RequestBody::FileList(_)
        | RequestBody::FileInfo(_)
        | RequestBody::FileSearch(_) => None,
        RequestBody::OperationStart(_)
        | RequestBody::OperationGet(_)
        | RequestBody::OperationRead(_)
        | RequestBody::OperationCancel(_)
        | RequestBody::OperationList(_)
        | RequestBody::OperationWait(_)
        | RequestBody::OperationsChanged(_)
        | RequestBody::OperationInput(_)
        | RequestBody::OperationResize(_)
        | RequestBody::ConfigGet(_)
        | RequestBody::ConfigSet(_)
        | RequestBody::BrokerCapabilities(_)
        | RequestBody::ProcessList(_)
        | RequestBody::ProcessKill(_)
        | RequestBody::UrlRead(_) => None,
    }
}

/// The request's tool name, and its answer.
pub(super) fn answer(
    files: &Scope<'_>,
    request: RequestBody,
) -> (&'static str, BridgeResult<ResponseBody>) {
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
        RequestBody::FileDelete(input) => (
            "delete_file",
            files.delete(input).map(ResponseBody::Deleted),
        ),
        RequestBody::FileSearch(input) => (
            "search_files",
            files.search(input).map(ResponseBody::Search),
        ),
        RequestBody::OperationStart(_)
        | RequestBody::OperationGet(_)
        | RequestBody::OperationRead(_)
        | RequestBody::OperationCancel(_)
        | RequestBody::OperationList(_)
        | RequestBody::OperationWait(_)
        | RequestBody::OperationsChanged(_)
        | RequestBody::ConfigGet(_)
        | RequestBody::ConfigSet(_)
        | RequestBody::ProcessList(_)
        | RequestBody::ProcessKill(_)
        | RequestBody::UrlRead(_)
        | RequestBody::OperationInput(_)
        | RequestBody::OperationResize(_)
        | RequestBody::BrokerCapabilities(_) => (
            "file request",
            Err(BridgeError::new("internal_error", "not a file request")),
        ),
    }
}
