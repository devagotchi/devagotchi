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
fn spawn(line: &str) -> Child {
    let mut command = Command::new(system32("cmd.exe"));
    command
        .args(["/d", "/c"])
        .raw_arg(line)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    configure(&mut command);
    command.spawn().unwrap()
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
