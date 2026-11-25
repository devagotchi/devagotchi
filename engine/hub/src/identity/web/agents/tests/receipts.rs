use crate::identity::web::agents::tests::setup;
use crate::tenancy::tests::tenants::Hosted;

fn request_key(html: &str) -> &str {
    html.split_once("name=\"request_id\" value=\"")
        .unwrap()
        .1
        .split_once('"')
        .unwrap()
        .0
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn profile_form_keeps_retry_identity_through_validation_and_duplicate_posts() {
    let (hub, cookie, owner) = or_skip!(setup());
    let (_, page) = Hosted::text(hub.get("/app/agents/new", Some(&cookie)).await).await;
    let key = request_key(&page);
    let mut fields = vec![
        ("account", owner.as_str()),
        ("id", "retry-worker"),
        ("name", ""),
        ("kind", "codex"),
        ("role", "worker"),
        ("wip", "2"),
        ("version", "0"),
        ("enabled", "true"),
        ("request_id", key),
    ];
    let (status, invalid) =
        Hosted::text(hub.form("/app/agents", Some(&cookie), &fields).await).await;
    assert_eq!(status, 400);
    assert_eq!(request_key(&invalid), key);
    fields
        .iter_mut()
        .find(|(name, _)| *name == "name")
        .unwrap()
        .1 = "Retry worker";
    for _ in 0..2 {
        assert_eq!(
            hub.form("/app/agents", Some(&cookie), &fields)
                .await
                .status(),
            303
        );
    }
    let replay = hub.form("/app/agents", Some(&cookie), &fields).await;
    assert_eq!(replay.headers()["idempotent-replayed"], "true");
    let current = hub
        .accounts
        .agent(&owner, &owner, "retry-worker")
        .await
        .unwrap();
    assert_eq!(current.version, 1);
    fields
        .iter_mut()
        .find(|(name, _)| *name == "name")
        .unwrap()
        .1 = "Different content";
    assert_eq!(
        hub.form("/app/agents", Some(&cookie), &fields)
            .await
            .status(),
        409
    );
    let (_, next) = Hosted::text(hub.get("/app/agents/retry-worker", Some(&cookie)).await).await;
    assert_ne!(request_key(&next), key);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn configuration_forms_keep_independent_keys_and_replay_completed_deletion() {
    let (hub, cookie, owner) = or_skip!(setup());
    let path = "/app/agents/configuration";
    let (_, page) = Hosted::text(hub.get(path, Some(&cookie)).await).await;
    let keys: Vec<_> = page
        .split("<form ")
        .skip(1)
        .filter(|form| {
            form.starts_with(
                "class=\"stack card\" method=\"post\" action=\"/app/agents/configuration\"",
            )
        })
        .map(request_key)
        .collect();
    let unique: std::collections::BTreeSet<_> = keys.iter().collect();
    assert_eq!(keys.len(), unique.len());
    let new_role = page
        .split("<form ")
        .find(|f| f.contains("value=\"role-save\"") && f.contains("name=\"version\" value=\"0\""))
        .unwrap();
    let key = request_key(new_role);
    let mut fields = vec![
        ("operation", "role-save"),
        ("id", ""),
        ("description", "Review"),
        ("version", "0"),
        ("request_id", key),
    ];
    let (status, invalid) = Hosted::text(hub.form(path, Some(&cookie), &fields).await).await;
    assert_eq!(status, 400);
    let draft = invalid
        .split("<form ")
        .find(|f| f.contains("value=\"role-save\"") && f.contains("name=\"version\" value=\"0\""))
        .unwrap();
    assert_eq!(request_key(draft), key);
    fields[1].1 = "retry-reviewer";
    for _ in 0..2 {
        assert_eq!(hub.form(path, Some(&cookie), &fields).await.status(), 303);
    }
    let (_, page) = Hosted::text(hub.get(path, Some(&cookie)).await).await;
    let delete = page
        .split("<form ")
        .find(|f| {
            f.contains("value=\"role-delete\"")
                && f.contains("name=\"id\" value=\"retry-reviewer\"")
        })
        .unwrap();
    let fields = [
        ("operation", "role-delete"),
        ("id", "retry-reviewer"),
        ("version", "1"),
        ("request_id", request_key(delete)),
    ];
    for _ in 0..2 {
        assert_eq!(hub.form(path, Some(&cookie), &fields).await.status(), 303);
    }
    assert!(
        !hub.accounts
            .principal_configuration(&owner)
            .await
            .unwrap()
            .roles
            .iter()
            .any(|r| r.id == "retry-reviewer")
    );
}
