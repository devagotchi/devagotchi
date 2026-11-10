//! Organisations end to end over HTTP, the whole router as it serves (`orgs`, `orgs::api`,
//! `board::mcp::Hub::board_access`): a person makes an organisation and invites another,
//! a project is made, the invited person's AI works it, and a change of role holds from that
//! AI's next call: demoted to viewer it reads and changes nothing, removed it reaches
//! nothing. Only people change who works what; another site's form changes nothing. On
//! SQLite, and on Postgres when the test server is set (CI).

use serde_json::{Value, json};

use crate::tenant_tests::{Hosted, Person, hosted_with, refused};

async fn api(
    hub: &Hosted,
    method: reqwest::Method,
    path: &str,
    person: &Person,
    as_ai: bool,
    body: Option<Value>,
) -> (u16, Value) {
    let mut request = hub.http.request(method, format!("{}{path}", hub.url));
    request = if as_ai {
        request.bearer_auth(&person.token)
    } else {
        request
            .header(axum::http::header::COOKIE, &person.cookie)
            .header(axum::http::header::ORIGIN, &hub.url)
    };
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request.send().await.unwrap();
    let status = response.status().as_u16();
    let text = response.text().await.unwrap();
    (
        status,
        serde_json::from_str(&text).unwrap_or(Value::String(text)),
    )
}

async fn subject_of(hub: &Hosted, person: &mut Person) -> String {
    let me = hub.call(person, "whoami", json!({})).await;
    me["structuredContent"]["subject"]
        .as_str()
        .expect("whoami names the subject")
        .to_string()
}

