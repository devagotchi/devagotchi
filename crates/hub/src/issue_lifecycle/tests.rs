use super::*;
use crate::{
    board::{Actor, Filter},
    store::testing,
    tenant_tests::{Hosted, hosted_with_ingress, refused},
};
use serde_json::{Value, json};

const SECRET: &str = "test-only-webhook-secret-32-bytes-long";

fn event(action: &str, state: &str, at: &str) -> IssueWebhookEvent {
    serde_json::from_value(json!({
        "action":action,"repository":{"id":123,"full_name":"acme/demo"},
        "issue":{"number":42,"title":"Source issue","state":state,
            "html_url":"https://github.com/acme/demo/issues/42","updated_at":at,"body":"Original source context"},
        "sender":{"login":"octocat"}
    })).unwrap()
}

fn headers(body: &[u8], delivery: &str) -> HeaderMap {
    let mut mac = Hmac::<Sha256>::new_from_slice(SECRET.as_bytes()).unwrap();
    mac.update(body);
    let signature = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));
    let mut headers = HeaderMap::new();
    headers.insert("x-hub-signature-256", signature.parse().unwrap());
    headers.insert("x-github-event", "issues".parse().unwrap());
    headers.insert("x-github-delivery", delivery.parse().unwrap());
    headers
}

#[test]
fn signatures_use_library_verification_and_require_delivery_headers() {
    let ingress = IssueLifecycleStore::with_secret(SECRET.into()).unwrap();
    let body = br#"{"test":"raw UTF-8 bytes"}"#;
    let signed = headers(body, "delivery-1");
    assert_eq!(
        ingress.verify(&signed, body).unwrap(),
        ("issues".into(), "delivery-1".into())
    );
    assert_eq!(
        ingress.verify(&signed, b"changed").unwrap_err().code,
        "invalid_signature"
    );
    let mut missing = signed.clone();
    missing.remove("x-github-delivery");
    assert_eq!(ingress.verify(&missing, body).unwrap_err().code, "invalid");
    missing = signed.clone();
    missing.append("x-github-delivery", "another".parse().unwrap());
    assert!(ingress.verify(&missing, body).is_err());
    let disabled = IssueLifecycleStore::disabled();
    assert_eq!(
        disabled.verify(&signed, body).unwrap_err().code,
        "not_offered"
    );
    let official = IssueLifecycleStore {
        secret: Some("It's a Secret to Everybody".into()),
    };
    let mut known = HeaderMap::new();
    known.insert("x-github-event", "issues".parse().unwrap());
    known.insert("x-github-delivery", "known-vector".parse().unwrap());
    known.insert(
        "x-hub-signature-256",
        "sha256=757107ea0eb2509fc211221cce984b8a37570b6d7586c22c46f4379c8b043e17"
            .parse()
            .unwrap(),
    );
    official.verify(&known, b"Hello, World!").unwrap();
}

async fn hub() -> Option<(Hosted, testing::TestStore)> {
    let test = testing::postgres().await?;
    let hub=hosted_with_ingress(Some(&test.url),|dir,_|json!({
        "boards":[{"project":"source","dir":dir.join("never-local")}],
        "github":{"repositories":[{"repository_id":123,"repository":"acme/demo","project":"source"}]}
    }),IssueLifecycleStore::with_secret(SECRET.into()).unwrap()).await;
    Some((hub, test))
}

