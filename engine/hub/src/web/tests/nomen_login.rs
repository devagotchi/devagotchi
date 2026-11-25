//! Signing in with Nomen on the hub's pages: the round trip, its refusals and outages, where
//! a person returns to, and the session that outlasts a restart.

use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn signing_in_shows_the_account_and_machines_and_signing_out_ends_it() {
    let w = or_skip!(world());
    let first = w.get("/account", None).await;
    assert_eq!(first.status(), StatusCode::SEE_OTHER);
    assert_eq!(location(&first), "/auth/login?return_to=/account");

    let (login, state, code) = w.authorize("/account", "tok-owner", 3600).await;
    let back = w
        .get(
            &format!("/auth/callback?code={code}&state={state}"),
            Some(&login),
        )
        .await;
    assert_eq!(back.status(), StatusCode::SEE_OTHER);
    assert_eq!(location(&back), "/account");
    let session = set_cookie(&back, "dg_session").unwrap();
    assert!(
        session.contains("HttpOnly")
            && session.contains("SameSite=Lax")
            && session.contains("Max-Age=3600"),
        "{session}"
    );
    assert!(
        set_cookie(&back, "dg_login").unwrap().contains("Max-Age=0"),
        "the login cookie is cleared"
    );
    let session = cookie_pair(&session);
    assert!(
        !session.contains("tok-owner"),
        "the browser never holds the token"
    );

    let page = w.get("/account", Some(&session)).await;
    assert_eq!(page.status(), StatusCode::OK);
    let csp = page.headers()[header::CONTENT_SECURITY_POLICY]
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        csp.starts_with("default-src 'none'") && csp.contains("frame-ancestors 'none'"),
        "{csp}"
    );
    let html = page.text().await.unwrap();
    assert!(
        html.contains(OWNER) && html.contains("You own this hub"),
        "{html}"
    );
    for (machine, status) in [
        ("gold-ws", "offline"),
        ("gus", "offline"),
        ("lab", "not_enrolled"),
        ("old", "revoked"),
    ] {
        assert!(
            html.contains(&format!("<td>{machine}</td>")),
            "{machine} listed"
        );
        assert!(
            html.contains(&format!("status {status}")),
            "{machine} is {status}"
        );
    }

    let me: Value = w
        .get("/api/v1/me", Some(&session))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(me["subject"], OWNER);
    assert_eq!(me["owner"], true);
    let statuses: Vec<(String, String, String)> = me["machines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            (
                m["name"].as_str().unwrap().into(),
                m["kind"].as_str().unwrap().into(),
                m["status"].as_str().unwrap().into(),
            )
        })
        .collect();
    assert_eq!(
        statuses,
        [
            ("gold-ws", "local", "offline"),
            ("gus", "linked", "offline"),
            ("lab", "linked", "not_enrolled"),
            ("old", "linked", "revoked")
        ]
        .map(|(a, b, c)| (a.to_string(), b.to_string(), c.to_string()))
    );
    assert!(me["machines"][1]["enrolled_at"].as_u64().is_some());

    let out = w
        .http
        .post(format!("{}/auth/logout", w.url))
        .header(header::COOKIE, &session)
        .send()
        .await
        .unwrap();
    assert_eq!(out.status(), StatusCode::SEE_OTHER);
    assert_eq!(location(&out), "/signed-out");
    assert!(
        set_cookie(&out, "dg_session")
            .unwrap()
            .contains("Max-Age=0")
    );
    let after = w.get("/account", Some(&session)).await;
    assert_eq!(
        after.status(),
        StatusCode::SEE_OTHER,
        "the old session id no longer works"
    );
    let me = w.get("/api/v1/me", Some(&session)).await;
    assert_eq!(me.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_person_sees_only_their_granted_machines_by_session_or_bearer() {
    let w = or_skip!(world());
    let session = w.sign_in("tok-other").await;
    let me: Value = w
        .get("/api/v1/me", Some(&session))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(me["owner"], false);
    assert_eq!(
        me["machines"],
        json!([{"name": "gus", "kind": "linked", "status": "offline", "enrolled_at": me["machines"][0]["enrolled_at"]}])
    );
    let html = w
        .get("/account", Some(&session))
        .await
        .text()
        .await
        .unwrap();
    assert!(
        html.contains("<td>gus</td>")
            && !html.contains("gold-ws")
            && !html.contains("You own this hub")
    );

    let by_bearer: Value = w
        .http
        .get(format!("{}/api/v1/me", w.url))
        .bearer_auth("tok-other")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(by_bearer["subject"], OTHER);
    assert_eq!(by_bearer["via"], "dev");
    let bad = w
        .http
        .get(format!("{}/api/v1/me", w.url))
        .bearer_auth("nope")
        .send()
        .await
        .unwrap();
    assert_eq!(bad.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        w.get("/api/v1/me", None).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_callback_needs_this_browsers_state_once_and_the_right_verifier() {
    let w = or_skip!(world());
    let (login, state, code) = w.authorize("/account", "tok-owner", 3600).await;
    // No login cookie, or another browser's: refused, and the sign-in stays usable.
    let bare = w
        .get(&format!("/auth/callback?code={code}&state={state}"), None)
        .await;
    assert_eq!(bare.status(), StatusCode::BAD_REQUEST);
    assert!(set_cookie(&bare, "dg_session").is_none());
    let (other_login, _, _) = w.authorize("/account", "tok-owner", 3600).await;
    let crossed = w
        .get(
            &format!("/auth/callback?code={code}&state={state}"),
            Some(&other_login),
        )
        .await;
    assert_eq!(crossed.status(), StatusCode::BAD_REQUEST);
    // The right browser finishes; replaying the same callback does not sign in again.
    let ok = w
        .get(
            &format!("/auth/callback?code={code}&state={state}"),
            Some(&login),
        )
        .await;
    assert_eq!(ok.status(), StatusCode::SEE_OTHER);
    let replay = w
        .get(
            &format!("/auth/callback?code={code}&state={state}"),
            Some(&login),
        )
        .await;
    assert_eq!(replay.status(), StatusCode::BAD_REQUEST);
    assert!(set_cookie(&replay, "dg_session").is_none());

    // A code bound to another challenge (someone else's PKCE) is refused by Nomen.
    let (login, state, code) = w.authorize("/account", "tok-owner", 3600).await;
    w.fake
        .codes
        .lock()
        .unwrap()
        .get_mut(&code)
        .unwrap()
        .challenge = "not-this-one".into();
    let wrong = w
        .get(
            &format!("/auth/callback?code={code}&state={state}"),
            Some(&login),
        )
        .await;
    assert_eq!(wrong.status(), StatusCode::BAD_REQUEST);
    assert!(set_cookie(&wrong, "dg_session").is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn refusals_outages_and_cancelled_sign_ins_never_open_a_session() {
    let w = or_skip!(world());
    // Nomen issued a token the hub does not accept (another client, another project...).
    let (login, state, code) = w.authorize("/account", "tok-unknown", 3600).await;
    let refused = w
        .get(
            &format!("/auth/callback?code={code}&state={state}"),
            Some(&login),
        )
        .await;
    assert_eq!(refused.status(), StatusCode::FORBIDDEN);
    assert!(set_cookie(&refused, "dg_session").is_none());

    // Nomen's token endpoint fails: an outage, not a refusal.
    let (login, state, code) = w.authorize("/account", "tok-owner", 3600).await;
    w.fake.status.store(502, SeqCst);
    let down = w
        .get(
            &format!("/auth/callback?code={code}&state={state}"),
            Some(&login),
        )
        .await;
    assert_eq!(down.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(set_cookie(&down, "dg_session").is_none());
    w.fake.status.store(200, SeqCst);

    // The person cancels at Nomen; whatever comes back in `error` is shown inert.
    let (login, state, _) = w.authorize("/account", "tok-owner", 3600).await;
    let cancelled = w
        .get(
            &format!("/auth/callback?error=access_denied%3Cscript%3E&state={state}"),
            Some(&login),
        )
        .await;
    assert_eq!(cancelled.status(), StatusCode::BAD_REQUEST);
    let html = cancelled.text().await.unwrap();
    assert!(
        html.contains("access_deniedscript") && !html.contains("<script>"),
        "{html}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sessions_end_with_the_token_and_return_paths_stay_on_the_hub() {
    let w = or_skip!(world());
    let (login, state, code) = w.authorize("//evil.example/steal", "tok-owner", 1).await;
    let back = w
        .get(
            &format!("/auth/callback?code={code}&state={state}"),
            Some(&login),
        )
        .await;
    assert_eq!(
        location(&back),
        "/account",
        "only a path on this hub is a return target"
    );
    let session = cookie_pair(&set_cookie(&back, "dg_session").unwrap());
    assert_eq!(
        w.get("/account", Some(&session)).await.status(),
        StatusCode::OK
    );
    tokio::time::sleep(Duration::from_millis(1200)).await;
    let expired = w.get("/account", Some(&session)).await;
    assert_eq!(
        expired.status(),
        StatusCode::SEE_OTHER,
        "the session ended with its one-second token"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn return_paths_cookies_dates_and_escaping() {
    for (asked, kept) in [
        ("/account", "/account"),
        ("/account?tab=machines", "/account?tab=machines"),
        ("//evil.example", "/account"),
        ("https://evil.example", "/account"),
        ("/\\evil.example", "/account"),
        ("/a\"><script>", "/account"),
        ("", "/account"),
    ] {
        assert_eq!(safe_return_to(asked), kept, "{asked}");
    }
    let state = tempfile::tempdir().unwrap();
    let store = or_skip!(crate::store::testing::postgres()).store;
    let public = WebLogin::new(
        "https://id.example",
        &["p".into()],
        "web",
        "https://hub.example",
        state.path(),
        store.clone(),
    )
    .unwrap();
    assert_eq!(public.session_cookie(), "__Host-dg_session");
    let cookie = public.cookie(public.session_cookie(), "abc", 60);
    assert!(
        cookie.ends_with("; Secure") && cookie.contains("Path=/") && !cookie.contains("Domain"),
        "{cookie}"
    );
    assert_eq!(pages::utc(951_782_400), "2000-02-29 00:00 UTC");
    assert_eq!(pages::utc(1_792_713_600 + 3_723), "2026-10-23 01:02 UTC");
    assert_eq!(
        pages::escape("<img src=x onerror='a'>&\""),
        "&lt;img src=x onerror=&#39;a&#39;&gt;&amp;&quot;"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_phone_stays_signed_in_across_a_restart_and_through_refreshes() {
    let w = or_skip!(world());
    w.fake.refresh_on_code.store(true, SeqCst);
    let (login, state, code) = w.authorize("/approvals", "tok-owner", 1).await;
    let back = w
        .get(
            &format!("/auth/callback?code={code}&state={state}"),
            Some(&login),
        )
        .await;
    let set = set_cookie(&back, "dg_session").unwrap();
    assert!(
        set.contains(&format!("Max-Age={}", 90 * 24 * 3600)),
        "with a refresh token the phone keeps its session for 90 days: {set}"
    );
    let session = cookie_pair(&set);

    // The one-second access token is renewed through Nomen on the next page; the
    // refresh token it was renewed with is spent and replaced.
    assert_eq!(
        w.get("/approvals", Some(&session)).await.status(),
        StatusCode::OK
    );
    assert_eq!(w.fake.refreshed.load(SeqCst), 1);
    let live: Vec<String> = w.fake.refreshes.lock().unwrap().keys().cloned().collect();
    assert_eq!(live.len(), 1, "rotation leaves one live refresh token");

    // Neither token is kept in the clear.
    let sealed: Vec<Vec<u8>> =
        sqlx::query_scalar("SELECT sealed FROM devagotchi.auth_web_sessions")
            .fetch_all(w.sessions.admin())
            .await
            .unwrap();
    for row in &sealed {
        let text = String::from_utf8_lossy(row);
        assert!(
            !text.contains("tok-owner") && !text.contains("refresh-token-"),
            "a session holds a token in the clear"
        );
    }

    // The hub restarts: a new sign-in service over the same state directory finds the
    // session, with no new sign-in and no extra refresh.
    let restarted = WebLogin::new(
        &w.fake.issuer,
        &["hub-project".to_string()],
        WEB_CLIENT,
        &w.url,
        w.dir.path(),
        w.sessions.clone(),
    )
    .unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(header::COOKIE, session.parse().unwrap());
    assert!(matches!(restarted.token(&headers).await, SessionToken::Token(t) if t == "tok-owner"));
    assert_eq!(w.fake.refreshed.load(SeqCst), 1);

    // Signing out ends the session and revokes its refresh token at Nomen.
    let out = w
        .http
        .post(format!("{}/auth/logout", w.url))
        .header(header::COOKIE, &session)
        .send()
        .await
        .unwrap();
    assert_eq!(out.status(), StatusCode::SEE_OTHER);
    for _ in 0..50 {
        if !w.fake.revoked.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(*w.fake.revoked.lock().unwrap(), live);
    assert_eq!(
        w.get("/approvals", Some(&session)).await.status(),
        StatusCode::SEE_OTHER
    );
    assert!(matches!(
        restarted.token(&headers).await,
        SessionToken::None
    ));

    // Nomen refusing a refresh (account disabled, token revoked) signs the phone out.
    let (login, state, code) = w.authorize("/approvals", "tok-owner", 1).await;
    let back = w
        .get(
            &format!("/auth/callback?code={code}&state={state}"),
            Some(&login),
        )
        .await;
    let due = cookie_pair(&set_cookie(&back, "dg_session").unwrap());
    w.fake.refreshes.lock().unwrap().clear();
    let refused = w.get("/approvals", Some(&due)).await;
    assert_eq!(refused.status(), StatusCode::SEE_OTHER);
    assert_eq!(location(&refused), "/auth/login?return_to=/approvals");
    assert_eq!(
        w.get("/approvals", Some(&due)).await.status(),
        StatusCode::SEE_OTHER,
        "and the session is gone, not retried"
    );
}
