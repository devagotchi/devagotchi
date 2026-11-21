//! Broker admission and the operation lifecycle: durable admissions,
//! idempotent retries, independent output cursors, explicit cancellation,
//! bounded shutdown, and restart recovery. Each admitted operation's process
//! is run by `crate::execution`. Executable grants are not a sandbox.

use crate::execution::platform::{self, SCOPE, SCOPE_TERMINAL};
use crate::execution::{ExecutionSpec, InputRequest, InputStep, StopReason, run_job};
use crate::journal::{Journal, JournalFrame, JournalOperation};
use crate::launch::{self, Os};
use crate::trace;
use access::{claim_writer, reach, reach_mut, visible};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use devagotchi_contracts::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{Notify, Semaphore, mpsc, oneshot, watch};
use tokio::time::{Instant, timeout};

const MAX_FRAMES: usize = 1024;
/// Waits held at once. A wait keeps its connection open until something changes, so they
/// are capped well under the broker's 32 connections; ordinary requests always get through.
const MAX_WAITERS: usize = 16;
const MAX_PAGE_FRAMES: usize = 64;
/// How long one client holds the keyboard of an interactive operation after its last input;
/// another client of the account waits it out or takes over.
const WRITER_LEASE_MS: u64 = 60_000;
/// Longest the broker waits for the program to take one input before answering
/// `input_blocked` (the program is not reading; the input stays queued).
const INPUT_WAIT: Duration = Duration::from_secs(5);
/// Input requests queued for one operation at once.
const INPUT_QUEUE: usize = 8;

/// What a broker will run. `granted` (the default): only the operator's
/// `allowed_executables`, with exact arguments, in the workspace. `account`: anything its
/// own OS account can run, including a `command` line for the machine's shell, in any
/// directory, with the account's environment; the account's permissions are the boundary.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecPolicy {
    #[default]
    Granted,
    Account,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrokerConfig {
    pub workspace: String,
    #[serde(default)]
    pub allowed_executables: Vec<String>,
    #[serde(default)]
    pub exec_policy: ExecPolicy,
    #[serde(default = "settings::default_output_limit")]
    pub output_limit_bytes: usize,
    #[serde(default = "settings::default_max_operations")]
    pub max_operations: usize,
    #[serde(default = "settings::default_max_concurrent")]
    pub max_concurrent: usize,
}

struct Operation {
    record: JournalOperation,
    authority: Option<remote::AuthorityClock>,
    retained_bytes: usize,
    active: bool,
    control: Option<watch::Sender<Option<StopReason>>>,
    persistence_fault: Option<String>,
    /// The running job's input, while an interactive operation can still take it.
    input: Option<mpsc::Sender<InputRequest>>,
    /// The client holding the keyboard, and until when (ms since the epoch).
    writer: Option<(String, u64)>,
    /// How the program was chosen (see `launch`), kept at the front of the final detail.
    launch: Option<String>,
    /// The root process, while it runs (not journaled: a recovered PID is never trusted).
    pid: Option<u32>,
}

struct State {
    journal: Journal,
    operations: HashMap<String, Operation>,
    keys: HashMap<String, String>,
    active_count: usize,
    closing: bool,
    persistence_fault: Option<BridgeError>,
    /// The limits `set_config_value` changes while the broker runs.
    limits: Limits,
}

pub(crate) struct Shared {
    config: BrokerConfig,
    /// The broker.json this broker was started from, where a changed limit is saved.
    config_file: Option<PathBuf>,
    workspace: PathBuf,
    executables: HashSet<PathBuf>,
    config_identity: String,
    state: Mutex<State>,
    settlement: Notify,
    /// Advances on every change to any operation (state, output, cancellation), so a wait
    /// sleeps until something happens instead of asking again and again.
    changes: watch::Sender<u64>,
    /// Advances when an operation is admitted or retired or changes state; output alone
    /// does not. A list (the inspector's) waits on it to re-read only when it changed.
    roster: watch::Sender<u64>,
    waiters: Semaphore,
    file_mutations: files::MutationGate,
}

