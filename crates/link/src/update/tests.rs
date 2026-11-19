//! The updater's own logic: the idle wait against a fake broker, the hub's checksum list,
//! and what a restarted link reports from the record the previous one left.

use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use devagotchi_contracts::{UPDATE_DONE, UPDATE_FAILED};

use super::{handoff::outcome, idle::wait_for_idle};

#[tokio::test]
async fn the_wait_ends_when_the_fake_broker_runs_no_jobs() {
    // Busy twice, then idle: two "waiting_for_jobs" reports, then on.
    let calls = Arc::new(AtomicUsize::new(0));
    let mut waited = 0;
    let fake = {
        let calls = calls.clone();
        move || {
            let n = calls.fetch_add(1, Ordering::SeqCst);
            async move { Ok(if n < 2 { 3 } else { 0 }) }
        }
    };
    let result = wait_for_idle(
        fake,
        Duration::from_millis(5),
        Duration::from_secs(10),
        || waited += 1,
    )
    .await;
    assert_eq!(result, Ok(()));
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    assert_eq!(waited, 2);
}

#[tokio::test]
async fn a_broker_that_stays_busy_ends_the_wait_with_busy() {
    let mut waited = 0;
    let result = wait_for_idle(
        || async { Ok(1) },
        Duration::from_millis(5),
        Duration::from_millis(30),
        || waited += 1,
    )
    .await;
    assert_eq!(result, Err("busy".to_string()));
    assert!(waited >= 1);
}

#[tokio::test]
async fn an_idle_broker_is_not_waited_on_and_a_broker_error_is_said() {
    let mut waited = 0;
    let idle = wait_for_idle(
        || async { Ok(0) },
        Duration::from_secs(30),
        Duration::from_secs(60),
        || waited += 1,
    )
    .await;
    assert_eq!(idle, Ok(()));
    assert_eq!(waited, 0);
    let down = wait_for_idle(
        || async { Err::<usize, _>("the broker: gone".to_string()) },
        Duration::from_secs(30),
        Duration::from_secs(60),
        || {},
    )
    .await;
    assert_eq!(down, Err("the broker: gone".to_string()));
}

#[test]
fn a_restarted_link_reports_done_or_failed_once() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(outcome(dir.path(), "main-aaaaaaa"), None, "nothing pending");
    let pending = dir.path().join("update-pending.json");
    std::fs::write(&pending, r#"{"version":"main-bbbbbbb"}"#).unwrap();
    let done = outcome(dir.path(), "main-bbbbbbb").unwrap();
    assert_eq!((done.state, done.detail), (UPDATE_DONE, None));
    assert!(!pending.exists(), "the record is used once");
    assert_eq!(outcome(dir.path(), "main-bbbbbbb"), None);
    std::fs::write(&pending, r#"{"version":"main-bbbbbbb"}"#).unwrap();
    let failed = outcome(dir.path(), "main-aaaaaaa").unwrap();
    assert_eq!(failed.state, UPDATE_FAILED);
    assert!(failed.detail.unwrap().contains("main-bbbbbbb"));
}

/// The real Windows handoff: the updater script starts detached with its output in
/// `<root>\logs\update-<version>.log` (a file, never a pipe), runs the installer it is given
/// and says how it ended. The installer here only records what it was asked.
#[cfg(windows)]
#[test]
fn the_windows_updater_runs_detached_and_logs_to_a_file() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("link-state");
    let dir = state.join("updates").join("ci-test");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::create_dir_all(root.path().join("current")).unwrap();
    std::fs::write(
        dir.join("install-devagotchi.ps1"),
        "param([string]$Version, [string]$ArtifactDir, [string]$Root, [switch]$Autostart)\n\
         Write-Host \"fake installer: $Version from $ArtifactDir into $Root autostart=$Autostart\"\n",
    )
    .unwrap();
    super::handoff::start(&state, &dir, "ci-test").unwrap();
    let log = super::handoff::log_path(&state, "ci-test").unwrap();
    assert_eq!(log, root.path().join("logs").join("update-ci-test.log"));
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    let mut text = String::new();
    while std::time::Instant::now() < deadline {
        text = String::from_utf8_lossy(&std::fs::read(&log).unwrap_or_default()).into_owned();
        if text.contains("devagotchi update to ci-test done") {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    assert!(text.contains("fake installer: ci-test from"), "{text}");
    assert!(text.contains("autostart=True"), "{text}");
    assert!(text.contains("devagotchi update to ci-test done"), "{text}");
}

/// A machine in a container moves with its image: every update the hub sends is answered
/// `failed` with the reason at once, nothing is downloaded, and its hello says `container`.
#[tokio::test]
async fn a_container_refuses_every_update_and_says_so() {
    let state = tempfile::tempdir().unwrap();
    let config = crate::state::LinkConfig {
        hub: "http://127.0.0.1:9".into(),
        device: "docked".into(),
        broker: "/state/broker/broker.sock".into(),
    };
    let docked = crate::update::Updater::new(&config, state.path(), "main-abc1234", true);
    assert_eq!(docked.kind(), Some("container"));
    for force in [false, true] {
        let status = docked.request("main-def5678".into(), force).unwrap();
        assert_eq!(status.state, UPDATE_FAILED);
        let detail = status.detail.unwrap();
        assert!(detail.starts_with("container:"), "{detail}");
        assert!(detail.contains("docker compose up -d --build"), "{detail}");
    }
    assert!(
        !state.path().join("updates").exists(),
        "nothing was downloaded"
    );
    let plain = crate::update::Updater::new(&config, state.path(), "main-abc1234", false);
    assert_eq!(plain.kind(), None);
}
