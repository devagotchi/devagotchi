//! Every board call leaves exactly one audit line, refusals included, over HTTP-MCP as an AI
//! client calls (`.ai/patterns/board-operations.md`, rule 12; mcp-tools AU1): a seat header
//! that is not a name, a fence the hub does not know, a dependency kind that does not exist,
//! a wait on a board nobody holds, and a bind that made the board; and none of the successes
//! counted a failure by `get_usage_stats`. On the test Postgres
//! (`DEVAGOTCHI_TEST_POSTGRES_URL`; CI sets it).

use serde_json::json;

use super::serving::{MEMBER, hub_audited, tool, tool_as};
use crate::ops::audit::Recorded;

/// The one line `tool` left for the member, or a panic naming what was there.
async fn only_line(audit: &crate::ops::audit::Audit, tool: &str) -> Recorded {
    let (lines, matched) = audit.calls_of(MEMBER, Some(tool), None, 10).await.unwrap();
    assert_eq!(matched, 1, "{tool}: {lines:?}");
    lines.into_iter().next().unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refused_board_call_leaves_one_line_with_its_code() {
    let dir = tempfile::tempdir().unwrap();
    let (url, audit) = or_skip!(hub_audited(dir.path(), json!({}), &[]));
    let bound = tool(
        &url,
        "tok-member",
        "board_bind",
        json!({"project": "alpha"}),
    )
    .await;
    assert_eq!(bound["structuredContent"]["created"], true, "{bound}");
    let line = only_line(&audit, "board_bind").await;
    assert_eq!(
        (line.outcome.as_str(), line.operation_id.as_str()),
        ("ok", "created")
    );

    // A seat header that is not a name: refused inside the call, with its line.
    let seated = tool_as(
        &url,
        "tok-member",
        Some("Not A Name"),
        "board_list",
        json!({"project": "alpha"}),
    )
    .await;
    assert_eq!(seated["isError"], true, "{seated}");
    assert_eq!(seated["structuredContent"]["code"], "invalid");
    assert_eq!(only_line(&audit, "board_list").await.outcome, "invalid");

    // No fence given and none known: `stale_fence`, with its line naming the ticket.
    let made = tool(
        &url,
        "tok-member",
        "board_create",
        json!({"project": "alpha", "title": "t"}),
    )
    .await;
    let id = made["structuredContent"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let done = tool(
        &url,
        "tok-member",
        "board_complete",
        json!({"project": "alpha", "id": id, "summary": "done"}),
    )
    .await;
    assert_eq!(done["structuredContent"]["code"], "stale_fence", "{done}");
    let line = only_line(&audit, "board_complete").await;
    assert_eq!(
        (line.outcome.as_str(), line.operation_id.as_str()),
        ("stale_fence", id.as_str())
    );

    // A dependency kind that does not exist.
    let dep = tool(
        &url,
        "tok-member",
        "board_dep",
        json!({"project": "alpha", "id": id, "depends_on": id, "kind": "sideways"}),
    )
    .await;
    assert_eq!(dep["structuredContent"]["code"], "invalid", "{dep}");
    assert_eq!(only_line(&audit, "board_dep").await.outcome, "invalid");

    // A wait on a board this account does not have: one line, with the refusal's code.
    let waited = tool(
        &url,
        "tok-member",
        "board_wait",
        json!({"project": "nobodys", "after_seq": 0, "timeout_ms": 0}),
    )
    .await;
    assert_eq!(waited["isError"], true, "{waited}");
    let line = only_line(&audit, "board_wait").await;
    assert_eq!(
        line.outcome,
        waited["structuredContent"]["code"].as_str().unwrap()
    );
}

/// A wait that answers is one line however long it held, with `ok`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_wait_is_one_line_however_long_it_held() {
    let dir = tempfile::tempdir().unwrap();
    let (url, audit) = or_skip!(hub_audited(dir.path(), json!({}), &[]));
    tool(
        &url,
        "tok-member",
        "board_bind",
        json!({"project": "alpha"}),
    )
    .await;
    let waited = tool(
        &url,
        "tok-member",
        "board_wait",
        json!({"project": "alpha", "after_seq": 1000, "timeout_ms": 300}),
    )
    .await;
    assert_eq!(waited["structuredContent"]["events"], json!([]), "{waited}");
    assert_eq!(only_line(&audit, "board_wait").await.outcome, "ok");
    // A bind that made the board and a wait that answered are successes, never failures
    // (AU1: `created` and an approval's status go in the target, not the outcome).
    let usage = tool(&url, "tok-member", "get_usage_stats", json!({})).await;
    let usage = &usage["structuredContent"];
    assert_eq!(usage["calls"], 2, "{usage}");
    assert_eq!(usage["failed"], 0, "{usage}");
}
