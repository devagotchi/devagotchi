//! The person's own page end to end over HTTP, the whole router as it serves (`web/app/me.rs`,
//! `crate::preferences`, `web/approvals.rs`): the frame says who is signed in, what a person
//! sets changes what approvals offer them, a kind set to always ask stops what they allowed
//! and cannot be allowed again, and only their own page changes any of it. On SQLite, and on
//! Postgres when the test server is set (CI).

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

async fn ask(hub: &Hosted, ai: &mut Person, kind: &str) -> Value {
    let answer = hub
        .call(
            ai,
            "approval_request",
            json!({
                "project": "site", "title": format!("Do a {kind}"),
                "what": "a thing", "why": "a reason", "undo": "the way back",
                "kind": kind
            }),
        )
        .await;
    answer["structuredContent"].clone()
}

async fn decide(hub: &Hosted, person: &Person, asked: &Value, allow: Option<&str>) {
    let version = asked["version"].as_i64().unwrap().to_string();
    let mut fields = vec![
        ("project", "site"),
        ("id", asked["id"].as_str().unwrap()),
        ("version", version.as_str()),
        ("verdict", "approved"),
    ];
    if let Some(days) = allow {
        fields.push(("allow_days", days));
    }
    let decided = form(hub, "/approvals/decide", person, &fields)
        .send()
        .await
        .unwrap();
    assert_eq!(decided.status(), 303);
}

async fn page(hub: &Hosted, path: &str, person: &Person) -> String {
    let (status, html) = Hosted::text(hub.get(path, Some(&person.cookie)).await).await;
    assert_eq!(status, 200, "{path}: {html}");
    html
}

async fn save(hub: &Hosted, person: &Person, fields: &[(&str, &str)]) -> reqwest::Response {
    form(hub, "/app/me", person, fields).send().await.unwrap()
}

const USUAL: [(&str, &str); 4] = [
    ("name", ""),
    ("theme", "system"),
    ("allow_days", "30"),
    ("machine_pushes", "on"),
];

