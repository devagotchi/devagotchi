use super::*;

#[cfg(unix)]
const BROKER: &str = "/run/b.sock";
#[cfg(windows)]
const BROKER: &str = r"\\.\pipe\devagotchi-broker-x";

/// A compose file brought up a second time runs `enroll` again with a code already
/// spent: a key enrolled for the same hub and device makes that a success, decided before
/// any code is read or any connection made (this hub's port answers nothing).
#[tokio::test]
async fn re_enrolling_with_an_existing_key_exits_0_without_a_network_call() {
    let parent = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    let parent_path = std::fs::canonicalize(parent.path()).unwrap();
    #[cfg(windows)]
    let parent_path = parent.path().to_path_buf();
    let dir = parent_path.join("link");
    let config = LinkConfig {
        hub: "http://127.0.0.1:9".into(),
        device: "docked".into(),
        broker: BROKER.into(),
    };
    // Nothing enrolled yet: the code and the hub are needed.
    assert!(!already_enrolled(&dir, &config).unwrap());
    // A key and this very config on disk, as the first enroll leaves them.
    state::device_key(&dir).unwrap();
    state::save_config(&dir, &config).unwrap();
    assert!(already_enrolled(&dir, &config).unwrap());
    let started = std::time::Instant::now();
    let exit = dispatch(
        Command::Enroll {
            state: dir.clone(),
            config: config.clone(),
        },
        "test",
    )
    .await;
    assert_eq!(exit, ExitCode::SUCCESS);
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "no connection was tried"
    );
    // Another device or hub is a new enrollment, never a silent success.
    for other in [
        LinkConfig {
            device: "other".into(),
            ..config.clone()
        },
        LinkConfig {
            hub: "https://hub.example".into(),
            ..config.clone()
        },
    ] {
        assert!(!already_enrolled(&dir, &other).unwrap());
    }
}

#[test]
fn jitter_stays_within_a_quarter() {
    let base = Duration::from_secs(8);
    for _ in 0..50 {
        let pause = jitter(base);
        assert!(pause >= base && pause <= base.mul_f32(1.25));
    }
}
