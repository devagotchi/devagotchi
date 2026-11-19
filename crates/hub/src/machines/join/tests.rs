use sha2::{Digest, Sha256};

use super::*;

const CODE: &str = "dvg-0123456789abcdef0123456789abcdef";

/// Whether a placeholder is left (`"$@"` in the shell script is not one).
fn unfilled(text: &str) -> bool {
    [
        "@HUB@",
        "@PROTO@",
        "@DEVICE@",
        "@NAME@",
        "@VERSION@",
        "@CODE@",
        "@SHA_",
    ]
    .iter()
    .any(|placeholder| text.contains(placeholder))
}

fn downloads(files: &[&str]) -> (tempfile::TempDir, Downloads) {
    let dir = tempfile::tempdir().unwrap();
    let mut sums = String::new();
    for name in files {
        std::fs::write(dir.path().join(name), name.as_bytes()).unwrap();
        sums.push_str(&format!(
            "{}  {name}\n",
            hex::encode(Sha256::digest(name.as_bytes()))
        ));
    }
    std::fs::write(dir.path().join("SHA256SUMS"), sums).unwrap();
    std::fs::write(dir.path().join("VERSION"), "main-9842d39\n").unwrap();
    let downloads = Downloads::open(dir.path()).unwrap();
    (dir, downloads)
}

#[test]
fn a_script_carries_the_name_the_version_every_checksum_and_the_code() {
    let (_dir, downloads) = downloads(&[
        "install-devagotchi.sh",
        "install-devagotchi.ps1",
        "devagotchi-x86_64-unknown-linux-gnu.tar.xz",
        "devagotchi-x86_64-pc-windows-msvc.zip",
    ]);
    // On a hosted hub the machine joins under its stored name (tenant tag first) and the
    // person is told the name `machines` shows them.
    let sh = script(
        Shell::Sh,
        "https://app.devagotchi.com/",
        "t0123456789ab-dabo",
        "dabo",
        CODE,
        &downloads,
    )
    .unwrap();
    assert!(!unfilled(&sh), "every placeholder is filled:\n{sh}");
    let done = sh.lines().find(|line| line.contains("done:")).unwrap();
    assert!(
        done.contains("dabo is connected to $hub and starts with the computer. Tell your AI to use \\\"dabo\\\"."),
        "{done}"
    );
    assert!(
        !done.contains("t0123456789ab"),
        "the tag is not the person's word: {done}"
    );
    for wanted in [
        "hub='https://app.devagotchi.com'",
        "device='t0123456789ab-dabo'",
        "version='main-9842d39'",
        "--proto '=https'",
        &format!("printf '%s\\n' '{CODE}' | bash"),
        downloads
            .sha256("devagotchi-x86_64-unknown-linux-gnu.tar.xz")
            .unwrap(),
        downloads.sha256("install-devagotchi.sh").unwrap(),
    ] {
        assert!(sh.contains(wanted), "{wanted} missing:\n{sh}");
    }
    // No aarch64 build here: the script says so on such a machine instead of guessing.
    assert!(sh.contains("target=aarch64-unknown-linux-gnu; sum=''"));
    // The code reaches the installer on stdin only, never as an argument.
    assert_eq!(sh.matches(CODE).count(), 1);
    let ps = script(
        Shell::PowerShell,
        "https://app.devagotchi.com",
        "t0123456789ab-dabo",
        "dabo",
        CODE,
        &downloads,
    )
    .unwrap();
    assert!(!unfilled(&ps), "every placeholder is filled:\n{ps}");
    let done = ps.lines().find(|line| line.contains("done:")).unwrap();
    assert!(done.contains("dabo is connected to $hub and starts with the computer. Tell your AI to use `\"dabo`\"."), "{done}");
    assert!(!done.contains("t0123456789ab"), "{done}");
    assert!(
        ps.contains("$device = 't0123456789ab-dabo'"),
        "--device keeps the stored name"
    );
    assert!(ps.contains(&format!("$env:DEVAGOTCHI_ENROLL_CODE = '{CODE}'")));
    assert!(ps.contains(downloads.sha256("install-devagotchi.ps1").unwrap()));
    assert!(
        !ps.contains("-EnrollCode"),
        "the code never goes on a command line"
    );
    // No machine- or person-specific path (.ai/patterns/link.md rule 11): the person's own
    // choice, else their local app data.
    assert!(!ps.contains("localdev") && !ps.contains("D:\\"), "{ps}");
    assert!(ps.contains("$env:DEVAGOTCHI_ROOT") && ps.contains("$env:LOCALAPPDATA"));
}

#[test]
fn nothing_unsafe_is_ever_written_into_a_script() {
    let (_dir, downloads) = downloads(&["install-devagotchi.sh"]);
    let hub = "https://app.devagotchi.com";
    for (hub, device, code) in [
        ("https://evil.example/'; rm -rf / #", "platinum", CODE),
        ("ftp://hub.example", "platinum", CODE),
        (hub, "Plat'inum", CODE),
        (hub, "platinum", "dvg-0123456789ABCDEF0123456789abcdef"),
        (hub, "platinum", "dvg-'$(id)'"),
    ] {
        assert!(
            script(Shell::Sh, hub, device, "platinum", code, &downloads).is_none(),
            "{hub} {device} {code}"
        );
    }
    assert!(script(Shell::Sh, hub, "platinum", "plat'inum", CODE, &downloads).is_none());
    // A hub without the PowerShell installer writes no PowerShell script.
    assert!(
        script(
            Shell::PowerShell,
            hub,
            "platinum",
            "platinum",
            CODE,
            &downloads
        )
        .is_none()
    );
    assert!(valid_code(CODE) && !valid_code("dvg-") && !valid_code(&format!("{CODE}0")));
}

#[cfg(unix)]
#[test]
fn the_shell_script_parses_and_refuses_a_machine_it_cannot_serve() {
    let (_dir, downloads) = downloads(&["install-devagotchi.sh"]);
    let sh = script(
        Shell::Sh,
        "http://127.0.0.1:9",
        "platinum",
        "platinum",
        CODE,
        &downloads,
    )
    .unwrap();
    assert!(sh.contains("--proto '=http'"));
    let checked = std::process::Command::new("sh")
        .args(["-n", "-c", &sh])
        .status()
        .unwrap();
    assert!(checked.success(), "sh -n rejects the script");
    // As a non-root user it stops before downloading anything, and says how to run it.
    if unsafe_is_root() {
        return;
    }
    let out = std::process::Command::new("sh")
        .args(["-c", &sh])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(
        said.contains("devagotchi join:") && !said.contains(CODE),
        "{said}"
    );
    let refused = refusal(Shell::Sh, "https://app.devagotchi.com");
    let out = std::process::Command::new("sh")
        .args(["-c", &refused])
        .output()
        .unwrap();
    assert!(
        !out.status.success() && String::from_utf8_lossy(&out.stderr).contains("Add a machine")
    );
}

#[cfg(unix)]
fn unsafe_is_root() -> bool {
    std::process::Command::new("id")
        .arg("-u")
        .output()
        .is_ok_and(|out| out.stdout.starts_with(b"0"))
}
