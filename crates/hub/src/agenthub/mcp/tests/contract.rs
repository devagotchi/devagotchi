//! The board tools held to what boards promise (`.ai/patterns/board-operations.md`, rules 9
//! and 11): a viewer may call exactly the tools that change nothing but its own read state,
//! and every code the store refuses with names the call to make next. Titles, hints, closed
//! and documented inputs and output schemas are every tool's, checked by `check_tools`
//! (`crates/hub/src/mcp/tests/tools.rs`).

use std::{collections::BTreeSet, path::Path};

use devagotchi_contracts::BridgeError;

use super::super::{PER_CALLER_WRITES, READ_ONLY_TOOLS, refuse};

/// Store codes no agent can act on: runtime plumbing, or reached only by a page or the
/// webhook route.
const INTERNAL: &[&str] = &[
    "runtime",
    "board_random",
    "board_id",
    "board_runtime",
    "delivery_conflict",
    "unsupported_action",
    "wake_failed",
    "no_done_state",
    "too_many",
    "undo_too_late",
];

#[test]
fn a_viewer_may_call_exactly_the_board_tools_that_change_nothing_but_its_read_state() {
    let mut seen = 0;
    for tool in crate::mcp::tools() {
        let name = tool.name.to_string();
        if !(name.starts_with("board_") || name.starts_with("approval_")) {
            continue;
        }
        seen += 1;
        let read_only = tool.annotations.as_ref().and_then(|a| a.read_only_hint) == Some(true);
        assert_eq!(
            READ_ONLY_TOOLS.contains(&name.as_str()),
            read_only,
            "{name}: READ_ONLY_TOOLS against its read_only_hint"
        );
        if PER_CALLER_WRITES.contains(&name.as_str()) {
            assert!(
                !read_only,
                "{name} writes the caller's read state: not read-only"
            );
        }
        assert_eq!(
            super::super::viewer_may_call(&name),
            read_only || PER_CALLER_WRITES.contains(&name.as_str()),
            "{name}: what a viewer may call"
        );
    }
    assert!(seen >= 26, "only {seen} board tools served");
}

/// Every refusal code in the files where the store decides.
fn store_codes() -> BTreeSet<String> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../agenthub/src");
    let mut files: Vec<_> = [
        "approval.rs",
        "lease.rs",
        "wake.rs",
        "issue_lifecycle.rs",
        "settings.rs",
        "workflow.rs",
        "checks.rs",
        "lib.rs",
    ]
    .iter()
    .map(|f| src.join(f))
    .collect();
    fn collect(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                collect(&path, files);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                files.push(path);
            }
        }
    }
    for dir in ["hosted", "board"] {
        collect(&src.join(dir), &mut files);
    }
    // `invalid()` builds `invalid`; `read_only` is the hub's own refusal (`mcp/calls.rs`).
    let mut codes = BTreeSet::from(["invalid".to_string(), "read_only".to_string()]);
    for path in files {
        let flat: String = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
            .split_whitespace()
            .collect();
        for marker in ["fail(\"", "BridgeError::new(\""] {
            for piece in flat.split(marker).skip(1) {
                codes.insert(piece.split('"').next().unwrap().to_string());
            }
        }
    }
    codes
}

#[test]
fn every_code_the_store_refuses_with_names_the_next_call() {
    let codes = store_codes();
    assert!(
        codes.len() > 25,
        "the scan found only {} codes",
        codes.len()
    );
    let fallback = refuse(BridgeError::new("zz_never_a_code", ""), None).0.next;
    let missing: Vec<&String> = codes
        .iter()
        .filter(|code| !INTERNAL.contains(&code.as_str()))
        .filter(|code| refuse(BridgeError::new(code, "m"), Some("x")).0.next == fallback)
        .collect();
    assert!(
        missing.is_empty(),
        "no next of its own in mcp/refusals.rs: {missing:?}"
    );
}

/// A wait's default and longest hold are the constants the tool uses, and its field says so.
#[test]
fn each_wait_says_its_default_and_its_longest_hold() {
    use super::super::{
        APPROVAL_WAIT_DEFAULT_MS, APPROVAL_WAIT_MAX_MS, BOARD_WAIT_DEFAULT_MS, BOARD_WAIT_MAX_MS,
    };
    for (tool, default, max) in [
        ("board_wait", BOARD_WAIT_DEFAULT_MS, BOARD_WAIT_MAX_MS),
        (
            "approval_wait",
            APPROVAL_WAIT_DEFAULT_MS,
            APPROVAL_WAIT_MAX_MS,
        ),
    ] {
        let found = crate::mcp::tools()
            .into_iter()
            .find(|t| t.name == tool)
            .unwrap_or_else(|| panic!("{tool} is not served"));
        let said = found.input_schema["properties"]["timeout_ms"]["description"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        assert!(
            said.contains(&format!("default {default}")) && said.contains(&format!("max {max}")),
            "{tool}.timeout_ms says {said:?}, not default {default} and max {max}"
        );
    }
}