pub struct Broker {
    shared: Arc<Shared>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

/// An operation ID the broker does not hold: never admitted here, or finished and retired
/// to keep history bounded.
fn not_found() -> BridgeError {
    BridgeError::new(
        "not_found",
        "No operation has this ID here. Finished operations are retired, oldest first, once the broker holds its configured max_operations.",
    )
}

fn invalid(message: impl Into<String>) -> BridgeError {
    BridgeError::new("invalid_request", message)
}

/// `value` resolved: `~` is this account's home; a relative path is under `base` when there is
/// one (the workspace, for a `cwd`) and refused otherwise (a setting in broker.json).
fn canonical_existing(
    value: &str,
    base: Option<&Path>,
    kind: &str,
    code: &str,
) -> BridgeResult<PathBuf> {
    let path = crate::files::given::absolute(value, base)
        .map_err(|error| BridgeError::new(code, format!("{kind}: {}", error.message)))?;
    // Windows canonical paths drop the verbatim prefix when that is lossless, so
    // child working directories and displayed paths stay ordinary drive paths.
    dunce::canonicalize(path)
        .map_err(|error| BridgeError::new(code, format!("Cannot canonicalize {kind}: {error}")))
}

fn path_string(path: &Path) -> BridgeResult<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| invalid("Canonical paths must be representable as UTF-8"))
}

/// Whether this broker would accept `path` as an executable grant or request target: a
/// regular file the platform runs directly (Linux: an execute bit; Windows: an `.exe` or
/// `.com` image). `devagotchi start` asks before it writes a grant, so a config it writes
/// is one `Broker::open` accepts.
pub fn is_directly_executable(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|metadata| platform::is_directly_executable(path, &metadata))
}

/// The platform's addition to "an executable regular file", for messages about a refused
/// grant.
pub const EXECUTABLE_RULE: &str = platform::EXECUTABLE_RULE;

impl Broker {
    pub fn open(config: BrokerConfig, state_dir: &Path) -> BridgeResult<Arc<Self>> {
        Self::open_at(config, state_dir, None)
    }

    /// `open`, for a broker started from `config_file`: `set_config_value` saves there.
    pub fn open_at(
        config: BrokerConfig,
        state_dir: &Path,
        config_file: Option<PathBuf>,
    ) -> BridgeResult<Arc<Self>> {
        let limits = Limits::from_config(&config)?;
        if config.allowed_executables.len() > 128 {
            return Err(BridgeError::new(
                "invalid_config",
                "At most 128 executable grants",
            ));
        }
        let workspace = canonical_existing(&config.workspace, None, "workspace", "invalid_config")?;
        if !workspace.is_dir() {
            return Err(BridgeError::new(
                "invalid_config",
                "Workspace must be an existing directory",
            ));
        }
        let mut executables = HashSet::new();
        for grant in &config.allowed_executables {
            let executable = canonical_existing(grant, None, "executable grant", "invalid_config")?;
            let metadata = executable
                .metadata()
                .map_err(|error| BridgeError::new("invalid_config", error.to_string()))?;
            if !platform::is_directly_executable(&executable, &metadata) {
                return Err(BridgeError::new(
                    "invalid_config",
                    format!(
                        "Each executable grant must resolve to an executable regular file{}",
                        platform::EXECUTABLE_RULE
                    ),
                ));
            }
            executables.insert(executable);
        }
        // A retry key's broker half: where (the workspace) and as whom (the exec policy). The
        // limits and the grant list are left out, so tuning them never turns a retry into a
        // conflict; normalize() checks the grant and cwd live on every start, replays included.
        let identity_bytes = serde_json::to_vec(&(
            "machine-engine-execution",
            path_string(&workspace)?,
            config.exec_policy,
        ))
        .map_err(|error| BridgeError::new("invalid_config", error.to_string()))?;
        let config_identity = hex::encode(Sha256::digest(identity_bytes));
        let (journal, operations, keys) = recover::recover(state_dir, limits.output_limit_bytes)?;
        Ok(Arc::new(Self {
            shared: Arc::new(Shared {
                config,
                config_file,
                workspace,
                executables,
                config_identity,
                state: Mutex::new(State {
                    journal,
                    operations,
                    keys,
                    active_count: 0,
                    closing: false,
                    persistence_fault: None,
                    limits,
                }),
                settlement: Notify::new(),
                changes: watch::channel(0).0,
                roster: watch::channel(0).0,
                waiters: Semaphore::new(MAX_WAITERS),
                file_mutations: Default::default(),
            }),
        }))
    }

