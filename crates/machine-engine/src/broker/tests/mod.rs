use super::*;
use tokio::time::sleep;

/// An executable that exits successfully without arguments or input.
fn test_executable() -> PathBuf {
    #[cfg(unix)]
    let executable = dunce::canonicalize("/usr/bin/true").unwrap();
    #[cfg(windows)]
    let executable = system32("hostname.exe");
    executable
}

#[cfg(windows)]
fn system32(name: &str) -> PathBuf {
    let root = std::env::var_os("SystemRoot").expect("SystemRoot is set on Windows");
    dunce::canonicalize(Path::new(&root).join("System32").join(name)).unwrap()
}

/// The platform shell and the argv that makes it run `script`.
fn shell(script: &[&str]) -> (PathBuf, Vec<String>) {
    #[cfg(unix)]
    let (shell, mut args) = (
        dunce::canonicalize("/bin/sh").unwrap(),
        vec!["-c".to_owned()],
    );
    #[cfg(windows)]
    let (shell, mut args) = (system32("cmd.exe"), vec!["/d".to_owned(), "/c".to_owned()]);
    #[cfg(unix)]
    args.push(script.join(" "));
    #[cfg(windows)]
    args.extend(script.iter().map(|part| (*part).to_owned()));
    (shell, args)
}

fn open_broker(directory: &tempfile::TempDir, grants: &[&Path]) -> Arc<Broker> {
    let config = BrokerConfig {
        workspace: directory.path().display().to_string(),
        allowed_executables: grants
            .iter()
            .map(|grant| grant.display().to_string())
            .collect(),
        output_limit_bytes: 65_536,
        max_operations: 8,
        max_concurrent: 4,
        exec_policy: ExecPolicy::Granted,
    };
    Broker::open(
        config,
        &devagotchi_ipc::private_test_state(directory.path()),
    )
    .unwrap()
}

/// A granted broker's config holding `max` operations, one running at a time.
fn limited_config(directory: &tempfile::TempDir, grants: &[&Path], max: usize) -> BrokerConfig {
    BrokerConfig {
        workspace: directory.path().display().to_string(),
        allowed_executables: grants.iter().map(|g| g.display().to_string()).collect(),
        output_limit_bytes: 8192,
        max_operations: max,
        max_concurrent: 1,
        exec_policy: ExecPolicy::Granted,
    }
}

fn input(key: &str, executable: &Path, args: Vec<String>) -> StartInput {
    StartInput {
        idempotency_key: key.into(),
        executable: executable.display().to_string(),
        args,
        command: None,
        cwd: None,
        deadline_ms: 5_000,
        interactive: None,
        combined_output: false,
        caller: None,
    }
}

fn start(
    broker: &Broker,
    key: &str,
    executable: &Path,
    args: Vec<String>,
    deadline_ms: u64,
) -> String {
    broker
        .start(StartInput {
            idempotency_key: key.into(),
            executable: executable.display().to_string(),
            args,
            command: None,
            cwd: None,
            deadline_ms,
            interactive: None,
            combined_output: false,
            caller: None,
        })
        .unwrap()
        .operation
        .operation_id
}

async fn settle(broker: &Broker, id: &str) -> OperationView {
    settle_within(broker, id, 20).await
}

/// Windows PowerShell's first start on a fresh CI image can take tens of seconds.
/// Every test that settles an operation does it with the broker's own wait.
async fn settle_within(broker: &Broker, id: &str, seconds: u64) -> OperationView {
    let deadline = Instant::now() + Duration::from_secs(seconds);
    loop {
        let left = deadline
            .saturating_duration_since(Instant::now())
            .as_millis() as u64;
        let view = broker
            .wait(WaitInput {
                operation_id: id.into(),
                output_after: None,
                timeout_ms: left.clamp(1, MAX_WAIT_MS),
                caller: None,
            })
            .await
            .unwrap();
        if view.state.terminal() {
            return view;
        }
        assert!(
            Instant::now() < deadline,
            "operation did not settle: {view:?}"
        );
    }
}

fn wait_for(id: &str, output_after: Option<u64>, timeout_ms: u64) -> WaitInput {
    WaitInput {
        operation_id: id.into(),
        output_after,
        timeout_ms,
        caller: None,
    }
}

/// A shell line that prints `first`, pauses about three seconds, then prints `second`.
fn pause_between_lines() -> (PathBuf, Vec<String>) {
    #[cfg(unix)]
    return shell(&["echo first;", "sleep 3;", "echo second"]);
    // By full path: the operation's environment is cleared, so cmd.exe has no PATH.
    #[cfg(windows)]
    return shell(&[
        "echo",
        "first&",
        &system32("PING.EXE").display().to_string(),
        "-n",
        "4",
        "127.0.0.1",
        ">nul&",
        "echo",
        "second",
    ]);
}

fn output_text(broker: &Broker, id: &str) -> String {
    let page = broker
        .read(ReadInput {
            operation_id: id.into(),
            cursor: 0,
            max_bytes: MAX_OUTPUT_PAGE_BYTES,
            caller: None,
        })
        .unwrap();
    page.frames
        .iter()
        .map(|frame| frame.text.as_str())
        .collect()
}

mod access;
mod file_mutations;
mod interactive;
mod lifecycle;
mod retry;
mod waits;
