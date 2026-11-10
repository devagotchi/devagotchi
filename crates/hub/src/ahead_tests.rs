//! Approving ahead of time end to end over HTTP, the whole router as it serves
//! (`board::approval`, `web/approvals.rs`, `approval_request`): a person's AI asks to
//! restart a machine and the person taps "Approve, and allow"; the AI's next restart there
//! is approved the moment it asks and no phone is notified; the approvals page lists it with
//! Stop, and once stopped the next one waits for the person again. Only the person's page
//! changes any of it. On SQLite, and on Postgres when the test server is set (CI).

use serde_json::{Value, json};

use crate::tenant_tests::{Hosted, Person, hosted_with};

fn form(
    hub: &Hosted,
    path: &str,
    person: &Person,
    fields: &[(&str, &str)],
) -> reqwest::RequestBuilder {
    hub.http
        .post(format!("{}{path}", hub.url))
        .header(axum::http::header::COOKIE, &person.cookie)
        .header(axum::http::header::ORIGIN, &hub.url)
        .form(fields)
}

async fn restart(hub: &Hosted, ai: &mut Person) -> Value {
    let answer = hub
        .call(
            ai,
            "approval_request",
            json!({
                "project": "site", "title": "Restart the api on dabo",
                "what": "systemctl restart api", "why": "a new build", "undo": "start the old build",
                "kind": "restart", "target": "dabo"
            }),
        )
        .await;
    answer["structuredContent"].clone()
}

async fn run(store: Option<&str>) {
    let hub = hosted_with(store, |_, _| json!({})).await;
    let mut alice = hub.first_connect("Claude").await;
    let bound = hub
        .call(&mut alice, "board_bind", json!({ "project": "site" }))
        .await;
    assert_eq!(bound["structuredContent"]["project"], "site", "{bound}");

    // The AI asks; it waits for the person, and the approvals page names its kind.
    let first = restart(&hub, &mut alice).await;
    assert_eq!(first["status"], "pending", "{first}");
    assert_eq!(first["kind"], "restart");
    assert!(first.get("standing").is_none());
    let (status, page) = Hosted::text(hub.get("/approvals", Some(&alice.cookie)).await).await;
    assert_eq!(status, 200);
    assert!(
        page.contains("Approve, and allow restart on dabo for 30 days"),
        "{page}"
    );

    // Approved, with restarts on dabo allowed for 30 days.
    let version = first["version"].as_i64().unwrap().to_string();
    let decided = form(
        &hub,
        "/approvals/decide",
        &alice,
        &[
            ("project", "site"),
            ("id", first["id"].as_str().unwrap()),
            ("version", &version),
            ("verdict", "approved"),
            ("allow_days", "30"),
        ],
    )
    .send()
    .await
    .unwrap();
    assert_eq!(decided.status(), 303);

    // The next restart on dabo: approved the moment it is asked, nobody notified.
    let second = restart(&hub, &mut alice).await;
    assert_eq!(second["status"], "approved", "{second}");
    assert_eq!(second["phones"], 0);
    let standing = second["standing"].clone();
    assert_eq!(standing["kind"], "restart");
    assert_eq!(standing["target"], "dabo");
    assert!(
        second["decided_by"]
            .as_str()
            .unwrap()
            .starts_with("standing:st-")
    );

    // The page lists it with Stop; stopping it makes the next one wait for a person again.
    let (_, page) = Hosted::text(hub.get("/approvals", Some(&alice.cookie)).await).await;
    let id = standing["id"].as_str().unwrap();
    assert!(page.contains(&format!("id=\"standing-{id}\"")), "{page}");
    let stopped = form(
        &hub,
        "/approvals/standing/revoke",
        &alice,
        &[("project", "site"), ("id", id)],
    )
    .send()
    .await
    .unwrap();
    assert_eq!(stopped.status(), 303);
    let third = restart(&hub, &mut alice).await;
    assert_eq!(third["status"], "pending", "{third}");

    // Another site's form changes nothing, though the browser sends the session.
    let cross = hub
        .http
        .post(format!("{}/approvals/standing", hub.url))
        .header(axum::http::header::COOKIE, &alice.cookie)
        .header(axum::http::header::ORIGIN, "https://elsewhere.example")
        .form(&[("project", "site"), ("kind", "restart"), ("target", "dabo")])
        .send()
        .await
        .unwrap();
    assert_eq!(cross.status(), 403);
    let fourth = restart(&hub, &mut alice).await;
    assert_eq!(
        fourth["status"], "pending",
        "nothing was granted from another site"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_kind_approved_ahead_of_time_skips_the_phone_until_the_person_stops_it() {
    run(None).await;
    if let Some(url) = crate::store::testing::empty_postgres().await {
        run(Some(&url)).await;
    }
}
