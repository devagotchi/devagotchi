use super::*;
use std::process::Stdio;
use tokio::time::Instant;

fn system32(name: &str) -> String {
    let root = std::env::var_os("SystemRoot").expect("SystemRoot is set on Windows");
    Path::new(&root)
        .join("System32")
        .join(name)
        .display()
        .to_string()
}

/// `cmd /d /c <line>`, created suspended as the broker creates every root.
fn spawn(line: &str) -> Root {
    let mut command = Command::new(system32("cmd.exe"));
    command
        .args(["/d", "/c"])
        .raw_arg(line)
        // A test that fails before job adoption must not leak a suspended cmd.
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    configure(&mut command);
    Root::Piped(command.spawn().unwrap())
}

fn ping(seconds: u32) -> String {
    format!("{} -n {} 127.0.0.1 >nul", system32("PING.EXE"), seconds + 1)
}

#[test]
fn a_command_line_prints_its_output_and_no_progress_records() {
    let (shell, args) = shell_command("Write-Progress -Activity x -Status y; 'ok'").unwrap();
    let out = std::process::Command::new(shell)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let (stdout, stderr) = (
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(out.status.success(), "{stdout}\n{stderr}");
    assert_eq!(stdout.trim(), "ok", "{stderr}");
    assert!(!stderr.contains("#< CLIXML"), "{stderr}");
}

/// A `.sh` line runs in Git Bash. GitHub's Windows runners have Git for Windows; a
/// machine without it fails here, never skips.
#[test]
fn a_shell_script_runs_in_git_bash() {
    let dir = std::env::temp_dir().join(format!("dvg-git-bash-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("ok.sh");
    std::fs::write(&script, "echo ok\n").unwrap();
    let (shell, args) = shell_command(&script.display().to_string()).unwrap_or_else(|error| {
        panic!("Git for Windows is required on this machine for this test: {error}")
    });
    assert!(
        !shell
            .display()
            .to_string()
            .to_ascii_lowercase()
            .contains("system32")
    );
    let out = std::process::Command::new(&shell)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    let (stdout, stderr) = (
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(
        out.status.success(),
        "{}: {stdout}\n{stderr}",
        shell.display()
    );
    assert_eq!(stdout.trim(), "ok", "{stderr}");
}

#[tokio::test]
async fn the_root_exit_is_the_wait_on_its_handle() {
    let mut child = spawn(&ping(1));
    let tree = OwnedTree::adopt(&mut child, Scope::Group)
        .await
        .ok()
        .unwrap();
    let began = Instant::now();
    tree.root_exit(&mut child).await.unwrap();
    assert!(
        began.elapsed() >= Duration::from_millis(500),
        "{:?}",
        began.elapsed()
    );
    tree.members_settled().await.unwrap();
    assert!(!tree.has_executing_members().unwrap());
}

#[tokio::test]
async fn the_job_reports_members_that_outlive_the_root() {
    // `start /b` runs ping inside the job, after the root has already exited.
    let mut child = spawn(&format!("start \"\" /b {} & exit 0", ping(2)));
    let tree = OwnedTree::adopt(&mut child, Scope::Group)
        .await
        .ok()
        .unwrap();
    tree.root_exit(&mut child).await.unwrap();
    assert!(
        tree.has_executing_members().unwrap(),
        "a member outlives the root"
    );
    timeout(Duration::from_secs(30), tree.members_settled())
        .await
        .expect("settled")
        .unwrap();
    assert!(!tree.has_executing_members().unwrap());
    // The member's exit was reported through the job's completion port.
    let mut exits = tree.notices.exits.clone();
    timeout(Duration::from_secs(10), exits.wait_for(|count| *count > 0))
        .await
        .expect("a job exit notice arrived")
        .unwrap();
}

#[tokio::test]
async fn termination_ends_every_member_and_verifies_it() {
    let mut child = spawn(&format!("start \"\" /b {} & {}", ping(30), ping(30)));
    let tree = OwnedTree::adopt(&mut child, Scope::Group)
        .await
        .ok()
        .unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    let began = Instant::now();
    tree.terminate().await.unwrap();
    assert!(began.elapsed() < GROUP_SETTLE, "{:?}", began.elapsed());
    assert!(!tree.has_executing_members().unwrap());
    // TerminateJobObject can empty the job before its listener has read the notices
    // (the first query already finds no member); they still arrive.
    let mut exits = tree.notices.exits.clone();
    timeout(Duration::from_secs(10), exits.wait_for(|count| *count > 0))
        .await
        .expect("a job exit notice arrived")
        .unwrap();
}

#[test]
fn an_invalid_process_snapshot_fails_closed() {
    assert!(resume::suspended_threads(null_mut(), 0).is_err());
}

#[tokio::test]
async fn a_thread_snapshot_neither_runs_nor_resumes_another_root() {
    let directory = tempfile::tempdir().unwrap();
    let first_marker = directory.path().join("first.txt");
    let second_marker = directory.path().join("second.txt");
    let mut first = spawn(&format!("echo first>\"{}\"", first_marker.display()));
    let mut second = spawn(&format!("echo second>\"{}\"", second_marker.display()));
    let threads =
        resume::suspended_threads(first.raw_handle().unwrap(), first.id().unwrap()).unwrap();
    assert_eq!(
        threads.len(),
        1,
        "a new suspended cmd has only its initial thread"
    );
    // Capturing and checking the metadata (the creation time against GetThreadTimes on
    // the opened handle) must not run the target.
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!first_marker.exists());
    assert!(!second_marker.exists());
    drop(threads);
    let first_tree = OwnedTree::adopt(&mut first, Scope::Group)
        .await
        .ok()
        .unwrap();
    timeout(Duration::from_secs(5), first_tree.root_exit(&mut first))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(&first_marker).unwrap().trim(),
        "first"
    );
    assert!(
        !second_marker.exists(),
        "resuming one root must not resume the other"
    );
    let second_tree = OwnedTree::adopt(&mut second, Scope::Group)
        .await
        .ok()
        .unwrap();
    timeout(Duration::from_secs(5), second_tree.root_exit(&mut second))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(&second_marker).unwrap().trim(),
        "second"
    );
}

#[tokio::test]
async fn an_extra_suspend_fails_adoption_and_terminates_without_effect() {
    use windows_sys::Win32::System::Threading::SuspendThread;
    let directory = tempfile::tempdir().unwrap();
    let marker = directory.path().join("must-not-run.txt");
    let mut child = spawn(&format!("echo unsafe>\"{}\"", marker.display()));
    let threads =
        resume::suspended_threads(child.raw_handle().unwrap(), child.id().unwrap()).unwrap();
    assert_eq!(threads.len(), 1);
    // SAFETY: an owned handle to this test's suspended child. It stands in for a second
    // caller holding a suspension, which adoption must never consume.
    assert_eq!(unsafe { SuspendThread(threads[0].as_raw_handle()) }, 1);
    drop(threads);
    let failure = match OwnedTree::adopt(&mut child, Scope::Group).await {
        Ok(_) => panic!("adopted a root whose thread remained suspended"),
        Err(failure) => failure,
    };
    assert!(
        matches!(failure.state, OperationState::Failed),
        "{}",
        failure.detail
    );
    assert!(
        failure.detail.contains("initial suspend count"),
        "{}",
        failure.detail
    );
    assert!(
        failure.detail.contains("no active processes remaining"),
        "{}",
        failure.detail
    );
    assert!(piped(&mut child).unwrap().try_wait().unwrap().is_some());
    assert!(!marker.exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_suspended_roots_produce_exactly_their_own_effects() {
    let directory = tempfile::tempdir().unwrap();
    let mut tasks = tokio::task::JoinSet::new();
    for index in 0..8 {
        let marker = directory.path().join(format!("root-{index}.txt"));
        tasks.spawn(async move {
            // Prefix redirection: `echo root-N>file` would read the digit as a descriptor.
            let mut child = spawn(&format!(">\"{}\" echo root-{index}", marker.display()));
            let tree = OwnedTree::adopt(&mut child, Scope::Group)
                .await
                .ok()
                .unwrap();
            timeout(Duration::from_secs(5), tree.root_exit(&mut child))
                .await
                .unwrap()
                .unwrap();
            timeout(Duration::from_secs(5), tree.members_settled())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(
                std::fs::read_to_string(marker).unwrap().trim(),
                format!("root-{index}")
            );
        });
    }
    while let Some(result) = tasks.join_next().await {
        result.unwrap();
    }
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 8);
}

/// Processes started detached (`spawn_detached`), which outlive the broker.
mod detached {
    use crate::execution::detached::{self, DetachedSpec};
    use std::{
        ffi::OsString,
        path::Path,
        time::{Duration, Instant},
    };

    #[test]
    fn a_detached_process_runs_where_and_with_only_what_it_is_given() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("out.log");
        let system_root = std::env::var_os("SystemRoot").unwrap();
        let cmd = Path::new(&system_root).join("System32").join("cmd.exe");
        let env = [
            (OsString::from("SystemRoot"), system_root),
            (
                OsString::from("DEVAGOTCHI_PROBE"),
                OsString::from("kept here"),
            ),
        ];
        let args: [OsString; 3] = ["/d".into(), "/c".into(), "cd & set".into()];
        let mut child = detached::spawn(&DetachedSpec {
            program: &cmd,
            args: &args,
            env: &env,
            cwd: dir.path(),
            log: &log,
        })
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        let code = loop {
            if let Some(code) = child.try_wait().unwrap() {
                break code;
            }
            assert!(Instant::now() < deadline, "cmd did not finish");
            std::thread::sleep(Duration::from_millis(50));
        };
        let out = std::fs::read_to_string(&log).unwrap();
        assert_eq!(code, 0, "{out}");
        let mut lines = out.lines();
        let cwd = lines.next().unwrap().trim();
        assert_eq!(
            std::fs::canonicalize(cwd).unwrap(),
            std::fs::canonicalize(dir.path()).unwrap(),
            "{out}"
        );
        let vars: Vec<String> = lines.map(|l| l.to_ascii_uppercase()).collect();
        assert!(
            vars.contains(&"DEVAGOTCHI_PROBE=KEPT HERE".to_string()),
            "{out}"
        );
        for absent in ["PATH=", "USERPROFILE=", "USERNAME="] {
            assert!(
                !vars.iter().any(|v| v.starts_with(absent)),
                "{absent} reached it: {out}"
            );
        }
    }
}
