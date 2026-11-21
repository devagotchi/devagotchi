//! Actual browser forms, namespace authority, versioned drafts and PostgreSQL failures.
use super::super::{form::WorkflowDraft, tests::workflow_fields};
use crate::{
    agenthub::Workflow,
    tenancy::tests::tenants::{Hosted, Person, refused},
    web::app::tests::{hosted, page},
};
use serde_json::json;

async fn bind(hub: &Hosted, person: &mut Person, project: &str) {
    let result = hub
        .call(person, "board_bind", json!({"project":project}))
        .await;
    assert!(!refused(&result), "{result}");
}
async fn post(
    hub: &Hosted,
    person: &Person,
    project: &str,
    fields: &[(&str, &str)],
) -> reqwest::Response {
    hub.form(
        &format!("/app/boards/{project}/workflows"),
        Some(&person.cookie),
        fields,
    )
    .await
}
async fn edit(
    hub: &Hosted,
    person: &Person,
    draft: &WorkflowDraft,
    action: &str,
) -> reqwest::Response {
    let fields = workflow_fields(draft, action);
    let pairs = fields
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect::<Vec<_>>();
    hub.form(
        "/app/boards/alpha/workflows/delivery",
        Some(&person.cookie),
        &pairs,
    )
    .await
}
fn draft(version: i64, name: &str) -> WorkflowDraft {
    let mut workflow = Workflow::default_workflow();
    workflow.id = "delivery".into();
    workflow.name = name.into();
    WorkflowDraft { version, workflow }
}
async fn publish(hub: &Hosted, person: &Person) {
    assert_eq!(
        post(
            hub,
            person,
            "alpha",
            &[
                ("action", "publish"),
                ("board_version", "1"),
                ("id", "delivery"),
                ("name", "Delivery")
            ]
        )
        .await
        .status(),
        303
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shared_forms_edit_both_boards_detach_current_rules_and_keep_history() {
    let hub = or_skip!(two_boards());
    let mut alice = hub.first_connect("Claude").await;
    for name in ["alpha", "beta"] {
        bind(&hub, &mut alice, name).await;
    }
    assert!(
        page(&hub, "/app/boards/alpha/workflow", &alice)
            .await
            .1
            .contains("/app/boards/alpha/workflows")
    );
    publish(&hub, &alice).await;
    assert_eq!(
        post(
            &hub,
            &alice,
            "beta",
            &[
                ("action", "bind"),
                ("board_version", "1"),
                ("selection", "delivery:1")
            ]
        )
        .await
        .status(),
        303
    );
    let editor = "/app/boards/alpha/workflows/delivery";
    let html = page(&hub, editor, &alice).await.1;
    assert!(html.contains("alpha, beta") && html.contains("Saving changes every board"));
    let task = hub
        .call(
            &mut alice,
            "board_create",
            json!({"project":"beta","title":"Under shared rules"}),
        )
        .await;
    assert!(!refused(&task), "{task}");
    let id = task["structuredContent"]["id"].as_str().unwrap();
    let held = hub
        .call(&mut alice, "board_claim", json!({"project":"beta","id":id}))
        .await;
    assert!(!refused(&held), "{held}");
    let mut first = draft(1, "Shared <rules>");
    let done = first
        .workflow
        .states
        .iter()
        .find(|s| s.kind == crate::agenthub::StateKind::Done)
        .unwrap()
        .key
        .clone();
    first.workflow.lanes[0].complete_to = done.clone();
    assert_eq!(edit(&hub, &alice, &first, "save").await.status(), 303);
    let completed = hub.call(&mut alice, "board_complete", json!({"project":"beta","id":id,"fence":held["structuredContent"]["fence"],"summary":"Completed with the updated shared stage"})).await;
    assert!(!refused(&completed), "{completed}");
    assert_eq!(completed["structuredContent"]["state"], done);
    for name in ["alpha", "beta"] {
        let html = page(&hub, &format!("/app/boards/{name}/workflow"), &alice)
            .await
            .1;
        assert!(
            html.contains("Shared &lt;rules&gt;") && html.contains("Make this board independent")
        );
        assert!(
            !html.contains("value=\"save\""),
            "bound board must not offer a private save"
        );
    }
    let stale = draft(1, "Keep this stale draft");
    let (status, html) = Hosted::text(edit(&hub, &alice, &stale, "save").await).await;
    assert_eq!(status, 409);
    assert!(html.contains("Keep this stale draft"));
    assert!(html.contains("href=\"/app/boards/alpha/workflows/delivery\">Reload"));
    assert!(html.contains("name=\"version\" value=\"1\""));
    assert_eq!(
        post(
            &hub,
            &alice,
            "beta",
            &[("action", "detach"), ("board_version", "2")]
        )
        .await
        .status(),
        409
    );
    assert_eq!(
        post(
            &hub,
            &alice,
            "alpha",
            &[("action", "delete"), ("id", "delivery"), ("version", "2")]
        )
        .await
        .status(),
        409
    );
    assert_eq!(
        post(
            &hub,
            &alice,
            "beta",
            &[("action", "detach"), ("board_version", "3")]
        )
        .await
        .status(),
        303
    );
    assert_eq!(
        edit(&hub, &alice, &draft(2, "Only alpha"), "save")
            .await
            .status(),
        303
    );
    assert!(
        page(&hub, "/app/boards/beta/workflow", &alice)
            .await
            .1
            .contains("Shared &lt;rules&gt;")
    );
    assert_eq!(
        post(
            &hub,
            &alice,
            "alpha",
            &[("action", "detach"), ("board_version", "4")]
        )
        .await
        .status(),
        303
    );
    assert_eq!(
        post(
            &hub,
            &alice,
            "alpha",
            &[("action", "delete"), ("id", "delivery"), ("version", "3")]
        )
        .await
        .status(),
        303
    );
    assert_eq!(page(&hub, editor, &alice).await.0, 404);
    assert!(
        page(&hub, "/app/boards/alpha/workflow", &alice)
            .await
            .1
            .contains("Only alpha")
    );
    let events = hub
        .call(&mut alice, "board_events", json!({"project":"alpha"}))
        .await;
    assert!(events.to_string().contains("workflow.deleted"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn catalog_forms_reject_cross_account_csrf_duplicates_and_expired_sessions() {
    let hub = or_skip!(hosted());
    let mut alice = hub.first_connect("Claude").await;
    let mut bob = hub.first_connect("Codex").await;
    bind(&hub, &mut alice, "alpha").await;
    bind(&hub, &mut bob, "alpha").await;
    publish(&hub, &alice).await;
    let root = "/app/boards/alpha/workflows";
    assert_eq!(hub.get(root, None).await.status(), 303);
    assert!(!page(&hub, root, &bob).await.1.contains("Delivery"));
    assert_eq!(page(&hub, &format!("{root}/delivery"), &bob).await.0, 404);
    assert_eq!(
        post(
            &hub,
            &bob,
            "alpha",
            &[
                ("action", "bind"),
                ("board_version", "1"),
                ("selection", "delivery:1")
            ]
        )
        .await
        .status(),
        404
    );
    for suffix in ["workflows", "workflows/delivery"] {
        let response = hub
            .http
            .post(format!("{}/app/boards/alpha/{suffix}", hub.url))
            .header("cookie", &alice.cookie)
            .header("origin", "https://elsewhere.example")
            .form(&[("action", "detach"), ("board_version", "2")])
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 403);
    }
    for extra in [("board_version", "2"), ("unexpected", "x")] {
        assert_eq!(
            post(
                &hub,
                &alice,
                "alpha",
                &[("action", "detach"), ("board_version", "2"), extra]
            )
            .await
            .status(),
            400
        );
    }
    assert_eq!(
        edit(&hub, &bob, &draft(1, "Cannot create by editing"), "save")
            .await
            .status(),
        404
    );
    sqlx::query("DELETE FROM devagotchi.auth_sessions")
        .execute(hub.accounts.store().admin())
        .await
        .unwrap();
    assert_eq!(
        post(
            &hub,
            &alice,
            "alpha",
            &[("action", "detach"), ("board_version", "2")]
        )
        .await
        .status(),
        303
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shared_drafts_and_storage_errors_do_not_write_or_lose_the_form() {
    let hub = or_skip!(hosted());
    let mut alice = hub.first_connect("Claude").await;
    bind(&hub, &mut alice, "alpha").await;
    publish(&hub, &alice).await;
    let before = hub
        .call(&mut alice, "board_events", json!({"project":"alpha"}))
        .await;
    let d = draft(1, "Unsaved shared draft");
    for action in ["add-state", "add-lane", "add-transition", "remove-state-0"] {
        let (status, html) = Hosted::text(edit(&hub, &alice, &d, action).await).await;
        assert_eq!(status, 200);
        assert!(html.contains("Unsaved shared draft") && html.contains("Unsaved changes"));
    }
    assert_eq!(
        hub.call(&mut alice, "board_events", json!({"project":"alpha"}))
            .await,
        before
    );
    let mut invalid = d.clone();
    invalid.workflow.initial_state = "missing".into();
    let (status, html) = Hosted::text(edit(&hub, &alice, &invalid, "save").await).await;
    assert_eq!(status, 400);
    assert!(html.contains("Unsaved shared draft"));
    sqlx::query("REVOKE UPDATE ON devagotchi.board_workflows FROM devagotchi_hub")
        .execute(hub.accounts.store().admin())
        .await
        .unwrap();
    let (status, html) = Hosted::text(edit(&hub, &alice, &d, "save").await).await;
    assert_eq!(status, 503);
    assert!(html.contains("Unsaved shared draft"));
    sqlx::query("GRANT UPDATE ON devagotchi.board_workflows TO devagotchi_hub")
        .execute(hub.accounts.store().admin())
        .await
        .unwrap();
    assert_eq!(
        hub.call(&mut alice, "board_events", json!({"project":"alpha"}))
            .await,
        before
    );
    assert_eq!(edit(&hub, &alice, &d, "save").await.status(), 303);
    sqlx::query("REVOKE SELECT ON devagotchi.board_workflows FROM devagotchi_hub")
        .execute(hub.accounts.store().admin())
        .await
        .unwrap();
    assert_eq!(
        page(&hub, "/app/boards/alpha/workflows", &alice).await.0,
        503
    );
    let (status, html) =
        Hosted::text(edit(&hub, &alice, &draft(2, "Read failure draft"), "save").await).await;
    assert_eq!(status, 503);
    assert!(html.contains("Read failure draft"));
    let (status, html) = Hosted::text(
        post(
            &hub,
            &alice,
            "alpha",
            &[
                ("action", "publish"),
                ("board_version", "3"),
                ("id", "kept-id"),
                ("name", "Publication draft during outage"),
            ],
        )
        .await,
    )
    .await;
    assert_eq!(status, 503);
    assert!(html.contains("Publication draft during outage") && html.contains("kept-id"));
    assert!(!html.contains("You have no shared workflows yet"));
}

async fn two_boards() -> Option<Hosted> {
    crate::tenancy::tests::tenants::hosted_with(None, |dir, _| {
        let site = crate::tenancy::tests::access::site_dir(dir, "two-project-site", |site| {
            site["plans"][0]["limits"]["projects"] = json!(2);
        });
        json!({"site":{"dir":site.to_string_lossy()}})
    })
    .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_task_on_the_second_board_prevents_an_incompatible_shared_form_save() {
    let hub = or_skip!(two_boards());
    let mut alice = hub.first_connect("Claude").await;
    for name in ["alpha", "beta"] {
        bind(&hub, &mut alice, name).await;
    }
    publish(&hub, &alice).await;
    let mut planned = draft(1, "Planned work");
    planned
        .workflow
        .states
        .push(crate::agenthub::workflow::State {
            key: "planned".into(),
            label: "Planned <queue>".into(),
            kind: crate::agenthub::StateKind::Queue,
        });
    planned.workflow.initial_state = "planned".into();
    assert_eq!(edit(&hub, &alice, &planned, "save").await.status(), 303);
    assert_eq!(
        post(
            &hub,
            &alice,
            "beta",
            &[
                ("action", "bind"),
                ("board_version", "1"),
                ("selection", "delivery:2")
            ]
        )
        .await
        .status(),
        303
    );
    let task = hub
        .call(
            &mut alice,
            "board_create",
            json!({"project":"beta","title":"Keep my planned state"}),
        )
        .await;
    assert!(!refused(&task), "{task}");
    assert_eq!(task["structuredContent"]["state"], "planned");
    let task_id = task["structuredContent"]["id"].as_str().unwrap();
    let ticket_html = page(&hub, &format!("/app/tickets/beta/{task_id}"), &alice)
        .await
        .1;
    assert!(ticket_html.contains("id=\"state\">Planned &lt;queue&gt;</dd>"));
    let board_html = page(&hub, "/app/boards/beta?state=planned", &alice).await.1;
    assert!(
        board_html.contains("Planned &lt;queue&gt;")
            && board_html.contains("Keep my planned state")
    );
    assert!(
        board_html.contains("name=\"state\" value=\"planned\""),
        "filter submits the immutable state key"
    );
    let before_a = hub
        .call(&mut alice, "board_events", json!({"project":"alpha"}))
        .await;
    let before_b = hub
        .call(&mut alice, "board_events", json!({"project":"beta"}))
        .await;
    let incompatible = draft(2, "Must retain this draft");
    incompatible.workflow.validate().unwrap();
    let (status, html) = Hosted::text(edit(&hub, &alice, &incompatible, "save").await).await;
    assert_eq!(status, 400);
    assert!(html.contains("Must retain this draft"));
    assert_eq!(
        hub.call(&mut alice, "board_events", json!({"project":"alpha"}))
            .await,
        before_a
    );
    assert_eq!(
        hub.call(&mut alice, "board_events", json!({"project":"beta"}))
            .await,
        before_b
    );
    let html = page(&hub, "/app/boards/alpha/workflows/delivery", &alice)
        .await
        .1;
    assert!(html.contains("Planned work") && html.contains("value=\"planned\""));
}
