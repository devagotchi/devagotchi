use crate::tenancy::web::tests::operator::{hub_on, operator_cookie};
use crate::{
    ops::audit::{AuditEntry, HistoryFilter},
    tenancy::tests::tenants::Hosted,
};

async fn record(hub: &Hosted, n: usize) -> u64 {
    hub.hub
        .audit
        .record(&AuditEntry {
            subject: if n == 0 { "actor<one>" } else { "actor-two" },
            via: "test",
            tool: if n == 0 {
                "admin.%_saved"
            } else if n == 1 {
                "admin.other_saved"
            } else {
                "operation_start"
            },
            broker: "machine<one>",
            operation_id: &format!("<b>op-{n}</b>"),
            outcome: if n == 0 { "denied" } else { "ok" },
        })
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owner_history_filters_pages_verifies_and_hides_from_everyone_else() {
    let hub = or_skip!(hub_on(None, None));
    let owner = operator_cookie(&hub).await;
    let other = hub.first_connect("Claude").await;
    for n in 0..103 {
        record(&hub, n).await;
    }
    for cookie in [None, Some(other.cookie.as_str())] {
        for path in [
            "/admin/audit",
            "/admin/audit/changes?after=0",
            "/admin/audit?verify=true",
        ] {
            assert_eq!(hub.get(path, cookie).await.status(), 404);
        }
    }
    let (status, settings) = Hosted::text(hub.get("/admin", Some(&owner)).await).await;
    assert_eq!(status, 200);
    assert!(settings.contains("href=\"/admin/audit\""));
    let (status, html) =
        Hosted::text(hub.get("/admin/audit?verify=true", Some(&owner)).await).await;
    assert_eq!(status, 200, "{html}");
    assert_eq!(html.matches("<li id=\"event-").count(), 100);
    assert!(html.contains("id=\"audit-verified\""));
    assert!(html.contains("&lt;b&gt;op-102&lt;/b&gt;"));
    assert!(!html.contains("<b>op-102</b>"));
    let older = html
        .split("id=\"audit-older\" href=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    let (_, older) = Hosted::text(hub.get(older, Some(&owner)).await).await;
    assert!(older.contains("&lt;b&gt;op-0&lt;/b&gt;"));
    let before = hub.hub.audit.last_seq().await.unwrap();
    let literal = hub
        .hub
        .audit
        .history(&HistoryFilter {
            action_prefix: Some("admin.%_".into()),
            limit: 100,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(
        literal.len(),
        1,
        "percent and underscore are literal, not SQL wildcards"
    );
    let filter = HistoryFilter {
        subject: Some("actor<one>".into()),
        action_prefix: Some("admin.%_*".into()),
        broker: Some("machine<one>".into()),
        outcome: Some("denied".into()),
        limit: 1,
        ..Default::default()
    };
    let filtered = hub.hub.audit.history(&filter).await.unwrap();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].operation_id, "<b>op-0</b>");
    assert!(
        hub.hub
            .audit
            .history(&HistoryFilter {
                before: Some(filtered[0].seq),
                ..filter
            })
            .await
            .unwrap()
            .is_empty()
    );
    let (status, filtered) = Hosted::text(hub.get("/admin/audit?actor=actor%3Cone%3E&action=admin.%25_&machine=machine%3Cone%3E&outcome=denied", Some(&owner)).await).await;
    assert_eq!(status, 200);
    assert_eq!(filtered.matches("<li id=\"event-").count(), 1);
    assert!(filtered.contains("actor&lt;one&gt;"));
    for path in [
        "/admin/audit?before=0",
        "/admin/audit?before=-1",
        "/admin/audit/changes?after=-1",
    ] {
        assert_eq!(hub.get(path, Some(&owner)).await.status(), 400);
    }
    let changed = hub.get("/admin/audit/changes?after=0", Some(&owner)).await;
    assert_eq!(changed.status(), 200);
    assert_eq!(changed.headers()["cache-control"], "no-store");
    assert_eq!(
        changed.json::<serde_json::Value>().await.unwrap()["changed"],
        true
    );
    assert_eq!(
        hub.hub.audit.last_seq().await.unwrap(),
        before,
        "reading writes no audit events"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owner_history_surfaces_store_failure_and_chain_tampering() {
    let hub = or_skip!(hub_on(None, None));
    let owner = operator_cookie(&hub).await;
    let seq = record(&hub, 0).await;
    sqlx::query("REVOKE SELECT ON devagotchi.audit_entries FROM devagotchi_hub")
        .execute(hub.hub.audit.store().admin())
        .await
        .unwrap();
    for path in ["/admin/audit", "/admin/audit/changes?after=0"] {
        assert_eq!(hub.get(path, Some(&owner)).await.status(), 503);
    }
    sqlx::query("GRANT SELECT ON devagotchi.audit_entries TO devagotchi_hub")
        .execute(hub.hub.audit.store().admin())
        .await
        .unwrap();
    sqlx::query("UPDATE devagotchi.audit_entries SET outcome='tampered' WHERE seq=$1")
        .bind(seq as i64)
        .execute(hub.hub.audit.store().admin())
        .await
        .unwrap();
    let (status, html) =
        Hosted::text(hub.get("/admin/audit?verify=true", Some(&owner)).await).await;
    assert_eq!(status, 200);
    assert!(html.contains("id=\"audit-broken\""));
    assert!(!html.contains("id=\"audit-verified\""));
    sqlx::query("REVOKE SELECT ON devagotchi.auth_sessions FROM devagotchi_hub")
        .execute(hub.accounts.store().admin())
        .await
        .unwrap();
    assert_eq!(hub.get("/admin/audit", Some(&owner)).await.status(), 503);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owner_history_wait_wakes_for_new_events_and_refuses_a_revoked_session() {
    let hub = or_skip!(hub_on(None, None));
    let owner = operator_cookie(&hub).await;
    for revoke in [false, true] {
        let head = hub.hub.audit.last_seq().await.unwrap();
        let request = hub
            .http
            .get(format!("{}/admin/audit/changes?after={head}", hub.url))
            .header("cookie", &owner);
        let mut waiting = tokio::spawn(async move { request.send().await.unwrap() });
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(200), &mut waiting)
                .await
                .is_err()
        );
        if revoke {
            assert_eq!(
                hub.form("/auth/logout", Some(&owner), &[]).await.status(),
                303
            );
        }
        record(&hub, 1).await;
        let response = tokio::time::timeout(std::time::Duration::from_secs(3), waiting)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(response.status(), if revoke { 404 } else { 200 });
    }
}
