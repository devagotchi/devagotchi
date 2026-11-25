//! The machine link's mechanical check (`.ai/patterns/link.md`, The mechanical check, 2):
//! the frame dispatches name every variant (rule 5), the words both sides compare come from
//! the contracts (rule 1), and nothing a person runs to join carries one person's paths
//! (rule 11). The wire's own golden test is in `platform/contracts/src/link/tests.rs`.

mod support;

use std::{fs, path::Path};

use support::{rust_files, workspace_root};

/// The link's and the hub's frame dispatches.
const DISPATCHES: [&str; 2] = [
    "runtime/link/src/session/mod.rs",
    "engine/hub/src/machines/link/serve.rs",
];
/// Where a word both sides compare could be written by hand.
const LINK_CODE: [&str; 3] = [
    "runtime/link/src",
    "engine/hub/src/machines/link",
    "engine/hub/src/machines/join",
];
/// Words the contracts define (`MACHINE_KIND_CONTAINER`, `REFUSAL_UNAVAILABLE`, `UPDATE_*`).
/// `done` and `failed` are too common to scan for; review holds them.
const SHARED_WORDS: [&str; 5] = [
    "\"container\"",
    "\"unavailable\"",
    "\"waiting_for_jobs\"",
    "\"downloading\"",
    "\"installing\"",
];
/// What a person runs to join a machine, and the installers it downloads.
const JOIN_ARTIFACTS: [&str; 5] = [
    "engine/hub/src/machines/join/join.sh",
    "engine/hub/src/machines/join/join.ps1",
    "engine/hub/src/machines/join/compose.yml",
    "deploy/windows/install-devagotchi.ps1",
    "deploy/linux/install-devagotchi.sh",
];

/// A file's code without its tests: test files are skipped, and so is everything after a
/// file's first `#[cfg(test)]`.
fn code_of(relative: &str, text: &str) -> Option<String> {
    let name = relative.rsplit('/').next().unwrap_or(relative);
    let test_file = name == "tests.rs"
        || name.ends_with("_tests.rs")
        || relative.split('/').any(|part| part == "tests");
    if test_file {
        return None;
    }
    let code = match text.find("#[cfg(test)]") {
        Some(at) => &text[..at],
        None => text,
    };
    Some(code.to_string())
}

/// Lines (1-based) whose text holds `needle`.
fn lines_with(text: &str, needle: &str) -> Vec<usize> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| line.contains(needle))
        .map(|(n, _)| n + 1)
        .collect()
}

/// A Windows drive path such as `D:\`, not preceded by a letter or digit (so `Env:\` and
/// `HKCU:\` are not one).
fn has_drive_path(line: &str) -> bool {
    let bytes = line.as_bytes();
    bytes.windows(3).enumerate().any(|(at, w)| {
        w[0].is_ascii_alphabetic()
            && w[1] == b':'
            && w[2] == b'\\'
            && (at == 0 || !bytes[at - 1].is_ascii_alphanumeric())
    })
}

fn read(root: &Path, path: &str) -> String {
    fs::read_to_string(root.join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

#[test]
fn every_frame_dispatch_names_its_variants() {
    let root = workspace_root();
    let mut found = Vec::new();
    for path in DISPATCHES {
        let code = code_of(path, &read(&root, path)).unwrap();
        for needle in ["Ok(_) =>", "Ok(_) |"] {
            for line in lines_with(&code, needle) {
                found.push(format!("{path}:{line}: {needle}"));
            }
        }
    }
    assert!(
        found.is_empty(),
        "a frame dispatch with a wildcard arm turns a new frame into a reconnect loop instead of a compile error (.ai/patterns/link.md rule 5):\n{}",
        found.join("\n")
    );
}

#[test]
fn the_words_both_sides_compare_come_from_the_contracts() {
    let root = workspace_root();
    let mut found = Vec::new();
    for (path, file) in rust_files(&root) {
        if !LINK_CODE
            .iter()
            .any(|dir| path.starts_with(&format!("{dir}/")))
        {
            continue;
        }
        let Some(code) = code_of(&path, &fs::read_to_string(&file).unwrap()) else {
            continue;
        };
        for word in SHARED_WORDS {
            for line in lines_with(&code, word) {
                found.push(format!("{path}:{line}: {word}"));
            }
        }
    }
    assert!(
        found.is_empty(),
        "use the devagotchi_contracts constant, so a typo on one side cannot compile (.ai/patterns/link.md rule 1):\n{}",
        found.join("\n")
    );
}

#[test]
fn nothing_a_person_runs_to_join_carries_one_persons_paths() {
    let root = workspace_root();
    let mut found = Vec::new();
    for path in JOIN_ARTIFACTS {
        for (n, line) in read(&root, path).lines().enumerate() {
            if line.contains("localdev") || has_drive_path(line) {
                found.push(format!("{path}:{}: {}", n + 1, line.trim()));
            }
        }
    }
    assert!(
        found.is_empty(),
        "a join template or installer holds a machine- or person-specific path (.ai/patterns/link.md rule 11):\n{}",
        found.join("\n")
    );
}

#[test]
fn the_scan_sees_what_it_looks_for() {
    assert!(has_drive_path(r"$root = 'D:\localdev\x'"));
    assert!(has_drive_path(r"C:\x"));
    assert!(!has_drive_path(r"Remove-Item Env:\DEVAGOTCHI_ENROLL_CODE"));
    assert!(!has_drive_path(r"HKCU:\Software\Microsoft"));
    assert!(!has_drive_path("http://127.0.0.1:8787"));
    assert_eq!(
        code_of("crates/x/src/a.rs", "a\n#[cfg(test)]\nOk(_) =>").as_deref(),
        Some("a\n")
    );
    for test_file in [
        "crates/x/src/tests.rs",
        "crates/x/src/update_tests.rs",
        "crates/x/src/tests/a.rs",
    ] {
        assert!(code_of(test_file, "").is_none(), "{test_file}");
    }
}
