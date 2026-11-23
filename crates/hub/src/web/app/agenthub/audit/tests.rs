use crate::tenancy::tests::tenants::refused;
use crate::web::app::tests::{hosted, page};
use serde_json::json;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn audit_filters_pages_verifies_and_never_shows_another_accounts_history() {
    let hub = or_skip!(hosted());
    let mut alice = hub.first_connect("Claude").await;
    let bob = hub.first_connect("Codex").await;
    let bound = hub
        .call(&mut alice, "board_bind", json!({"project":"audit"}))
        .await;
    assert!(!refused(&bound), "{bound}");
    let made = hub
        .call(
            &mut alice,
            "board_create",
            json!({"project":"audit","title":"Evidence <b>private</b>","request_id":"create<script>request</script>"}),
        )
        .await;
    assert!(!refused(&made), "{made}");
    let id = made["structuredContent"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let path = format!("/app/boards/audit/audit?ticket={id}&action=ticket.created");
    let (status, html) = page(&hub, &path, &alice).await;
    assert_eq!(status, 200, "{html}");
    assert!(html.contains("Evidence &lt;b&gt;private&lt;/b&gt;"));
    assert!(!html.contains("Evidence <b>private</b>"));
    assert!(html.contains("create&lt;script&gt;request&lt;/script&gt;"));
    assert!(!html.contains("create<script>request</script>"));
    assert_eq!(html.matches("<li id=\"event-").count(), 1);
    assert!(html.contains(&format!("/app/tickets/audit/{id}")));
    assert_eq!(page(&hub, &path, &bob).await.0, 404);
    assert_eq!(
        page(&hub, "/app/boards/audit/audit/changes?after=0", &bob)
            .await
            .0,
        404
    );
    assert_eq!(
        page(&hub, "/app/boards/audit/audit?before=-1", &alice)
            .await
            .0,
        400
    );
    assert_eq!(hub.get("/app/boards/audit/audit", None).await.status(), 303);
    let (_, verified) = page(&hub, "/app/boards/audit/audit?verify=true", &alice).await;
    assert!(verified.contains("id=\"audit-verified\""));
    for _ in 0..102 {
        let note = hub
            .call(
                &mut alice,
                "board_note",
                json!({"project":"audit","id":id,"body":"Page history"}),
            )
            .await;
        assert!(!refused(&note), "{note}");
    }
    let (_, newest) = page(
        &hub,
        &format!("/app/boards/audit/audit?ticket={id}"),
        &alice,
    )
    .await;
    assert_eq!(newest.matches("<li id=\"event-").count(), 100);
    let older = newest
        .split("id=\"audit-older\" href=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    let (status, older) = page(&hub, older, &alice).await;
    assert_eq!(status, 200, "{older}");
    assert_eq!(older.matches("<li id=\"event-").count(), 3);
    assert!(older.contains("Evidence &lt;b&gt;private&lt;/b&gt;"));
    let listed = hub.call(&mut alice, "board_events", json!({"project":"audit", "ticket":id, "action_prefix":"note.", "descending":true, "limit":2})).await;
    assert!(!refused(&listed), "{listed}");
    let events = listed["structuredContent"]["events"].as_array().unwrap();
    assert_eq!(events.len(), 2);
    let before = events[1]["seq"].as_i64().unwrap();
    assert!(events[0]["seq"].as_i64().unwrap() > before);
    let previous = hub.call(&mut alice, "board_events", json!({"project":"audit", "ticket":id, "action_prefix":"note.", "actor":events[0]["actor"], "before_seq":before, "descending":true, "limit":2})).await;
    assert!(!refused(&previous), "{previous}");
    assert!(
        previous["structuredContent"]["events"][0]["seq"]
            .as_i64()
            .unwrap()
            < before
    );
    let (_, none) = page(&hub, "/app/boards/audit/audit?actor=absent", &alice).await;
    assert_eq!(none.matches("<li id=\"event-").count(), 0);
    let changed = hub
        .get(
            "/app/boards/audit/audit/changes?after=0",
            Some(&alice.cookie),
        )
        .await;
    assert_eq!(changed.status(), 200);
    assert_eq!(changed.headers()["cache-control"], "no-store");
    assert_eq!(
        changed.json::<serde_json::Value>().await.unwrap(),
        json!({"changed":true})
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_waiting_audit_request_rechecks_a_signed_out_browser() {
    let hub = or_skip!(hosted());
    let mut person = hub.first_connect("Claude").await;
    let bound = hub
        .call(&mut person, "board_bind", json!({"project":"audit"}))
        .await;
    assert!(!refused(&bound), "{bound}");
    let latest = hub
        .call(
            &mut person,
            "board_events",
            json!({"project":"audit","descending":true,"limit":1}),
        )
        .await;
    let head = latest["structuredContent"]["events"][0]["seq"]
        .as_i64()
        .unwrap();
    let client = hub.http.clone();
    let url = format!("{}/app/boards/audit/audit/changes?after={head}", hub.url);
    let cookie = person.cookie.clone();
    let mut waiting = tokio::spawn(async move {
        client
            .get(url)
            .header("cookie", cookie)
            .send()
            .await
            .unwrap()
    });
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(200), &mut waiting)
            .await
            .is_err()
    );
    assert_eq!(
        hub.form("/auth/logout", Some(&person.cookie), &[])
            .await
            .status(),
        303
    );
    let made = hub
        .call(
            &mut person,
            "board_create",
            json!({"project":"audit","title":"After sign-out"}),
        )
        .await;
    assert!(!refused(&made), "{made}");
    let answer = tokio::time::timeout(std::time::Duration::from_secs(3), waiting)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(answer.status(), 303);
    assert!(
        answer.headers()["location"]
            .to_str()
            .unwrap()
            .starts_with("/auth/login")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn audit_reports_storage_failure_and_a_broken_chain() {
    let hub = or_skip!(hosted());
    let mut person = hub.first_connect("Claude").await;
    let bound = hub
        .call(&mut person, "board_bind", json!({"project":"audit"}))
        .await;
    assert!(!refused(&bound), "{bound}");
    sqlx::query("REVOKE SELECT ON devagotchi.board_events FROM devagotchi_hub")
        .execute(hub.accounts.store().admin())
        .await
        .unwrap();
    assert_eq!(page(&hub, "/app/boards/audit/audit", &person).await.0, 503);
    assert_eq!(
        page(&hub, "/app/boards/audit/audit/changes?after=0", &person)
            .await
            .0,
        503
    );
    sqlx::query("GRANT SELECT ON devagotchi.board_events TO devagotchi_hub")
        .execute(hub.accounts.store().admin())
        .await
        .unwrap();
    sqlx::query("ALTER TABLE devagotchi.board_events DISABLE TRIGGER board_events_append_only")
        .execute(hub.accounts.store().admin())
        .await
        .unwrap();
    sqlx::query("UPDATE devagotchi.board_events SET actor='tampered' WHERE seq=1")
        .execute(hub.accounts.store().admin())
        .await
        .unwrap();
    let (status, html) = page(&hub, "/app/boards/audit/audit?verify=true", &person).await;
    assert_eq!(status, 200, "{html}");
    assert!(html.contains("id=\"audit-broken\""));
    assert!(!html.contains("id=\"audit-verified\""));
}
