//! The hub's operator commands, driven as the host operator runs them: the real binary, its
//! stdin and its output (increment 117). On a fresh Postgres database from
//! `DEVAGOTCHI_TEST_POSTGRES_URL`; each test skips without one.

use std::process::Command;

fn devagotchi() -> Command {
    Command::new(env!("CARGO_BIN_EXE_devagotchi"))
}

/// The host operator's password recovery, as the binary runs it: `hub owner` makes the owner,
/// then `hub set-password` takes the password from a pipe (with a CRLF a Windows file leaves),
/// sets it on that account and on no other, and never echoes it. On a fresh Postgres database
/// from `DEVAGOTCHI_TEST_POSTGRES_URL`; skipped without one. From increment 117.
#[test]
fn hub_set_password_reads_stdin_keeps_the_owner_and_never_prints_the_password() {
    use std::{io::Write, process::Stdio};
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let Some(url) = runtime.block_on(devagotchi_store::testing::empty_postgres()) else {
        eprintln!("{}", devagotchi_store::testing::SKIPPED);
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let config = dir.path().join("hub.json");
    std::fs::write(
        &config,
        serde_json::json!({
            "listen": "127.0.0.1:1", "public_url": "http://127.0.0.1:8787",
            "auth": {"mode": "hub"}, "owner": "recovery-owner", "brokers": [], "grants": []
        })
        .to_string(),
    )
    .unwrap();
    let command = |verb: &str| {
        let mut cmd = devagotchi();
        cmd.args(["hub", verb, "--state-dir"])
            .arg(&state)
            .arg("--config")
            .arg(&config)
            .env("DEVAGOTCHI_HUB_STORAGE_URL", &url)
            .env_remove("DEVAGOTCHI_HUB_STORAGE_SCHEMA")
            .env_remove("DEVAGOTCHI_HUB_TENANCY")
            .env_remove("DEVAGOTCHI_HUB_OPERATORS");
        cmd
    };
    let owner = command("owner").output().unwrap();
    assert!(
        owner.status.success(),
        "{}",
        String::from_utf8_lossy(&owner.stderr)
    );
    for (account, success) in [("recovery-owner", true), ("missing-account", false)] {
        let mut child = command("set-password")
            .args([
                "--account",
                account,
                "--email",
                "owner@example.test",
                "--password-stdin",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"test secret password\r\n")
            .unwrap();
        let out = child.wait_with_output().unwrap();
        let (stdout, stderr) = (
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
        assert_eq!(out.status.success(), success, "{account}: {stderr}");
        assert!(!stdout.contains("test secret password"), "{stdout}");
        assert!(!stderr.contains("test secret password"), "{stderr}");
        if success {
            assert!(
                stdout.contains("Password set for recovery-owner"),
                "{stdout}"
            );
        }
    }
    let help = devagotchi()
        .args(["hub", "owner", "--help"])
        .output()
        .unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("hub set-password"));
}