    pub async fn handle(&self, request: RequestBody) -> BridgeResult<ResponseBody> {
        match request {
            RequestBody::OperationStart(input) => {
                let asked = trace::asked(&input);
                let started = self.start(input);
                match &started {
                    Err(error) => trace::refused(&format!("operation_start ({asked})"), error),
                    Ok(output) if !output.replayed => {
                        let operation = &output.operation;
                        trace::line(
                            &format!("admitted {}", operation.operation_id),
                            &format!(
                                "{} {:?} in {} ({asked}). {}",
                                operation.executable,
                                operation.args,
                                operation.cwd,
                                operation.detail.as_deref().unwrap_or_default()
                            ),
                        );
                    }
                    Ok(_) => {}
                }
                started.map(ResponseBody::Start)
            }
            RequestBody::OperationGet(input) => self
                .get(&input.operation_id, input.caller.as_ref())
                .map(ResponseBody::Operation),
            RequestBody::OperationRead(input) => self.read(input).map(ResponseBody::Output),
            RequestBody::OperationCancel(input) => self
                .cancel(&input.operation_id, input.caller.as_ref())
                .map(ResponseBody::Operation),
            RequestBody::OperationList(input) => self.list(input).map(ResponseBody::Operations),
            RequestBody::OperationWait(input) => {
                self.wait(input).await.map(ResponseBody::Operation)
            }
            RequestBody::OperationsChanged(input) => self
                .operations_changed(input)
                .await
                .map(ResponseBody::Revision),
            RequestBody::OperationInput(input) => {
                self.input(input).await.map(ResponseBody::InputSent)
            }
            RequestBody::OperationResize(input) => {
                self.resize(input).await.map(ResponseBody::Operation)
            }
            request @ (RequestBody::FileRead(_)
            | RequestBody::FileList(_)
            | RequestBody::FileWrite(_)
            | RequestBody::FileEdit(_)
            | RequestBody::FileInfo(_)
            | RequestBody::FileMkdir(_)
            | RequestBody::FileMove(_)
            | RequestBody::FileSearch(_)) => self.on_files(request).await,
            request @ (RequestBody::ConfigGet(_)
            | RequestBody::ConfigSet(_)
            | RequestBody::ProcessList(_)
            | RequestBody::ProcessKill(_)
            | RequestBody::UrlRead(_)
            | RequestBody::BrokerCapabilities(_)) => self.on_system(request).await,
        }
    }

    /// Answer when the operation settles (or, with `output_after`, when output past that
    /// sequence exists), or at `timeout_ms` unchanged. It sleeps on the broker's change
    /// signal: nothing is re-read until something changed.
    async fn wait(&self, input: WaitInput) -> BridgeResult<OperationView> {
        let id = validate_id(&input.operation_id)?;
        if !(1..=MAX_WAIT_MS).contains(&input.timeout_ms) {
            return Err(invalid(format!("timeout_ms must be 1..={MAX_WAIT_MS}")));
        }
        let Ok(_waiting) = self.shared.waiters.try_acquire() else {
            return Err(BridgeError::new(
                "wait_busy",
                format!(
                    "this broker is already holding {MAX_WAITERS} waits; use operation_get, or wait again shortly"
                ),
            ));
        };
        let mut changes = self.shared.changes.subscribe();
        let until = Instant::now() + Duration::from_millis(input.timeout_ms);
        loop {
            changes.borrow_and_update();
            {
                let state = self.shared.lock();
                let operation = reach(&state.operations, &id, input.caller.as_ref())?;
                let record = &operation.record;
                let settled = record.view.state.terminal() && !operation.active;
                let output = input
                    .output_after
                    .is_some_and(|after| record.latest_sequence > after);
                if settled || output {
                    return Ok(record.view.clone());
                }
            }
            match tokio::time::timeout_at(until, changes.changed()).await {
                Ok(Ok(())) => {}
                // The deadline, or a broker going away: answer with what is there now.
                _ => return self.get(&id, input.caller.as_ref()),
            }
        }
    }

