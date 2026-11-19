//! Process tools protect the broker and owned operations; only the spawned fixture is killed.
use super::lifecycle::account_broker;
use super::*;

#[tokio::test]
async fn process_tools_refuse_the_broker_and_missing_pids() {
    let directory = tempfile::tempdir().unwrap();
    let broker = account_broker(&directory);
    for (pid, code) in [
        (std::process::id(), "broker_process"),
        (u32::MAX, "process_not_found"),
    ] {
        let error = broker
            .handle(RequestBody::ProcessKill(ProcessKillInput {
                pid,
                force: false,
            }))
            .await
            .unwrap_err();
        assert_eq!(error.code, code);
    }
    broker.shutdown().await;
}

#[tokio::test]
async fn process_tools_list_and_stop_only_the_spawned_child() {
    let directory = tempfile::tempdir().unwrap();
    let broker = account_broker(&directory);
    #[cfg(unix)]
    let mut command = std::process::Command::new("sleep");
    #[cfg(unix)]
    command.arg("60");
    #[cfg(windows)]
    let mut command = std::process::Command::new("ping.exe");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command
            .args(["-n", "60", "127.0.0.1"])
            .creation_flags(0x08000000);
    }
    let mut child = command
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let pid = child.id();
    let result = async {
        let ResponseBody::Processes(page) = broker
            .handle(RequestBody::ProcessList(ProcessListInput {
                filter: Some(if cfg!(windows) { "ping" } else { "sleep" }.into()),
                max_results: MAX_PROCESS_RESULTS,
            }))
            .await
            .unwrap()
        else {
            panic!("process page");
        };
        assert!(page.processes.iter().any(|p| p.pid == pid));
        let ResponseBody::Killed(killed) = broker
            .handle(RequestBody::ProcessKill(ProcessKillInput {
                pid,
                force: false,
            }))
            .await
            .unwrap()
        else {
            panic!("kill result");
        };
        assert!(killed.exited);
        assert_eq!(
            killed.signal,
            if cfg!(windows) { "kill" } else { "terminate" }
        );
    }
    .await;
    // This fixture is ours even when the assertion above fails (CI process isolation).
    let _ = child.kill();
    child.wait().unwrap();
    broker.shutdown().await;
    result
}
