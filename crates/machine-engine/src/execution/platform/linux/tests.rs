use super::*;
use std::process::Stdio;
use tokio::time::Instant;

fn spawn(script: &str) -> Child {
    let mut command = Command::new("/bin/sh");
    command
        .args(["-c", script])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    configure(&mut command);
    command.spawn().unwrap()
}

async fn owned(child: &mut Child) -> OwnedTree {
    let tree = OwnedTree::adopt(child, Scope::Group).await.ok().unwrap();
    assert!(
        tree.root.is_some(),
        "this kernel should offer pidfd_open (Linux 5.3+)"
    );
    tree
}

#[tokio::test]
async fn a_pidfd_is_readable_when_the_root_exits_and_not_before() {
    let mut child = spawn("sleep 0.5");
    let tree = owned(&mut child).await;
    let root = tree.root.as_ref().unwrap();
    assert!(
        timeout(Duration::from_millis(100), root.readable())
            .await
            .is_err(),
        "readable while the root still runs"
    );
    let began = Instant::now();
    tree.root_exit(&mut child).await.unwrap();
    assert!(began.elapsed() < Duration::from_secs(10));
    // Observed without reaping: the status is still there to collect.
    assert!(root_exited_without_reaping(tree.pid).unwrap());
    assert!(child.wait().await.unwrap().success());
}

#[tokio::test]
async fn members_that_outlive_the_root_are_waited_for_by_their_exits() {
    // More members than are watched at once: they are watched in turns.
    let mut child = spawn("i=0; while [ $i -lt 100 ]; do sleep 0.4 & i=$((i+1)); done; exit 0");
    let tree = owned(&mut child).await;
    tree.root_exit(&mut child).await.unwrap();
    assert!(
        !group_members(tree.pid).unwrap().is_empty(),
        "members outlive the root"
    );
    let began = Instant::now();
    timeout(Duration::from_secs(20), tree.members_settled())
        .await
        .expect("settled")
        .unwrap();
    assert!(group_members(tree.pid).unwrap().is_empty());
    assert!(began.elapsed() < Duration::from_secs(20));
    child.wait().await.unwrap();
}

#[tokio::test]
async fn termination_ends_root_and_members_and_verifies_it() {
    let mut child = spawn("sleep 30 & sleep 30");
    let tree = owned(&mut child).await;
    while group_members(tree.pid).unwrap().len() < 2 {
        sleep(Duration::from_millis(10)).await;
    }
    tree.terminate().await.unwrap();
    assert!(group_members(tree.pid).unwrap().is_empty());
    assert!(!child.wait().await.unwrap().success());
}

#[tokio::test]
async fn a_member_that_ignores_sigterm_is_killed_after_the_grace() {
    let mut child = spawn("trap '' TERM; sleep 30 & wait");
    let tree = owned(&mut child).await;
    while group_members(tree.pid).unwrap().len() < 2 {
        sleep(Duration::from_millis(10)).await;
    }
    let began = Instant::now();
    tree.terminate().await.unwrap();
    // The root ignored SIGTERM, so the grace ran out before SIGKILL ended it.
    assert!(began.elapsed() >= TERM_GRACE, "{:?}", began.elapsed());
    assert!(group_members(tree.pid).unwrap().is_empty());
    child.wait().await.unwrap();
}

#[tokio::test]
async fn a_descendant_that_leaves_the_group_does_not_hold_settlement() {
    // setsid starts a new session: outside the owned group, as SCOPE says.
    let mut child = spawn("setsid sleep 3 & sleep 0.2; exit 0");
    let tree = owned(&mut child).await;
    tree.root_exit(&mut child).await.unwrap();
    let began = Instant::now();
    tree.members_settled().await.unwrap();
    assert!(
        began.elapsed() < Duration::from_secs(2),
        "{:?}",
        began.elapsed()
    );
    child.wait().await.unwrap();
}

#[tokio::test]
async fn a_member_that_leaves_after_it_is_watched_does_not_hold_settlement() {
    // In the group when settlement starts watching it, then out of it by setsid without
    // exiting: settlement ends when it leaves, not when it exits three seconds later.
    let mut child = spawn("(sleep 0.3; exec setsid sleep 3) & exit 0");
    let tree = owned(&mut child).await;
    tree.root_exit(&mut child).await.unwrap();
    assert!(
        !group_members(tree.pid).unwrap().is_empty(),
        "still a member"
    );
    let began = Instant::now();
    tree.members_settled().await.unwrap();
    assert!(
        began.elapsed() < Duration::from_secs(2),
        "{:?}",
        began.elapsed()
    );
    child.wait().await.unwrap();
}

/// Processes started detached (`spawn_detached`), which outlive the broker.
mod detached {
    use super::super::*;
    use crate::execution::detached::DetachedSpec;
    use std::{ffi::OsString, os::fd::AsRawFd};

    #[test]
    fn a_detached_process_holds_nothing_of_its_callers_and_has_a_session_of_its_own() {
        let dir = tempfile::tempdir().unwrap();
        // A descriptor the caller left inheritable, as an AI client's pipe to it may be.
        let extra = std::fs::File::open("/dev/null").unwrap();
        let fd = extra.as_raw_fd();
        // SAFETY: clears close-on-exec on a descriptor this test owns.
        assert_eq!(unsafe { libc::fcntl(fd, libc::F_SETFD, 0) }, 0);
        let log = dir.path().join("log");
        let script = format!(
            "[ -e /proc/self/fd/{fd} ] && echo leaked || echo closed; cut -d' ' -f6 /proc/$$/stat; echo \"secret=${{SECRET:-none}}\"; pwd"
        );
        let env = vec![(
            OsString::from("PATH"),
            std::env::var_os("PATH").unwrap_or_default(),
        )];
        let args = [OsString::from("-c"), OsString::from(script)];
        let mut child = spawn_detached(&DetachedSpec {
            program: Path::new("/bin/sh"),
            args: &args,
            env: &env,
            cwd: dir.path(),
            log: &log,
        })
        .unwrap();
        let began = std::time::Instant::now();
        while child.try_wait().unwrap().is_none() {
            assert!(began.elapsed() < Duration::from_secs(10), "never exited");
            std::thread::sleep(Duration::from_millis(20));
        }
        let out = std::fs::read_to_string(&log).unwrap();
        let lines: Vec<&str> = out.lines().map(str::trim).collect();
        assert_eq!(lines.len(), 4, "{out}");
        assert_eq!(
            lines[0], "closed",
            "the caller's descriptor reached it: {out}"
        );
        // SAFETY: getsid(0) reads this process's own session id.
        let ours = unsafe { libc::getsid(0) };
        assert_ne!(
            lines[1],
            ours.to_string(),
            "it shares the caller's session: {out}"
        );
        assert_eq!(lines[2], "secret=none", "only the given environment: {out}");
        assert_eq!(
            lines[3],
            std::fs::canonicalize(dir.path()).unwrap().to_str().unwrap()
        );
        let mode = std::fs::metadata(&log).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        drop(extra);
    }
}