async fn run(store: Option<&str>) {
    let hub = hosted_with(store, |_, _| json!({})).await;
    let mut person = hub.first_connect("Claude").await;
    let bound = hub
        .call(&mut person, "board_bind", json!({ "project": "site" }))
        .await;
    assert_eq!(bound["structuredContent"]["project"], "site", "{bound}");

    // Signed out, the page is the sign-in that comes back to it.
    let out = hub.get("/app/me", None).await;
    assert_eq!(out.status(), 303);
    assert!(
        out.headers()["location"]
            .to_str()
            .unwrap()
            .starts_with("/auth/login")
    );

    // The frame: the person's mark, five tabs with this one current, and no bar of links.
    let me = page(&hub, "/app/me", &person).await;
    assert!(me.contains("id=\"me-link\""), "{me}");
    assert!(me.contains("id=\"prefs\""), "{me}");
    assert!(
        me.contains("action=\"/auth/logout\""),
        "sign out lives here: {me}"
    );
    assert!(
        me.contains("href=\"/app/me\" aria-current=\"page\""),
        "{me}"
    );
    assert_eq!(me.matches("class=\"tab-icon\"").count(), 5, "{me}");
    assert!(!me.contains("data-theme"), "{me}");

    // A request that names its kind offers "allow" for the usual 30 days. The count on the
    // tab comes from one small answer that is never kept.
    let first = ask(&hub, &mut person, "restart").await;
    let approvals = page(&hub, "/approvals", &person).await;
    assert!(
        approvals.contains("Approve, and allow this exact restart for 30 days"),
        "{approvals}"
    );
    let attention = hub.get("/app/attention", Some(&person.cookie)).await;
    assert_eq!(attention.headers()["cache-control"], "no-store");
    let attention: Value = attention.json().await.unwrap();
    assert_eq!(attention["approvals"], 1);
    decide(&hub, &person, &first, None).await;

    // Set: a name, dark, 7 days, and two kinds that always ask.
    let saved = save(
        &hub,
        &person,
        &[
            ("name", "Ada Lovelace"),
            ("theme", "dark"),
            ("allow_days", "7"),
            ("offer_ahead", "on"),
            ("always_ask", "Deploy, rotate"),
            ("machine_pushes", "on"),
        ],
    )
    .await;
    assert_eq!(saved.status(), 303);
    assert_eq!(saved.headers()["location"], "/app/me?saved=1&stopped=0");

    // The frame and Home say it back, and every framed page wears the theme.
    let me = page(&hub, "/app/me?saved=1&stopped=0", &person).await;
    assert!(
        me.contains("<html lang=\"en\" data-theme=\"dark\">"),
        "{me}"
    );
    assert!(me.contains("<span class=\"avatar\">AL</span>"), "{me}");
    assert!(me.contains("value=\"deploy, rotate\""), "{me}");
    assert!(me.contains("id=\"flash\""), "{me}");
    let home = page(&hub, "/app", &person).await;
    assert!(home.contains("Hi Ada"), "{home}");

    // "Allow" now lasts 7 days, and the standing approval it makes does too.
    let second = ask(&hub, &mut person, "restart").await;
    let approvals = page(&hub, "/approvals", &person).await;
    assert!(
        approvals.contains("Approve, and allow this exact restart for 7 days"),
        "{approvals}"
    );
    decide(&hub, &person, &second, Some("7")).await;
    let at_once = ask(&hub, &mut person, "restart").await;
    assert_eq!(at_once["status"], "approved", "{at_once}");
    let standing = &at_once["standing"];
    let lasts = standing["expires_at"].as_i64().unwrap() - standing["granted_at"].as_i64().unwrap();
    assert_eq!(lasts, 7 * 86_400_000);

    // A kind that always asks: no allow button, a line that says why, and neither a stale
    // form nor a direct grant allows it.
    let deploy = ask(&hub, &mut person, "deploy").await;
    assert_eq!(deploy["status"], "pending");
    let approvals = page(&hub, "/approvals", &person).await;
    assert!(
        approvals.contains("You always decide deploy yourself."),
        "{approvals}"
    );
    assert!(
        !approvals.contains("Approve, and allow this exact deploy"),
        "{approvals}"
    );
    decide(&hub, &person, &deploy, Some("30")).await;
    let again = ask(&hub, &mut person, "deploy").await;
    assert_eq!(again["status"], "pending", "{again}");
    let direct = form(
        &hub,
        "/approvals/standing",
        &person,
        &[("project", "site"), ("kind", "deploy"), ("what", "a thing")],
    )
    .send()
    .await
    .unwrap();
    assert_eq!(direct.status(), 409);
    decide(&hub, &person, &again, None).await;

    // Offers: a kind approved three times is offered, and not once offers are off.
    for _ in 0..3 {
        let asked = ask(&hub, &mut person, "edit-file").await;
        decide(&hub, &person, &asked, None).await;
    }
    let approvals = page(&hub, "/approvals", &person).await;
    assert!(
        approvals.contains("Approve it automatically for 7 days"),
        "{approvals}"
    );
    let saved = save(
        &hub,
        &person,
        &[
            ("name", "Ada Lovelace"),
            ("theme", "dark"),
            ("allow_days", "7"),
            ("always_ask", "deploy, rotate"),
            ("machine_pushes", "on"),
        ],
    )
    .await;
    assert_eq!(saved.status(), 303);
    let approvals = page(&hub, "/approvals", &person).await;
    assert!(
        !approvals.contains("Approve it automatically"),
        "offer_ahead was left unticked: {approvals}"
    );

    // Setting a kind that is allowed to always ask stops what this person allowed.
    let saved = save(
        &hub,
        &person,
        &[
            ("theme", "dark"),
            ("allow_days", "7"),
            ("offer_ahead", "on"),
            ("always_ask", "deploy, rotate, restart"),
        ],
    )
    .await;
    assert_eq!(saved.headers()["location"], "/app/me?saved=1&stopped=1");
    let waits = ask(&hub, &mut person, "restart").await;
    assert_eq!(waits["status"], "pending", "{waits}");

    // Refused: what is not a choice, and another site's form, even with the session.
    for bad in [
        [
            ("name", ""),
            ("theme", "system"),
            ("allow_days", "45"),
            ("machine_pushes", "on"),
        ],
        [
            ("name", ""),
            ("theme", "mauve"),
            ("allow_days", "30"),
            ("machine_pushes", "on"),
        ],
    ] {
        let refused = save(&hub, &person, &bad).await;
        assert_eq!(refused.status(), 400);
    }
    let cross = hub
        .http
        .post(format!("{}/app/me", hub.url))
        .header(axum::http::header::COOKIE, &person.cookie)
        .header(axum::http::header::ORIGIN, "https://elsewhere.example")
        .form(&USUAL)
        .send()
        .await
        .unwrap();
    assert_eq!(cross.status(), 403);
    let me = page(&hub, "/app/me", &person).await;
    assert!(me.contains("data-theme=\"dark\""), "nothing changed: {me}");

    // Someone else keeps their own: nothing of this person's shows.
    let other = hub.first_connect("Codex").await;
    let theirs = page(&hub, "/app/me", &other).await;
    assert!(!theirs.contains("data-theme"), "{theirs}");
    assert!(!theirs.contains("Ada"), "{theirs}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn what_a_person_sets_is_what_their_approvals_do() {
    run(None).await;
    if let Some(url) = crate::store::testing::empty_postgres().await {
        run(Some(&url)).await;
    }
}
