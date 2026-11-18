//! A ticket's origin on the hub: over HTTP-MCP, two accounts importing the same project and
//! source each get a ticket of their own, a repeat from another session returns the same
//! one, and nobody names who recorded it; through the board command line on a bound project,
//! an imported ticket is worked through its lanes while its importer repeats itself.

use serde_json::{Value, json};

use super::serving::{MEMBER, hub_with, run, signed_in, tool};
use crate::agenthub::cli::{EXIT_CONFLICT, EXIT_OK};

const OTHER: &str = "300000000000000009";

fn origin() -> Value {
    json!({ "path": "docs/ROADMAP.md", "item": "scanner-preview",
            "revision": "commit:0123", "content_sha256": "a".repeat(64) })
}

fn create(origin: Value) -> Value {
    json!({ "project": "demo", "title": "Implement project scanner",
            "labels": ["roadmap"], "origin": origin })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn each_account_imports_its_own_and_a_repeat_from_another_session_is_the_same() {
    let dir = tempfile::tempdir().unwrap();
    let url = or_skip!(hub_with(dir.path(), json!({}), &[("tok-other", OTHER)]));
    let mut made = Vec::new();
    for (token, subject) in [("tok-member", MEMBER), ("tok-other", OTHER)] {
        let bound = tool(&url, token, "board_bind", json!({ "project": "demo" })).await;
        assert_eq!(bound["isError"], false, "{bound}");
        let ticket = tool(&url, token, "board_create", create(origin())).await;
        assert_eq!(ticket["isError"], false, "{ticket}");
        let ticket = ticket["structuredContent"].clone();
        let id = ticket["id"].as_str().unwrap().to_string();
        let view = tool(
            &url,
            token,
            "board_get",
            json!({ "project": "demo", "id": id }),
        )
        .await;
        let provenance = &view["structuredContent"]["provenance"];
        assert_eq!(provenance["source"], origin(), "{view}");
        assert_eq!(
            provenance["recorded_by"], subject,
            "the caller, as the hub knows it"
        );
        assert_eq!(provenance["recorded_at"], ticket["created_at"]);

        // Every `tool` call is an MCP session of its own: the hub fills another session
        // anchor, and that is no change of what was asked for.
        let events = |token| tool(&url, token, "board_events", json!({ "project": "demo" }));
        let before = events(token).await;
        let again = tool(&url, token, "board_create", create(origin())).await;
        assert_eq!(again["structuredContent"]["id"], id.as_str(), "{again}");
        assert_eq!(again["structuredContent"]["version"], ticket["version"]);

        // Nobody names who recorded it, and a changed import is refused: nothing written.
        let mut forged = origin();
        forged["recorded_by"] = json!("operator");
        let refused = tool(&url, token, "board_create", create(forged)).await;
        assert_eq!(refused["isError"], true, "{refused}");
        let mut changed = create(origin());
        changed["title"] = json!("A different scope");
        let refused = tool(&url, token, "board_create", changed).await;
        assert_eq!(refused["isError"], true, "{refused}");
        let error = &refused["structuredContent"];
        assert_eq!(error["code"], "source_conflict", "{refused}");
        assert!(error["message"].as_str().unwrap().contains(&id), "{error}");
        assert!(
            error["next"].as_str().unwrap().contains("reconcile"),
            "{error}"
        );
        assert_eq!(
            events(token).await,
            before,
            "no event for a repeat or a refusal"
        );
        made.push((token, id, ticket["created_by"].clone()));
    }
    // Two tickets, one per account: neither account's board holds the other's, by list or by id.
    let [(member, member_id, member_by), (other, other_id, other_by)] = &made[..] else {
        unreachable!()
    };
    assert_ne!(member_by, other_by);
    let accounts = [
        (member, member_id, member_by, other_id),
        (other, other_id, other_by, member_id),
    ];
    for (token, own, by, theirs) in accounts {
        // Ids are random per board; on the 1 in 65536 chance both drew one, it is our own.
        if theirs != own {
            let read = json!({ "project": "demo", "id": theirs });
            let read = tool(&url, token, "board_get", read).await;
            assert_eq!(read["isError"], true, "{read}");
            assert_eq!(read["structuredContent"]["code"], "not_found", "{read}");
        }
        let listed = tool(&url, token, "board_list", json!({ "project": "demo" })).await;
        let sourced: Vec<&Value> = listed["structuredContent"]["tickets"]
            .as_array()
            .or_else(|| listed["structuredContent"].as_array())
            .unwrap_or_else(|| panic!("{listed}"))
            .iter()
            .filter(|t| t["title"] == "Implement project scanner")
            .collect();
        assert_eq!(sourced.len(), 1, "{listed}");
        assert_eq!(
            (&sourced[0]["id"], &sourced[0]["created_by"]),
            (&json!(own), by)
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_imported_ticket_on_a_bound_project_is_made_once_and_worked_through_its_lanes() {
    let dir = tempfile::tempdir().unwrap();
    let url = or_skip!(hub_with(dir.path(), json!({}), &[]));
    let project = dir.path().join("demo");
    std::fs::create_dir_all(&project).unwrap();
    let account = dir.path().join("account.json");
    signed_in(&account, "tok-member");
    tokio::task::spawn_blocking(move || {
        let given = origin().to_string();
        let ok = |actor: Option<&str>, args: &[&str]| {
            let (code, value, all) = run(&project, &account, actor, args);
            assert_eq!(code, EXIT_OK, "{args:?}: {all}");
            value
        };
        ok(None, &["bind", "--hub", &url]);
        let import = [
            "new",
            "Implement project scanner",
            "--origin",
            given.as_str(),
        ];
        let made = ok(Some("claude"), &import);
        let id = made["id"].as_str().unwrap().to_string();
        let lease = ok(Some("claude"), &["claim", &id]);

        // The importer runs again from another seat while claude holds it.
        let again = ok(Some("codex"), &import);
        assert_eq!(again["id"], id.as_str());
        assert_eq!(
            (&again["holder"], &again["fence"]),
            (&json!("claude"), &lease["fence"])
        );
        ok(
            Some("claude"),
            &["handoff", &id, "over to codex", "--to", "codex"],
        );
        ok(Some("codex"), &["claim", &id]);
        ok(Some("codex"), &["done", &id, "built"]);
        let (code, refused, all) = run(&project, &account, Some("codex"), &["claim", &id]);
        assert_eq!(code, EXIT_CONFLICT, "{all}");
        assert_eq!(refused["error"]["code"], "distinct_required");
        ok(Some("claude"), &["claim", &id]);
        assert_eq!(
            ok(Some("claude"), &["done", &id, "reviewed"])["state"],
            "done"
        );

        // After it is done a repeat returns it done; a changed import is refused, and the
        // hub writes nothing for it.
        let after = ok(Some("codex"), &import);
        assert_eq!(
            (&after["id"], &after["state"]),
            (&json!(id), &json!("done"))
        );
        let entries = ok(None, &["verify"])["entries"].clone();
        let changed = ["new", "A different scope", "--origin", given.as_str()];
        let (code, refused, all) = run(&project, &account, Some("codex"), &changed);
        assert_eq!(code, EXIT_CONFLICT, "{all}");
        assert_eq!(refused["error"]["code"], "source_conflict", "{refused}");
        assert_eq!(ok(None, &["verify"])["entries"], entries);

        // The export carries the origin as the hub recorded it.
        let shown = ok(None, &["show", &id]);
        assert_eq!(shown["provenance"]["recorded_by"], "claude", "{shown}");
        assert_eq!(shown["provenance"]["source"], origin());
        let out = project.join("board.jsonl");
        ok(None, &["export", "--out", out.to_str().unwrap()]);
        let exported = std::fs::read_to_string(&out).unwrap();
        let row = exported
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .find(|row| row["ticket"]["id"] == id.as_str())
            .unwrap();
        assert_eq!(row["provenance"], shown["provenance"]);
        assert_eq!(ok(None, &["verify"])["ok"], true);
    })
    .await
    .unwrap();
}