async fn deliver(hub: &Hosted, event: &IssueWebhookEvent, id: &str) -> (u16, Value) {
    let body = serde_json::to_vec(event).unwrap();
    let response = hub
        .http
        .post(format!("{}/webhooks/github/issues", hub.url))
        .headers(headers(&body, id))
        .body(body)
        .send()
        .await
        .unwrap();
    let status = response.status().as_u16();
    let value = response.json().await.unwrap();
    (status, value)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn signed_deliveries_create_one_canonical_ticket_and_preserve_workflow_across_close_reopen() {
    let Some((hub, _test)) = hub().await else {
        return;
    };
    let opened = event("opened", "open", "2026-11-09T01:00:00Z");
    let (a, b) = tokio::join!(deliver(&hub, &opened, "one"), deliver(&hub, &opened, "one"));
    assert_eq!(a.0, 200, "{a:?}");
    assert_eq!(b.0, 200, "{b:?}");
    assert_ne!(a.1["duplicate"], b.1["duplicate"]);
    let source: SourceIssue = serde_json::from_value(a.1["issue"].clone()).unwrap();
    let board = hub.hub.boards.open("source").unwrap();
    assert_eq!(
        board
            .list(&Filter {
                label: Some("github-issue".into()),
                ..Default::default()
            })
            .unwrap()
            .len(),
        1
    );
    let actor = Actor::agent("cli");
    let held = board.claim(&actor, &source.ticket, None).unwrap();
    let before = board.get(&source.ticket).unwrap().ticket;
    let closed = event("closed", "closed", "2026-11-09T02:00:00Z");
    assert_eq!(deliver(&hub, &closed, "close").await.0, 200);
    let after = board.get(&source.ticket).unwrap().ticket;
    assert_eq!(before.state, after.state);
    assert_eq!(after.fence, held.fence);
    assert_eq!(after.holder.as_deref(), Some("cli"));
    let context = board
        .hosted_backend()
        .unwrap()
        .issue_context(123, 42, 0, 100)
        .await
        .unwrap();
    assert_eq!(context.finalization, "pending");
    assert_eq!(context.issue.source_state, "closed");
    let stale = deliver(&hub, &opened, "late-open").await;
    assert_eq!(stale.1["status"], "stale");
    let reopened = event("reopened", "open", "2026-11-09T03:00:00Z");
    let result = deliver(&hub, &reopened, "reopen").await;
    assert_eq!(result.1["issue"]["epoch"], 2);
    let mut changed = reopened.clone();
    changed.issue.title = "Different bytes".into();
    assert_eq!(deliver(&hub, &changed, "reopen").await.0, 409);
    let ambiguous = deliver(&hub, &changed, "ambiguous").await;
    assert_eq!(ambiguous.1["status"], "needs_reconciliation");
    assert_eq!(ambiguous.1["issue"]["issue_title"], "Source issue");
    assert_eq!(ambiguous.1["issue"]["needs_reconciliation"], 1);
    assert!(board.verify_chain().unwrap().ok);
    assert!(!hub.dir.path().join("agenthub-issues.sqlite3").exists());
    assert!(!hub.dir.path().join("never-local").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn issue_routes_require_board_grants_and_context_writes_are_versioned_append_only_and_repeat_safe()
 {
    let Some((hub, _test)) = hub().await else {
        return;
    };
    let mut alice = hub.first_connect("Claude").await;
    let bob = hub.first_connect("Codex").await;
    let me = hub.call(&mut alice, "whoami", json!({})).await;
    assert!(!refused(&me), "{me}");
    hub.config
        .learn_owner(me["structuredContent"]["subject"].as_str().unwrap())
        .unwrap();
    assert_eq!(
        deliver(
            &hub,
            &event("opened", "open", "2026-11-09T01:00:00Z"),
            "open"
        )
        .await
        .0,
        200
    );
    let path = format!(
        "{}/api/v1/agenthub/state?repository=acme%2Fdemo&issue_number=42",
        hub.url
    );
    assert_eq!(hub.http.get(&path).send().await.unwrap().status(), 401);
    assert_eq!(
        hub.http
            .get(&path)
            .bearer_auth(&bob.token)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    let response = hub
        .http
        .get(&path)
        .bearer_auth(&alice.token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let context: IssueContext = response.json().await.unwrap();
    let mut write = ContextWrite {
        repository: "acme/demo".into(),
        issue_number: 42,
        request_id: "write-1".into(),
        expected_version: context.ticket.version,
        fence: None,
        state_summary: "Verified source context".into(),
        handoff_note: "Review evidence".into(),
        decisions: vec![Advice {
            id: "advice-1".into(),
            model_provider: "subscription".into(),
            model_name: "unspecified".into(),
            recommendation: "Preserve the existing lease protocol".into(),
            confidence: 0.9,
            evidence_refs: vec!["test:lease".into()],
        }],
    };
    let url = format!("{}/api/v1/agenthub/state", hub.url);
    let send = |payload: ContextWrite| {
        hub.http
            .post(&url)
            .bearer_auth(&alice.token)
            .header("x-devagotchi-seat", "claude")
            .json(&payload)
            .send()
    };
    let (a, b) = tokio::join!(send(write.clone()), send(write.clone()));
    let a = a.unwrap();
    let b = b.unwrap();
    assert_eq!(a.status(), 200);
    assert_eq!(b.status(), 200);
    let a: ContextReceipt = a.json().await.unwrap();
    let b: ContextReceipt = b.json().await.unwrap();
    assert_eq!(a, b);
    write.state_summary = "Changed content".into();
    assert_eq!(send(write.clone()).await.unwrap().status(), 409);
    write.request_id = "stale-write".into();
    assert_eq!(send(write.clone()).await.unwrap().status(), 409);
    write.expected_version = a.revision;
    let response = send(write.clone()).await.unwrap();
    assert_eq!(response.status(), 200);
    let board = hub.hub.boards.open("source").unwrap();
    let view = board.get(&a.ticket).unwrap();
    assert_eq!(
        view.notes
            .iter()
            .filter(|n| n.kind == "issue.context")
            .count(),
        2
    );
    assert!(
        view.notes
            .iter()
            .filter(|n| n.kind == "issue.context")
            .all(|n| n.author.ends_with("/claude"))
    );
    let held = board
        .claim(&Actor::agent("different-holder"), &a.ticket, None)
        .unwrap();
    write.request_id = "not-holder".into();
    write.expected_version = held.ticket.version;
    assert_eq!(send(write).await.unwrap().status(), 409);
    let page = board
        .hosted_backend()
        .unwrap()
        .issue_context(123, 42, 0, 1)
        .await
        .unwrap();
    assert!(page.next_after.is_some());
    let second = board
        .hosted_backend()
        .unwrap()
        .issue_context(123, 42, page.next_after.unwrap(), 1)
        .await
        .unwrap();
    assert_ne!(page.notes[0].seq, second.notes[0].seq);
    assert!(board.verify_chain().unwrap().ok);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mcp_issue_tools_share_rest_authority_receipts_and_refuse_shadow_boards() {
    let Some((hub, _test)) = hub().await else {
        return;
    };
    let mut alice = hub.first_connect("Claude").await;
    let mut bob = hub.first_connect("Codex").await;
    let me = hub.call(&mut alice, "whoami", json!({})).await;
    hub.config
        .learn_owner(me["structuredContent"]["subject"].as_str().unwrap())
        .unwrap();
    assert_eq!(
        deliver(
            &hub,
            &event("opened", "open", "2026-11-09T01:00:00Z"),
            "open"
        )
        .await
        .0,
        200
    );
    let lookup = json!({"repository":"acme/demo","issue_number":42});
    let denied_attach = hub
        .call(&mut bob, "board_attach", json!({"project":"source"}))
        .await;
    assert!(refused(&denied_attach), "{denied_attach}");
    let attached = hub
        .call(&mut alice, "board_attach", json!({"project":"source"}))
        .await;
    assert!(!refused(&attached), "{attached}");
    assert_eq!(
        attached["structuredContent"]["prefix"],
        hub.hub.boards.open("source").unwrap().prefix()
    );
    let denied = hub
        .call(&mut bob, "board_issue_context", lookup.clone())
        .await;
    assert!(refused(&denied), "{denied}");
    let read = hub
        .call(&mut alice, "board_issue_context", lookup.clone())
        .await;
    assert!(!refused(&read), "{read}");
    let context = &read["structuredContent"];
    let write = json!({
        "repository":"acme/demo","issue_number":42,"request_id":"mcp-1",
        "expected_version":context["ticket"]["version"],
        "state_summary":"MCP checkpoint seen by REST",
        "handoff_note":"Keep the evidence",
        "decisions":[{"id":"one","recommendation":"Use the canonical board","confidence":0.95,
                      "evidence_refs":["test:mcp-rest"]}]
    });
    let receipt = hub
        .call(&mut alice, "board_issue_checkpoint", write.clone())
        .await;
    assert!(!refused(&receipt), "{receipt}");
    let retry = hub
        .call(&mut alice, "board_issue_checkpoint", write.clone())
        .await;
    assert_eq!(receipt["structuredContent"], retry["structuredContent"]);
    let mut changed = write;
    changed["state_summary"] = json!("Changed content");
    let conflict = hub
        .call(&mut alice, "board_issue_checkpoint", changed)
        .await;
    assert!(refused(&conflict), "{conflict}");
    assert!(
        conflict.to_string().contains("request_conflict"),
        "{conflict}"
    );
    let response = hub
        .http
        .get(format!(
            "{}/api/v1/agenthub/state?repository=acme%2Fdemo&issue_number=42",
            hub.url
        ))
        .bearer_auth(&alice.token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let rest: IssueContext = response.json().await.unwrap();
    assert_eq!(
        rest.ticket.version,
        receipt["structuredContent"]["revision"].as_i64().unwrap()
    );
    assert_eq!(
        rest.notes
            .iter()
            .filter(|n| n.kind == "issue.context")
            .count(),
        1
    );
    assert!(
        rest.notes
            .iter()
            .any(|n| n.body.contains("MCP checkpoint seen by REST"))
    );

    let response = hub
        .http
        .get(format!("{}/app/tickets/source/{}", hub.url, rest.ticket.id))
        .header("cookie", &alice.cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let html = response.text().await.unwrap();
    assert!(html.contains("id=\"issue-source\""), "{html}");
    assert!(
        html.contains("href=\"https://github.com/acme/demo/issues/42\""),
        "{html}"
    );
    assert!(html.contains("MCP checkpoint seen by REST"), "{html}");
    let closed = event("closed", "closed", "2026-11-09T02:00:00Z");
    assert_eq!(deliver(&hub, &closed, "close-for-ui").await.0, 200);
    let response = hub
        .http
        .get(format!("{}/app/tickets/source/{}", hub.url, rest.ticket.id))
        .header("cookie", &alice.cookie)
        .send()
        .await
        .unwrap();
    let html = response.text().await.unwrap();
    assert!(
        html.contains("Pending: GitHub closure does not complete board work"),
        "{html}"
    );

    let checkout = hub.dir.path().join("shared checkout with spaces");
    let account_file = hub.dir.path().join("cli-account.json");
    crate::account::write(
        &account_file,
        &crate::account::Stored::Account(crate::account::Account::for_tests(
            "https://id.example",
            me["structuredContent"]["subject"].as_str().unwrap(),
            &alice.token,
            crate::account::unix_now() + 3600,
        )),
    )
    .unwrap();
    let cli_hub = hub.url.clone();
    let ticket_id = rest.ticket.id.clone();
    tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&checkout).unwrap();
        let invoke = |args: &[&str]| {
            let mut out = Vec::new();
            let mut err = Vec::new();
            let code = crate::board::cli::run(
                crate::board::cli::Invocation {
                    args: args.iter().map(std::ffi::OsString::from).collect(),
                    cwd: checkout.clone(),
                    actor_env: Some("claude".into()),
                    session_env: None,
                    os_user: Some("owner".into()),
                    stdout_is_terminal: false,
                    account_file: Some(account_file.clone()),
                },
                &mut out,
                &mut err,
            );
            assert_eq!(
                code,
                0,
                "{} {}",
                String::from_utf8_lossy(&out),
                String::from_utf8_lossy(&err)
            );
            serde_json::from_slice::<Value>(&out).unwrap()
        };
        let bound = invoke(&["bind", "--shared", "--hub", &cli_hub, "--project", "source"]);
        assert_eq!(bound["shared"], true);
        assert_eq!(bound["created"], false);
        let view = invoke(&["show", &ticket_id]);
        assert!(view["notes"].as_array().unwrap().iter().any(|note| {
            note["body"]
                .as_str()
                .unwrap()
                .contains("MCP checkpoint seen by REST")
        }));
        assert!(!checkout.join(".devagotchi").join("board.sqlite3").exists());
    })
    .await
    .unwrap();

    // Simulate a personal namespace created before its owner obtained the shared grant.
    hub.hub
        .boards
        .bind(
            me["structuredContent"]["subject"].as_str().unwrap(),
            "source",
            &[],
        )
        .unwrap();
    let shadow = hub.call(&mut alice, "board_issue_context", lookup).await;
    assert!(refused(&shadow), "{shadow}");
    assert!(shadow.to_string().contains("not_granted"), "{shadow}");
    let rest_shadow = hub
        .http
        .get(format!(
            "{}/api/v1/agenthub/state?repository=acme%2Fdemo&issue_number=42",
            hub.url
        ))
        .bearer_auth(&alice.token)
        .send()
        .await
        .unwrap();
    assert_eq!(rest_shadow.status(), 403);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn paged_context_retains_more_than_twenty_five_advice_records_across_checkpoints() {
    let Some((hub, _test)) = hub().await else {
        return;
    };
    let mut alice = hub.first_connect("Claude").await;
    let me = hub.call(&mut alice, "whoami", json!({})).await;
    hub.config
        .learn_owner(me["structuredContent"]["subject"].as_str().unwrap())
        .unwrap();
    assert_eq!(
        deliver(
            &hub,
            &event("opened", "open", "2026-11-09T01:00:00Z"),
            "open"
        )
        .await
        .0,
        200
    );
    let lookup = json!({"repository":"acme/demo","issue_number":42,"limit":1});
    let read = hub
        .call(&mut alice, "board_issue_context", lookup.clone())
        .await;
    let mut version = read["structuredContent"]["ticket"]["version"]
        .as_i64()
        .unwrap();
    for batch in 0..2 {
        let decisions: Vec<Value> = (0..15).map(|i|json!({
            "id":format!("advice-{}",batch*15+i),"recommendation":format!("Verified recommendation {}",batch*15+i),
            "confidence":0.9,"evidence_refs":[format!("test:evidence-{}",batch*15+i)]
        })).collect();
        let response = hub.call(&mut alice, "board_issue_checkpoint", json!({
            "repository":"acme/demo","issue_number":42,"request_id":format!("batch-{batch}"),
            "expected_version":version,"state_summary":format!("Batch {batch}"),"decisions":decisions
        })).await;
        assert!(!refused(&response), "{response}");
        version = response["structuredContent"]["revision"].as_i64().unwrap();
    }
    let mut after = 0;
    let mut advice = Vec::new();
    loop {
        let mut query = lookup.clone();
        query["after"] = json!(after);
        let read = hub.call(&mut alice, "board_issue_context", query).await;
        assert!(!refused(&read), "{read}");
        for note in read["structuredContent"]["notes"].as_array().unwrap() {
            if note["kind"] == "issue.context" {
                let context: ContextWrite =
                    serde_json::from_str(note["body"].as_str().unwrap()).unwrap();
                advice.extend(context.decisions);
            }
        }
        let Some(next) = read["structuredContent"]["next_after"].as_i64() else {
            break;
        };
        assert!(next > after);
        after = next;
    }
    assert_eq!(advice.len(), 30);
    for (i, decision) in advice.iter().enumerate() {
        assert_eq!(decision.id, format!("advice-{i}"));
        assert_eq!(decision.evidence_refs, vec![format!("test:evidence-{i}")]);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn webhook_deadline_includes_cold_board_resolution_and_failed_delivery_can_be_retried() {
    let Some((hub, test)) = hub().await else {
        return;
    };
    let mut blocker = test.store.admin().begin().await.unwrap();
    sqlx::query("LOCK TABLE devagotchi.board_projects IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *blocker)
        .await
        .unwrap();
    let opened = event("opened", "open", "2026-11-09T01:00:00Z");
    let started = std::time::Instant::now();
    let response = deliver(&hub, &opened, "retry-timeout").await;
    assert_eq!(response.0, 503, "{response:?}");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "cold board resolution exceeded GitHub's response deadline: {:?}",
        started.elapsed()
    );
    blocker.rollback().await.unwrap();
    let retry = deliver(&hub, &opened, "retry-timeout").await;
    assert_eq!(retry.0, 200, "{retry:?}");
    assert_eq!(retry.1["duplicate"], false);
    let replay = deliver(&hub, &opened, "retry-timeout").await;
    assert_eq!(replay.1["duplicate"], true);
    let board = hub.hub.boards.open("source").unwrap();
    assert_eq!(
        board
            .list(&Filter {
                label: Some("github-issue".into()),
                ..Default::default()
            })
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn malformed_or_unauthorized_ingress_and_failed_outbox_never_leave_partial_issue_state() {
    let Some((hub, test)) = hub().await else {
        return;
    };
    let good = event("opened", "open", "2026-11-09T01:00:00Z");
    let mut wrong = good.clone();
    wrong.repository.id = 999;
    assert_eq!(deliver(&hub, &wrong, "wrong-repo").await.0, 422);
    wrong = good.clone();
    wrong.issue.updated_at = "2026-99-99T01:00:00Z".into();
    assert_eq!(deliver(&hub, &wrong, "wrong-time").await.0, 422);
    let bad = hub
        .http
        .post(format!("{}/webhooks/github/issues", hub.url))
        .body(serde_json::to_vec(&good).unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(bad.status(), 422);
    sqlx::query("ALTER TABLE devagotchi.board_outbox ADD CONSTRAINT refuse_source CHECK(kind <> 'issue.projected')")
        .execute(test.store.admin()).await.unwrap();
    assert_eq!(deliver(&hub, &good, "retry-after-failure").await.0, 503);
    let board = hub.hub.boards.open("source").unwrap();
    assert!(
        board
            .list(&Filter {
                label: Some("github-issue".into()),
                ..Default::default()
            })
            .unwrap()
            .is_empty()
    );
    sqlx::query("ALTER TABLE devagotchi.board_outbox DROP CONSTRAINT refuse_source")
        .execute(test.store.admin())
        .await
        .unwrap();
    assert_eq!(deliver(&hub, &good, "retry-after-failure").await.0, 200);
    assert_eq!(
        board
            .list(&Filter {
                label: Some("github-issue".into()),
                ..Default::default()
            })
            .unwrap()
            .len(),
        1
    );
}