    /// Answer when the operations revision differs from `after_revision`, or at
    /// `timeout_ms` with it unchanged.
    async fn operations_changed(&self, input: ChangesInput) -> BridgeResult<Revision> {
        if !(1..=MAX_WAIT_MS).contains(&input.timeout_ms) {
            return Err(invalid(format!("timeout_ms must be 1..={MAX_WAIT_MS}")));
        }
        let mut roster = self.shared.roster.subscribe();
        let current = *roster.borrow_and_update();
        if input.after_revision != Some(current) {
            return Ok(Revision { revision: current });
        }
        let Ok(_waiting) = self.shared.waiters.try_acquire() else {
            return Err(BridgeError::new(
                "wait_busy",
                format!("this broker is already holding {MAX_WAITERS} waits; wait again shortly"),
            ));
        };
        let _ = timeout(Duration::from_millis(input.timeout_ms), roster.changed()).await;
        Ok(Revision {
            revision: *roster.borrow(),
        })
    }

    fn get(&self, id: &str, caller: Option<&Caller>) -> BridgeResult<OperationView> {
        let id = validate_id(id)?;
        let state = self.shared.lock();
        Ok(reach(&state.operations, &id, caller)?.record.view.clone())
    }

    fn list(&self, input: ListInput) -> BridgeResult<OperationsPage> {
        if !(1..=MAX_LIST_LIMIT).contains(&input.limit) {
            return Err(invalid(format!("limit must be 1..={MAX_LIST_LIMIT}")));
        }
        let scope = input.scope.unwrap_or_default();
        let state = self.shared.lock();
        let mut operations: Vec<_> = state
            .operations
            .values()
            .filter(|operation| {
                visible(
                    operation.record.owner.as_ref(),
                    input.caller.as_ref(),
                    scope,
                )
            })
            .map(|operation| &operation.record.view)
            .collect();
        operations.sort_by(|left, right| {
            right
                .created_at_ms
                .cmp(&left.created_at_ms)
                .then_with(|| right.operation_id.cmp(&left.operation_id))
        });
        let offset = usize::try_from(input.offset)
            .unwrap_or(usize::MAX)
            .min(operations.len());
        let end = offset
            .saturating_add(input.limit as usize)
            .min(operations.len());
        let summaries = operations[offset..end]
            .iter()
            .map(|operation| {
                // JSON quoting makes control characters and argument boundaries visible;
                // this preview is plain text and is never parsed back into a command.
                let mut display = std::iter::once(&operation.executable)
                    .chain(operation.args.iter())
                    .map(|part| serde_json::to_string(part).expect("Strings are serializable"))
                    .collect::<Vec<_>>()
                    .join(" ");
                if display.len() > 512 {
                    let mut boundary = 509;
                    while !display.is_char_boundary(boundary) {
                        boundary -= 1;
                    }
                    display.truncate(boundary);
                    display.push('…');
                }
                OperationSummary {
                    operation_id: operation.operation_id.clone(),
                    state: operation.state.clone(),
                    created_at_ms: operation.created_at_ms,
                    updated_at_ms: operation.updated_at_ms,
                    display_command: display,
                    interactive: operation
                        .interactive
                        .as_ref()
                        .map(|interactive| interactive.mode),
                }
            })
            .collect();
        Ok(OperationsPage {
            operations: summaries,
            next_offset: (end < operations.len()).then_some(end as u64),
        })
    }

    /// Type into an interactive operation: text, then named keys, delivered in order once
    /// the caller holds the keyboard (see `claim_writer`).
    async fn input(&self, input: SendInput) -> BridgeResult<InputSent> {
        let id = validate_id(&input.operation_id)?;
        let text = input.text.unwrap_or_default();
        if text.len() > MAX_INPUT_TEXT_BYTES || input.keys.len() > MAX_INPUT_KEYS {
            return Err(invalid(format!(
                "text is at most {MAX_INPUT_TEXT_BYTES} bytes and keys at most {MAX_INPUT_KEYS}"
            )));
        }
        if text.is_empty() && input.keys.is_empty() {
            return Err(invalid("Give text, keys, or both"));
        }
        let caller = input.caller.ok_or_else(no_caller)?;
        let (sender, steps, cursor) = {
            let mut state = self.shared.lock();
            let operation = reach_mut(&mut state.operations, &id, Some(&caller))?;
            let mode = claim_writer(operation, &caller, input.take_over)?;
            let steps = input_steps(mode, &text, &input.keys)?;
            let sender = operation.input.clone().ok_or_else(input_closed)?;
            (sender, steps, operation.record.latest_sequence)
        };
        self.shared.output_changed();
        let closes = steps.contains(&InputStep::Close);
        deliver(&sender, steps).await?;
        if closes {
            self.shared.input_closed(&id);
        }
        Ok(InputSent {
            operation: self.get(&id, Some(&caller))?,
            cursor,
        })
    }

