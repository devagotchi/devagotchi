use super::{form::WorkflowDraft, tests::*};
use crate::{tenancy::tests::tenants::Hosted, web::app::tests::hosted};

fn key(html: &str) -> String {
    html.split_once("name=\"request_id\" value=\"")
        .unwrap()
        .1
        .split_once('"')
        .unwrap()
        .0
        .into()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn browser_settings_and_workflow_replay_without_reapplying_or_repeating_events() {
    let hub = or_skip!(hosted());
    let mut alice = hub.first_connect("Claude").await;
    bind(&hub, &mut alice).await;
    let (_, html) = Hosted::text(
        hub.get("/app/boards/alpha/settings", Some(&alice.cookie))
            .await,
    )
    .await;
    let mut fields = settings_fields(1);
    fields.push(("request_id".into(), key(&html)));
    set(&mut fields, "default_priority", "5");
    assert_eq!(post(&hub, &alice, false, &fields).await.status(), 303);
    let before = events(&hub, &mut alice).await;
    assert_eq!(post(&hub, &alice, false, &fields).await.status(), 303);
    assert_eq!(events(&hub, &mut alice).await, before);
    let mut later = settings_fields(2);
    set(&mut later, "default_priority", "9");
    assert_eq!(post(&hub, &alice, false, &later).await.status(), 303);
    assert_eq!(post(&hub, &alice, false, &fields).await.status(), 303);
    let (_, html) = Hosted::text(
        hub.get("/app/boards/alpha/settings", Some(&alice.cookie))
            .await,
    )
    .await;
    assert!(html.contains("name=\"default_priority\" value=\"9\""));

    let (_, html) = Hosted::text(
        hub.get("/app/boards/alpha/workflow", Some(&alice.cookie))
            .await,
    )
    .await;
    let mut draft = WorkflowDraft::current(&initial());
    draft.workflow.name = "Retry workflow".into();
    let mut fields = workflow_fields(&draft, "add-state");
    let request_key = key(&html);
    fields.push(("request_id".into(), request_key.clone()));
    let (status, html) = Hosted::text(post(&hub, &alice, true, &fields).await).await;
    assert_eq!(status, 200);
    assert_eq!(
        key(&html),
        request_key,
        "draft-only row controls preserve retry identity"
    );
    set(&mut fields, "action", "save");
    assert_eq!(post(&hub, &alice, true, &fields).await.status(), 303);
    let before = events(&hub, &mut alice).await;
    assert_eq!(post(&hub, &alice, true, &fields).await.status(), 303);
    assert_eq!(events(&hub, &mut alice).await, before);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn browser_shared_workflow_replays_publish_edit_detach_and_delete() {
    let hub = or_skip!(hosted());
    let mut alice = hub.first_connect("Claude").await;
    bind(&hub, &mut alice).await;
    let path = "/app/boards/alpha/workflows";
    let (_, html) = Hosted::text(hub.get(path, Some(&alice.cookie)).await).await;
    let form = html
        .split("<form ")
        .find(|f| f.contains("name=\"action\" value=\"publish\""))
        .unwrap();
    let request_key = key(form);
    let publish = [
        ("action", "publish"),
        ("board_version", "1"),
        ("id", "delivery"),
        ("name", "Delivery"),
        ("request_id", request_key.as_str()),
    ];
    for _ in 0..2 {
        assert_eq!(
            hub.form(path, Some(&alice.cookie), &publish).await.status(),
            303
        );
    }
    let mut draft = WorkflowDraft::current(&initial());
    draft.workflow.id = "delivery".into();
    draft.workflow.name = "Delivery updated".into();
    let mut fields = workflow_fields(&draft, "save");
    fields.push(("request_id".into(), "shared-edit".into()));
    let pairs: Vec<_> = fields
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let editor = "/app/boards/alpha/workflows/delivery";
    for _ in 0..2 {
        assert_eq!(
            hub.form(editor, Some(&alice.cookie), &pairs).await.status(),
            303
        );
    }
    let detach = [
        ("action", "detach"),
        ("board_version", "3"),
        ("request_id", "shared-detach"),
    ];
    for _ in 0..2 {
        assert_eq!(
            hub.form(path, Some(&alice.cookie), &detach).await.status(),
            303
        );
    }
    let delete = [
        ("action", "delete"),
        ("id", "delivery"),
        ("version", "2"),
        ("request_id", "shared-delete"),
    ];
    for _ in 0..2 {
        assert_eq!(
            hub.form(path, Some(&alice.cookie), &delete).await.status(),
            303
        );
    }
    // Saved publication/edit responses remain readable after deletion; they do not recreate it.
    assert_eq!(
        hub.form(editor, Some(&alice.cookie), &pairs).await.status(),
        303
    );
    assert_eq!(
        hub.form(path, Some(&alice.cookie), &publish).await.status(),
        303
    );
    assert_eq!(hub.get(editor, Some(&alice.cookie)).await.status(), 404);
}