async fn run(store: Option<&str>) {
    let hub = hosted_with(store, |_, _| json!({})).await;
    let mut alice = hub.first_connect("Claude").await;
    let mut bob = hub.first_connect("Codex").await;
    let bob_id = subject_of(&hub, &mut bob).await;
    use reqwest::Method;

    // Alice makes an organisation from her browser; her AI's token may not.
    let (status, made) = api(
        &hub,
        Method::POST,
        "/api/v1/orgs",
        &alice,
        false,
        Some(json!({ "org": "acme", "name": "Acme Corp" })),
    )
    .await;
    assert_eq!(status, 201, "{made}");
    assert_eq!(made["role"], "owner");
    let (status, refusal) = api(
        &hub,
        Method::POST,
        "/api/v1/orgs",
        &alice,
        true,
        Some(json!({ "org": "acme-ai", "name": "By an AI" })),
    )
    .await;
    assert_eq!(
        (status, refusal["code"].as_str()),
        (403, Some("people_only"))
    );

    // Another site's form changes nothing, though the browser sends Alice's session.
    let cross = hub
        .http
        .post(format!("{}/api/v1/orgs", hub.url))
        .header(axum::http::header::COOKIE, &alice.cookie)
        .header(axum::http::header::ORIGIN, "https://elsewhere.example")
        .json(&json!({ "org": "evil", "name": "Evil" }))
        .send()
        .await
        .unwrap();
    assert_eq!(cross.status(), 403);

    // A project of the organisation, and an invite Bob takes.
    let (status, project) = api(
        &hub,
        Method::POST,
        "/api/v1/orgs/acme/projects",
        &alice,
        false,
        Some(json!({ "project": "site" })),
    )
    .await;
    assert_eq!(status, 201, "{project}");
    assert_eq!(project["project"], "acme/site");
    let (status, invite) = api(
        &hub,
        Method::POST,
        "/api/v1/orgs/acme/invites",
        &alice,
        false,
        Some(json!({ "role": "member", "note": "for Bob" })),
    )
    .await;
    assert_eq!(status, 201, "{invite}");
    let token = invite["token"].as_str().unwrap().to_string();
    assert!(
        invite["url"]
            .as_str()
            .unwrap()
            .ends_with(&format!("/invite#{token}"))
    );
    let (status, joined) = api(
        &hub,
        Method::POST,
        "/api/v1/invites/accept",
        &bob,
        false,
        Some(json!({ "token": token })),
    )
    .await;
    assert_eq!(
        (status, joined["role"].as_str()),
        (200, Some("member")),
        "{joined}"
    );
    let (status, again) = api(
        &hub,
        Method::POST,
        "/api/v1/invites/accept",
        &bob,
        false,
        Some(json!({ "token": token })),
    )
    .await;
    assert_eq!((status, again["code"].as_str()), (409, Some("spent")));

    // Bob's AI sees the organisation and its project, and works it.
    let me = hub.call(&mut bob, "whoami", json!({})).await;
    let seen = &me["structuredContent"];
    assert!(
        seen["boards"]
            .as_array()
            .unwrap()
            .contains(&json!("acme/site")),
        "{seen}"
    );
    assert_eq!(seen["orgs"][0]["org"], "acme");
    assert_eq!(seen["orgs"][0]["role"], "member");
    let made = hub
        .call(
            &mut bob,
            "board_create",
            json!({ "project": "acme/site", "title": "Ship the landing page" }),
        )
        .await;
    assert!(!refused(&made), "{made}");

    // Demoted to viewer: Bob's AI reads the board and changes nothing, from its next call.
    let (status, _) = api(
        &hub,
        Method::PUT,
        &format!("/api/v1/orgs/acme/members/{bob_id}"),
        &alice,
        false,
        Some(json!({ "role": "viewer" })),
    )
    .await;
    assert_eq!(status, 200);
    let listed = hub
        .call(&mut bob, "board_list", json!({ "project": "acme/site" }))
        .await;
    assert!(!refused(&listed), "{listed}");
    let written = hub
        .call(
            &mut bob,
            "board_create",
            json!({ "project": "acme/site", "title": "Should not land" }),
        )
        .await;
    assert!(refused(&written), "{written}");
    assert!(written.to_string().contains("read_only"), "{written}");

    // A viewer manages nobody; the organisation shows members to its members.
    let (status, _) = api(
        &hub,
        Method::POST,
        "/api/v1/orgs/acme/invites",
        &bob,
        false,
        Some(json!({ "role": "viewer" })),
    )
    .await;
    assert_eq!(status, 403);
    let (status, shown) = api(&hub, Method::GET, "/api/v1/orgs/acme", &bob, false, None).await;
    assert_eq!(status, 200, "{shown}");
    assert_eq!(shown["role"], "viewer");
    assert_eq!(shown["members"].as_array().unwrap().len(), 2);
    assert_eq!(shown["projects"][0]["project"], "site");

    // Removed: Bob's AI reaches nothing of the organisation from its next call, and the
    // organisation is no longer Bob's to read.
    let (status, _) = api(
        &hub,
        Method::DELETE,
        &format!("/api/v1/orgs/acme/members/{bob_id}"),
        &alice,
        false,
        None,
    )
    .await;
    assert_eq!(status, 200);
    let gone = hub
        .call(&mut bob, "board_list", json!({ "project": "acme/site" }))
        .await;
    assert!(refused(&gone), "{gone}");
    assert!(gone.to_string().contains("unknown_board"), "{gone}");
    let me = hub.call(&mut bob, "whoami", json!({})).await;
    assert!(
        !me["structuredContent"]["boards"]
            .as_array()
            .unwrap()
            .contains(&json!("acme/site"))
    );
    let (status, _) = api(&hub, Method::GET, "/api/v1/orgs/acme", &bob, false, None).await;
    assert_eq!(status, 404);

    // The last owner stays.
    let alice_id = subject_of(&hub, &mut alice).await;
    let (status, kept) = api(
        &hub,
        Method::DELETE,
        &format!("/api/v1/orgs/acme/members/{alice_id}"),
        &alice,
        false,
        None,
    )
    .await;
    assert_eq!((status, kept["code"].as_str()), (409, Some("last_owner")));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_change_of_role_holds_from_the_ais_next_call() {
    run(None).await;
    if let Some(url) = crate::store::testing::empty_postgres().await {
        run(Some(&url)).await;
    }
}

/// The organisation pages in a browser's place (`web/orgs.rs`): forms from this hub only, an
/// invite shown once and taken on `/invite`, the organisation's board in the app, and a
/// person made viewer refused on the ticket page's actions.
async fn pages(store: Option<&str>) {
    let hub = hosted_with(store, |_, _| json!({})).await;
    let alice = hub.first_connect("Claude").await;
    let mut bob = hub.first_connect("Codex").await;
    let bob_id = subject_of(&hub, &mut bob).await;
    let form = |path: &str, cookie: &str, fields: &[(&str, &str)]| {
        hub.http
            .post(format!("{}{path}", hub.url))
            .header(axum::http::header::COOKIE, cookie)
            .header(axum::http::header::ORIGIN, &hub.url)
            .form(fields)
            .send()
    };
    let location = |response: &reqwest::Response| {
        response.headers()[axum::http::header::LOCATION]
            .to_str()
            .unwrap()
            .to_string()
    };

    let (status, list) = Hosted::text(hub.get("/orgs", Some(&alice.cookie)).await).await;
    assert_eq!(status, 200);
    assert!(list.contains("id=\"new-org\""), "{list}");
    let made = form("/orgs", &alice.cookie, &[("org", "acme"), ("name", "Acme")])
        .await
        .unwrap();
    assert_eq!(made.status(), 303);
    assert_eq!(location(&made), "/orgs/acme?done=1");
    let cross = hub
        .http
        .post(format!("{}/orgs", hub.url))
        .header(axum::http::header::COOKIE, &alice.cookie)
        .header(axum::http::header::ORIGIN, "https://elsewhere.example")
        .form(&[("org", "evil"), ("name", "Evil")])
        .send()
        .await
        .unwrap();
    assert_eq!(cross.status(), 403);

    let made = form(
        "/orgs/acme/projects",
        &alice.cookie,
        &[("project", "site"), ("visibility", "org")],
    )
    .await
    .unwrap();
    assert_eq!(location(&made), "/orgs/acme?done=1");
    let (status, page) = Hosted::text(hub.get("/orgs/acme", Some(&alice.cookie)).await).await;
    assert_eq!(status, 200);
    for part in [
        "id=\"projects\"",
        "id=\"members\"",
        "id=\"teams\"",
        "id=\"invites\"",
        "/app/boards/acme%2Fsite",
    ] {
        assert!(page.contains(part), "{part}: {page}");
    }

    // The invite: its link shown on the one answer, never in a URL the browser keeps.
    let invited = form(
        "/orgs/acme/invites",
        &alice.cookie,
        &[("role", "member"), ("note", "Bob")],
    )
    .await
    .unwrap();
    assert_eq!(invited.status(), 200);
    assert_eq!(
        invited.headers()[axum::http::header::CACHE_CONTROL],
        "no-store"
    );
    let shown = invited.text().await.unwrap();
    let link = shown
        .split("id=\"invite-link\" value=\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .expect("the invite link")
        .to_string();
    let token = link.split_once('#').unwrap().1.to_string();
    let (status, join) = Hosted::text(hub.get("/invite", Some(&bob.cookie)).await).await;
    assert_eq!(status, 200);
    assert!(join.contains("id=\"invite-form\""), "{join}");
    let (status, signed_out) = Hosted::text(hub.get("/invite", None).await).await;
    assert_eq!(status, 200);
    assert!(signed_out.contains("id=\"invite-sign-in\""), "{signed_out}");
    let joined = form("/invite", &bob.cookie, &[("token", &token)])
        .await
        .unwrap();
    assert_eq!(location(&joined), "/orgs/acme?done=1");
    let (status, again) = Hosted::text(
        form("/invite", &bob.cookie, &[("token", &token)])
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(status, 409, "{again}");

    // Bob's board in the app, and its ticket page; then made viewer, he acts on nothing.
    let (status, board) =
        Hosted::text(hub.get("/app/boards/acme%2Fsite", Some(&bob.cookie)).await).await;
    assert_eq!(status, 200, "{board}");
    let made = hub
        .call(
            &mut bob,
            "board_create",
            json!({ "project": "acme/site", "title": "Landing page" }),
        )
        .await;
    let ticket = made["structuredContent"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let here = format!("/app/tickets/acme%2Fsite/{ticket}");
    let (status, _) = Hosted::text(hub.get(&here, Some(&bob.cookie)).await).await;
    assert_eq!(status, 200);
    let commented = form(
        &format!("{here}/comment"),
        &bob.cookie,
        &[("version", "1"), ("body", "on it")],
    )
    .await
    .unwrap();
    assert!(
        location(&commented).contains("done=comment"),
        "{}",
        location(&commented)
    );
    let demoted = form(
        "/orgs/acme/members",
        &alice.cookie,
        &[("subject", &bob_id), ("role", "viewer")],
    )
    .await
    .unwrap();
    assert_eq!(location(&demoted), "/orgs/acme?done=1");
    let refused = form(
        &format!("{here}/comment"),
        &bob.cookie,
        &[("version", "2"), ("body", "not now")],
    )
    .await
    .unwrap();
    assert!(
        location(&refused).contains("done=no"),
        "{}",
        location(&refused)
    );
    assert!(
        location(&refused).contains("viewer"),
        "{}",
        location(&refused)
    );
    let (status, as_viewer) = Hosted::text(hub.get("/orgs/acme", Some(&bob.cookie)).await).await;
    assert_eq!(status, 200);
    assert!(
        !as_viewer.contains("id=\"invites\""),
        "viewers see no invites"
    );

    // Bob leaves: back to his list, and the organisation is no longer his to open.
    let left = form(
        "/orgs/acme/members/remove",
        &bob.cookie,
        &[("subject", &bob_id)],
    )
    .await
    .unwrap();
    assert_eq!(location(&left), "/orgs?done=1");
    let (status, _) = Hosted::text(hub.get("/orgs/acme", Some(&bob.cookie)).await).await;
    assert_eq!(status, 404);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_organisation_pages_do_what_the_api_does_from_this_hubs_own_forms() {
    pages(None).await;
    if let Some(url) = crate::store::testing::empty_postgres().await {
        pages(Some(&url)).await;
    }
}