    /// Resize an interactive operation's terminal; its program gets SIGWINCH.
    async fn resize(&self, input: ResizeInput) -> BridgeResult<OperationView> {
        let id = validate_id(&input.operation_id)?;
        check_size(input.cols, input.rows)?;
        let caller = input.caller.ok_or_else(no_caller)?;
        let sender = {
            let mut state = self.shared.lock();
            let operation = reach_mut(&mut state.operations, &id, Some(&caller))?;
            if claim_writer(operation, &caller, input.take_over)? != InputMode::Terminal {
                return Err(BridgeError::new(
                    "no_terminal",
                    "This operation reads a pipe, not a terminal; there is nothing to resize",
                ));
            }
            operation.input.clone().ok_or_else(input_closed)?
        };
        deliver(
            &sender,
            vec![InputStep::Resize {
                cols: input.cols,
                rows: input.rows,
            }],
        )
        .await?;
        let mut state = self.shared.lock();
        let operation = reach_mut(&mut state.operations, &id, Some(&caller))?;
        if let Some(interactive) = &mut operation.record.view.interactive {
            interactive.cols = input.cols;
            interactive.rows = input.rows;
        }
        operation.record.view.updated_at_ms = now_ms();
        let view = operation.record.view.clone();
        if let Err(error) = state.journal.update(&view) {
            state.fault(error.clone());
            return Err(error);
        }
        drop(state);
        self.shared.output_changed();
        Ok(view)
    }

    fn cancel(&self, id: &str, caller: Option<&Caller>) -> BridgeResult<OperationView> {
        let id = validate_id(id)?;
        let mut state = self.shared.lock();
        let operation = reach_mut(&mut state.operations, &id, caller)?;
        if !operation.active {
            return Ok(operation.record.view.clone());
        }
        operation.record.view.cancellation_requested = true;
        operation.record.view.updated_at_ms = now_ms();
        let view = operation.record.view.clone();
        let control = operation.control.clone();
        if let Err(error) = state.journal.update(&view) {
            state.fault(error.clone());
            return Err(error);
        }
        drop(state);
        self.shared.changed();
        if let Some(control) = control {
            let _ = control.send(Some(StopReason::Cancelled));
        }
        Ok(view)
    }

    pub async fn shutdown(&self) {
        {
            let mut state = self.shared.lock();
            state.closing = true;
            let ids: Vec<_> = state
                .operations
                .iter()
                .filter(|(_, operation)| operation.active)
                .map(|(id, _)| id.clone())
                .collect();
            for id in ids {
                let operation = state
                    .operations
                    .get_mut(&id)
                    .expect("ID came from this locked registry");
                operation.record.view.cancellation_requested = true;
                operation.record.view.updated_at_ms = now_ms();
                let view = operation.record.view.clone();
                let control = operation.control.clone();
                if let Err(error) = state.journal.update(&view) {
                    state.fault(error);
                }
                if let Some(control) = control {
                    let _ = control.send(Some(StopReason::Shutdown));
                }
            }
        }
        self.shared.changed();
        let settled = async {
            loop {
                let notified = self.shared.settlement.notified();
                if self.shared.lock().active_count == 0 {
                    return;
                }
                notified.await;
            }
        };
        if timeout(Duration::from_secs(8), settled).await.is_err() {
            let mut state = self.shared.lock();
            let ids: Vec<_> = state
                .operations
                .iter()
                .filter(|(_, operation)| operation.active)
                .map(|(id, _)| id.clone())
                .collect();
            for id in ids {
                let operation = state
                    .operations
                    .get_mut(&id)
                    .expect("ID came from this locked registry");
                operation.record.view.state = OperationState::Unknown;
                operation.record.view.output_complete = false;
                operation.record.view.updated_at_ms = now_ms();
                operation.record.view.detail = Some("Graceful shutdown settlement exceeded its bound; process effects and resource status remain unknown.".into());
                let view = operation.record.view.clone();
                if let Err(error) = state.journal.update(&view) {
                    state.fault(error);
                }
            }
            drop(state);
            self.shared.changed();
            eprintln!(
                "Broker shutdown: one or more owned jobs did not settle within eight seconds; state is unknown."
            );
        }
    }
}

