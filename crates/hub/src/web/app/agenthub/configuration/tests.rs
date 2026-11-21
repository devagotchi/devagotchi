//! Real browser POSTs and MCP share configuration, versions, authority and audit.
use super::form::{SettingsDraft, WorkflowDraft};
use crate::{
    agenthub::{ConfigurationState, Settings, Workflow},
    tenancy::tests::tenants::{Hosted, Person, refused},
    web::app::tests::{hosted, page},
};
use serde_json::{Value, json};

fn initial() -> ConfigurationState {
    ConfigurationState {
        settings: Settings::default(),
        settings_version: 1,
        workflow: Workflow::default_workflow(),
        workflow_version: 1,
    }
}
fn settings_fields(version: i64) -> Vec<(String, String)> {
    let mut draft = SettingsDraft::current(&initial());
    draft.version = version;
    let mut fields: Vec<_> = draft.values.into_iter().collect();
    fields.push(("version".into(), version.to_string()));
    fields
}
pub(super) fn workflow_fields(draft: &WorkflowDraft, action: &str) -> Vec<(String, String)> {
    let w = &draft.workflow;
    let mut fields = vec![
        ("version".into(), draft.version.to_string()),
        ("action".into(), action.into()),
        ("id".into(), w.id.clone()),
        ("name".into(), w.name.clone()),
        ("initial_state".into(), w.initial_state.clone()),
        ("attention_state".into(), w.attention_state.clone()),
        ("states".into(), w.states.len().to_string()),
        ("lanes".into(), w.lanes.len().to_string()),
        ("transitions".into(), w.transitions.len().to_string()),
    ];
    for (i, s) in w.states.iter().enumerate() {
        fields.push((format!("state_{i}_key"), s.key.clone()));
        fields.push((format!("state_{i}_label"), s.label.clone()));
        fields.push((
            format!("state_{i}_kind"),
            serde_json::to_value(s.kind)
                .unwrap()
                .as_str()
                .unwrap()
                .into(),
        ));
    }
    for (i, l) in w.lanes.iter().enumerate() {
        for (k, v) in [
            ("queue", &l.queue),
            ("active", &l.active),
            ("complete_to", &l.complete_to),
            ("release_to", &l.release_to),
        ] {
            fields.push((format!("lane_{i}_{k}"), v.clone()));
        }
        if l.distinct_from_previous {
            fields.push((format!("lane_{i}_distinct"), "true".into()));
        }
    }
    for (i, t) in w.transitions.iter().enumerate() {
        fields.push((format!("transition_{i}_from"), t.from.clone()));
        fields.push((format!("transition_{i}_to"), t.to.clone()));
    }
    fields
}
fn set(fields: &mut [(String, String)], key: &str, value: &str) {
    fields.iter_mut().find(|(k, _)| k == key).unwrap().1 = value.into();
}
async fn post(
    hub: &Hosted,
    person: &Person,
    workflow: bool,
    fields: &[(String, String)],
) -> reqwest::Response {
    hub.form(
        if workflow {
            "/app/boards/alpha/workflow"
        } else {
            "/app/boards/alpha/settings"
        },
        Some(&person.cookie),
        &fields
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect::<Vec<_>>(),
    )
    .await
}
async fn bind(hub: &Hosted, person: &mut Person) {
    let r = hub
        .call(person, "board_bind", json!({"project":"alpha"}))
        .await;
    assert!(!refused(&r), "{r}");
}
async fn events(hub: &Hosted, person: &mut Person) -> Value {
    hub.call(person, "board_events", json!({"project":"alpha"}))
        .await["structuredContent"]
        .clone()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn settings_forms_save_through_the_board_and_retain_conflicting_or_invalid_drafts() {
    let hub = or_skip!(hosted());
    let mut alice = hub.first_connect("Claude").await;
    let bob = hub.first_connect("Codex").await;
    bind(&hub, &mut alice).await;
    let url = "/app/boards/alpha/settings";
    assert_eq!(hub.get(url, None).await.status(), 303);
    let response = hub.get(url, Some(&alice.cookie)).await;
    assert_eq!(response.headers()["referrer-policy"], "same-origin");
    let (status, html) = Hosted::text(response).await;
    assert_eq!(status, 200);
    assert!(
        html.contains("Default lease (seconds)") && html.contains("name=\"version\" value=\"1\"")
    );
    assert!(
        page(&hub, "/app/boards/alpha", &alice)
            .await
            .1
            .contains("/app/boards/alpha/settings")
    );
    let before = events(&hub, &mut alice).await;
    let mut fields = settings_fields(1);
    assert_eq!(post(&hub, &alice, false, &fields).await.status(), 303);
    assert_eq!(events(&hub, &mut alice).await, before, "no-op has no event");
    set(&mut fields, "default_priority", "6");
    set(&mut fields, "lease_ttl", "60.001");
    set(&mut fields, "environments", "dev, qa");
    let response = post(&hub, &alice, false, &fields).await;
    assert_eq!(response.status(), 303);
    assert_eq!(
        response.headers()["location"],
        "/app/boards/alpha/settings?saved=1"
    );
    let got = hub
        .call(&mut alice, "board_settings", json!({"project":"alpha"}))
        .await;
    assert_eq!(got["structuredContent"]["default_priority"], 6);
    assert_eq!(got["structuredContent"]["lease_ttl_ms"], 60_001);
    let saved = events(&hub, &mut alice).await;
    let (status, html) = Hosted::text(post(&hub, &alice, false, &fields).await).await;
    assert_eq!(status, 409);
    assert!(
        html.contains("Someone changed")
            && html.contains("value=\"60.001\"")
            && html.contains("Reload current settings")
    );
    set(&mut fields, "version", "2");
    set(&mut fields, "lease_ttl", "0");
    let (status, html) = Hosted::text(post(&hub, &alice, false, &fields).await).await;
    assert_eq!(status, 400);
    assert!(html.contains("value=\"0\""));
    assert_eq!(events(&hub, &mut alice).await, saved);
    assert_eq!(page(&hub, url, &bob).await.0, 404);
    assert_eq!(post(&hub, &bob, false, &fields).await.status(), 404);
    // A real database refusal neither looks like a saved draft nor advances history.
    set(&mut fields, "lease_ttl", "61");
    sqlx::query("REVOKE UPDATE ON devagotchi.board_settings FROM devagotchi_hub")
        .execute(hub.accounts.store().admin())
        .await
        .unwrap();
    let (status, html) = Hosted::text(post(&hub, &alice, false, &fields).await).await;
    assert_eq!(status, 503);
    assert!(html.contains("value=\"61\""));
    sqlx::query("GRANT UPDATE ON devagotchi.board_settings TO devagotchi_hub")
        .execute(hub.accounts.store().admin())
        .await
        .unwrap();
    assert_eq!(events(&hub, &mut alice).await, saved);
    sqlx::query("REVOKE SELECT ON devagotchi.board_settings FROM devagotchi_hub")
        .execute(hub.accounts.store().admin())
        .await
        .unwrap();
    assert_eq!(page(&hub, url, &alice).await.0, 503);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn workflow_forms_edit_drafts_without_writing_and_save_only_the_current_version() {
    let hub = or_skip!(hosted());
    let mut alice = hub.first_connect("Claude").await;
    bind(&hub, &mut alice).await;
    let mut draft = WorkflowDraft::current(&initial());
    draft.workflow.name = "Team <workflow>".into();
    draft.workflow.states[0].label = "Later <work>".into();
    let fields = workflow_fields(&draft, "save");
    assert_eq!(post(&hub, &alice, true, &fields).await.status(), 303);
    let (_, html) = page(&hub, "/app/boards/alpha/workflow", &alice).await;
    assert!(html.contains("Team &lt;workflow&gt;") && html.contains("Later &lt;work&gt;"));
    let before = events(&hub, &mut alice).await;
    let (status, html) = Hosted::text(post(&hub, &alice, true, &fields).await).await;
    assert_eq!(status, 409);
    assert!(html.contains("Team &lt;workflow&gt;"));
    draft.version = 2;
    draft.workflow.name = "Unsaved title".into();
    for action in [
        "add-state",
        "add-lane",
        "add-transition",
        "remove-state-0",
        "remove-lane-0",
        "remove-transition-0",
    ] {
        let (status, html) =
            Hosted::text(post(&hub, &alice, true, &workflow_fields(&draft, action)).await).await;
        assert_eq!(status, 200, "{action}: {html}");
        assert!(html.contains("Unsaved title") && html.contains("Unsaved changes"));
    }
    assert_eq!(events(&hub, &mut alice).await, before);
    assert!(
        !page(&hub, "/app/boards/alpha/workflow", &alice)
            .await
            .1
            .contains("Unsaved title")
    );
    draft.workflow.initial_state = "in_progress".into();
    let (status, html) =
        Hosted::text(post(&hub, &alice, true, &workflow_fields(&draft, "save")).await).await;
    assert_eq!(status, 400);
    assert!(html.contains("Unsaved title"));
    assert_eq!(events(&hub, &mut alice).await, before);
    let mut duplicate = workflow_fields(&draft, "save");
    duplicate.push(("version".into(), "3".into()));
    assert_eq!(post(&hub, &alice, true, &duplicate).await.status(), 400);
    let mut excess = workflow_fields(&draft, "save");
    set(&mut excess, "states", "513");
    assert_eq!(post(&hub, &alice, true, &excess).await.status(), 400);
    assert_eq!(events(&hub, &mut alice).await, before);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn configuration_posts_require_this_hubs_origin_and_a_live_browser_session() {
    let hub = or_skip!(hosted());
    let mut alice = hub.first_connect("Claude").await;
    bind(&hub, &mut alice).await;
    for (origin, fetch) in [
        (None, None),
        (None, Some("same-origin")),
        (Some("null"), None),
        (Some("https://elsewhere.example"), Some("same-origin")),
        (Some(hub.url.as_str()), Some("cross-site")),
    ] {
        for suffix in ["settings", "workflow"] {
            let mut request = hub
                .http
                .post(format!("{}/app/boards/alpha/{suffix}", hub.url))
                .header("cookie", &alice.cookie)
                .form(&settings_fields(1));
            if let Some(o) = origin {
                request = request.header("origin", o);
            }
            if let Some(f) = fetch {
                request = request.header("sec-fetch-site", f);
            }
            assert_eq!(request.send().await.unwrap().status(), 403);
        }
    }
    let bearer = hub
        .http
        .post(format!("{}/app/boards/alpha/settings", hub.url))
        .bearer_auth(&alice.token)
        .header("origin", &hub.url)
        .form(&settings_fields(1))
        .send()
        .await
        .unwrap();
    assert_eq!(
        bearer.status(),
        303,
        "AI token does not replace browser identity"
    );
    sqlx::query("DELETE FROM devagotchi.auth_sessions")
        .execute(hub.accounts.store().admin())
        .await
        .unwrap();
    assert_eq!(
        post(&hub, &alice, false, &settings_fields(1))
            .await
            .status(),
        303
    );
}

async fn api(
    hub: &Hosted,
    person: &Person,
    method: reqwest::Method,
    path: &str,
    body: Value,
) -> Value {
    let response = hub
        .http
        .request(method, format!("{}{path}", hub.url))
        .header("cookie", &person.cookie)
        .header("origin", &hub.url)
        .json(&body)
        .send()
        .await
        .unwrap();
    let status = response.status();
    let text = response.text().await.unwrap();
    assert!(status.is_success(), "{status}: {text}");
    serde_json::from_str(&text).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn configuration_authority_is_checked_again_after_each_role_change() {
    use reqwest::Method;
    let hub = or_skip!(hosted());
    let owner = hub.first_connect("Claude").await;
    let mut member = hub.first_connect("Codex").await;
    let id = hub.call(&mut member, "whoami", json!({})).await["structuredContent"]["subject"]
        .as_str()
        .unwrap()
        .to_string();
    api(
        &hub,
        &owner,
        Method::POST,
        "/api/v1/orgs",
        json!({"org":"team","name":"Team"}),
    )
    .await;
    api(
        &hub,
        &owner,
        Method::POST,
        "/api/v1/orgs/team/projects",
        json!({"project":"alpha"}),
    )
    .await;
    let invite = api(
        &hub,
        &owner,
        Method::POST,
        "/api/v1/orgs/team/invites",
        json!({"role":"admin","note":"configuration test"}),
    )
    .await;
    api(
        &hub,
        &member,
        Method::POST,
        "/api/v1/invites/accept",
        json!({"token":invite["token"]}),
    )
    .await;
    let root = "/app/boards/team%2Falpha";
    // A project admin, and even this organization's owner, has no account-catalog authority.
    for person in [&owner, &member] {
        assert_eq!(
            page(&hub, &format!("{root}/workflows"), person).await.0,
            403
        );
        assert_eq!(
            hub.form(
                &format!("{root}/workflows"),
                Some(&person.cookie),
                &[("action", "detach"), ("board_version", "1")]
            )
            .await
            .status(),
            403
        );
    }
    for suffix in ["settings", "workflow"] {
        assert_eq!(
            page(&hub, &format!("{root}/{suffix}"), &member).await.0,
            200
        );
    }
    for role in ["member", "agent", "viewer"] {
        api(
            &hub,
            &owner,
            Method::PUT,
            &format!("/api/v1/orgs/team/members/{id}"),
            json!({"role":role}),
        )
        .await;
        for suffix in ["settings", "workflow"] {
            let path = format!("{root}/{suffix}");
            assert_eq!(
                page(&hub, &path, &member).await.0,
                403,
                "{role} GET {suffix}"
            );
            let fields = settings_fields(1);
            let pairs: Vec<_> = fields
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            assert_eq!(
                hub.form(&path, Some(&member.cookie), &pairs).await.status(),
                403,
                "{role} POST {suffix}"
            );
        }
        let (status, html) = page(&hub, root, &member).await;
        assert_eq!(status, 200);
        assert!(!html.contains("/settings\""));
    }
    api(
        &hub,
        &owner,
        Method::PUT,
        &format!("/api/v1/orgs/team/members/{id}"),
        json!({"role":"admin"}),
    )
    .await;
    assert_eq!(
        page(&hub, &format!("{root}/settings"), &member).await.0,
        200
    );
    let fields = settings_fields(1);
    let pairs: Vec<_> = fields
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    assert_eq!(
        hub.form(&format!("{root}/settings"), Some(&member.cookie), &pairs)
            .await
            .status(),
        303
    );
}