/// The launch sentence, then the detail.
fn with_launch(launch: Option<&str>, detail: &str) -> String {
    match launch {
        Some(launch) => format!("Launch: {launch}. {detail}"),
        None => detail.to_owned(),
    }
}

fn validate_id(id: &str) -> BridgeResult<String> {
    uuid::Uuid::parse_str(id)
        .map(|id| id.to_string())
        .map_err(|_| invalid("operation_id must be a valid UUID"))
}

fn check_size(cols: u16, rows: u16) -> BridgeResult<()> {
    if !(2..=500).contains(&cols) || !(1..=200).contains(&rows) {
        return Err(invalid("cols must be 2..=500 and rows 1..=200"));
    }
    Ok(())
}

impl State {
    fn fault(&mut self, error: BridgeError) {
        if self.persistence_fault.is_none() {
            eprintln!("Broker admission blocked: {error}");
            self.persistence_fault = Some(error.clone());
        }
        for operation in self
            .operations
            .values_mut()
            .filter(|operation| operation.active)
        {
            operation.persistence_fault = Some(error.to_string());
            operation.record.view.state = OperationState::Unknown;
            operation.record.view.updated_at_ms = now_ms();
            operation.record.view.output_complete = false;
            operation.record.view.cancellation_requested = true;
            operation.record.view.detail = Some(format!(
                "Critical persistence failure: {error}. Further starts are blocked; cancellation has been requested. Durable effects and execution status cannot be certified."
            ));
            if let Some(control) = &operation.control {
                let _ = control.send(Some(StopReason::PersistenceFailure));
            }
        }
    }
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                let mut guard = poisoned.into_inner();
                guard.fault(BridgeError::new("persistence_error", "Internal registry was poisoned; admission is closed and execution state is unknown"));
                guard
            }
        }
    }

    /// Wake every wait: something about an operation changed.
    /// An operation was admitted or retired or changed state.
    pub(crate) fn changed(&self) {
        self.changes
            .send_modify(|revision| *revision = revision.wrapping_add(1));
        self.roster
            .send_modify(|revision| *revision = revision.wrapping_add(1));
    }

    /// An interactive operation takes no more input: it is ending, or its pipe was closed.
    pub(crate) fn input_closed(&self, id: &str) {
        let mut state = self.lock();
        let Some(operation) = state.operations.get_mut(id) else {
            return;
        };
        operation.input = None;
        operation.writer = None;
        let Some(interactive) = &mut operation.record.view.interactive else {
            return;
        };
        if !interactive.input_open {
            return;
        }
        interactive.input_open = false;
        interactive.writer_until_ms = None;
        operation.record.view.updated_at_ms = now_ms();
        let view = operation.record.view.clone();
        if let Err(error) = state.journal.update(&view) {
            state.fault(error);
        }
        drop(state);
        self.output_changed();
    }

    /// An operation has new output.
    fn output_changed(&self) {
        self.changes
            .send_modify(|revision| *revision = revision.wrapping_add(1));
    }

    pub(crate) fn running(&self, id: &str, pid: Option<u32>) {
        let mut state = self.lock();
        let Some(operation) = state.operations.get_mut(id) else {
            return;
        };
        operation.pid = pid;
        if operation.persistence_fault.is_some() {
            return;
        }
        operation.record.view.state = OperationState::Running;
        operation.record.view.updated_at_ms = now_ms();
        operation.record.view.detail = Some(platform::running_detail(
            self.config.exec_policy == ExecPolicy::Account,
            operation
                .record
                .view
                .interactive
                .as_ref()
                .map(|interactive| interactive.mode),
        ));
        let view = operation.record.view.clone();
        if let Err(error) = state.journal.update(&view) {
            state.fault(error);
        }
        drop(state);
        self.changed();
    }

    pub(crate) fn mark_stop(&self, id: &str) {
        let mut state = self.lock();
        let Some(operation) = state.operations.get_mut(id) else {
            return;
        };
        if operation.record.view.cancellation_requested {
            return;
        }
        operation.record.view.cancellation_requested = true;
        operation.record.view.updated_at_ms = now_ms();
        let view = operation.record.view.clone();
        if let Err(error) = state.journal.update(&view) {
            state.fault(error);
        }
        drop(state);
        self.changed();
    }

    pub(crate) fn output(&self, id: &str, stream: &str, bytes: Vec<u8>, text: String) {
        if bytes.is_empty() && text.is_empty() {
            return;
        }
        let mut state = self.lock();
        let output_limit = state.limits.output_limit_bytes;
        let Some(operation) = state.operations.get_mut(id) else {
            return;
        };
        let sequence = operation.record.latest_sequence + 1;
        let frame = JournalFrame {
            sequence,
            stream: stream.into(),
            bytes,
            text,
        };
        operation.record.latest_sequence = sequence;
        operation.retained_bytes += frame.bytes.len();
        operation.record.frames.push_back(frame.clone());
        while operation.retained_bytes > output_limit || operation.record.frames.len() > MAX_FRAMES
        {
            if let Some(dropped) = operation.record.frames.pop_front() {
                operation.retained_bytes -= dropped.bytes.len();
            }
        }
        let earliest = operation
            .record
            .frames
            .front()
            .map_or(sequence + 1, |frame| frame.sequence);
        if state.persistence_fault.is_none() {
            if let Err(error) = state.journal.append(id, &frame, earliest) {
                state.fault(error);
            }
        }
        drop(state);
        self.output_changed();
    }

    pub(crate) fn finish(
        &self,
        id: &str,
        final_state: OperationState,
        exit_code: Option<i32>,
        output_complete: bool,
        detail: String,
    ) {
        let mut state = self.lock();
        let Some(operation) = state.operations.get_mut(id) else {
            return;
        };
        if !operation.active {
            return;
        }
        operation.record.view.state = if operation.persistence_fault.is_some() {
            OperationState::Unknown
        } else {
            final_state
        };
        operation.record.view.updated_at_ms = now_ms();
        operation.record.view.exit_code = exit_code;
        operation.record.view.output_complete =
            output_complete && operation.persistence_fault.is_none();
        if let Some(interactive) = &mut operation.record.view.interactive {
            interactive.input_open = false;
            interactive.writer_until_ms = None;
        }
        let detail = with_launch(operation.launch.as_deref(), &detail);
        operation.record.view.detail = Some(match &operation.persistence_fault {
            Some(error) => format!(
                "{detail} Critical persistence failure remains visible: {error}; durable completion cannot be certified."
            ),
            None => detail,
        });
        let view = operation.record.view.clone();
        trace::line(
            &format!("settled {}", &view.operation_id),
            &format!(
                "{:?} exit {}: {}",
                view.state,
                view.exit_code
                    .map_or_else(|| "none".to_owned(), |code| code.to_string()),
                view.detail.as_deref().unwrap_or_default()
            ),
        );
        if let Err(error) = state.journal.update(&view) {
            state.fault(error);
        }
        let operation = state
            .operations
            .get_mut(id)
            .expect("Operation remains in this locked registry");
        operation.active = false;
        operation.authority = None;
        operation.control = None;
        operation.input = None;
        operation.writer = None;
        operation.pid = None;
        state.active_count = state.active_count.saturating_sub(1);
        drop(state);
        self.settlement.notify_waiters();
        self.changed();
    }
}

#[cfg(test)]
mod tests;

mod access;
mod files;
mod history;
mod input;
mod normalize;
mod output;
mod recover;
mod remote;
mod settings;
mod start;
mod system;
use input::{deliver, input_closed, input_steps, no_caller};
use settings::Limits;
