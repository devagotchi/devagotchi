//! The browser sign-in end to end against a fake Nomen token endpoint: the hub's real
//! routes, cookies and redirects, with the authorization step played by the test.
//! (`tests/web_probe.py` runs the same flow against the real binary with Nomen introspection.)

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU16, AtomicUsize, Ordering::SeqCst},
    },
};

use axum::{Form, extract::State, http::StatusCode, response::IntoResponse, routing::post};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::*;
use crate::{
    AppState, config::HubConfig, identity::auth::Verifier, machines::devices::Devices,
    machines::link::Links, mcp::Hub, notify::push::Push,
};

const OWNER: &str = "300000000000000001";
const OTHER: &str = "300000000000000002";
const WEB_CLIENT: &str = "devagotchi-web";
#[cfg(unix)]
const KEY_FILE: &str = "/nonexistent/hub-key.json";
#[cfg(windows)]
const KEY_FILE: &str = r"C:\nonexistent\hub-key.json";
#[cfg(unix)]
const SOCKET: &str = "/nonexistent/devagotchi-broker.sock";
#[cfg(windows)]
const SOCKET: &str = r"\\.\pipe\devagotchi-web-test-absent";

/// A code the "authorization" step issued: the PKCE challenge it was bound to and the
/// access token it buys.
struct Grant {
    challenge: String,
    token: String,
    expires_in: u64,
}

struct FakeNomen {
    issuer: String,
    redirect_uri: Mutex<String>,
    codes: Mutex<HashMap<String, Grant>>,
    status: AtomicU16,
    /// Codes also buy a refresh token (the app has the refresh grant and the hub asked
    /// for `offline_access`).
    refresh_on_code: AtomicBool,
    /// Live refresh tokens and the access token each buys; spent on use (rotation).
    refreshes: Mutex<HashMap<String, String>>,
    refreshed: AtomicUsize,
    revoked: Mutex<Vec<String>>,
}

impl FakeNomen {
    fn mint_refresh(&self, access: &str) -> String {
        let mut refreshes = self.refreshes.lock().unwrap();
        let token = format!(
            "refresh-token-{}",
            self.refreshed.load(SeqCst) + refreshes.len()
        );
        refreshes.insert(token.clone(), access.to_string());
        token
    }
}

async fn revoke(
    State(fake): State<Arc<FakeNomen>>,
    Form(form): Form<HashMap<String, String>>,
) -> StatusCode {
    let token = form.get("token").cloned().unwrap_or_default();
    if form.get("client_id").map(String::as_str) == Some(WEB_CLIENT) {
        fake.refreshes.lock().unwrap().remove(&token);
        fake.revoked.lock().unwrap().push(token);
    }
    StatusCode::OK
}

async fn token(
    State(fake): State<Arc<FakeNomen>>,
    Form(form): Form<HashMap<String, String>>,
) -> axum::response::Response {
    let status = fake.status.load(SeqCst);
    if status != 200 {
        return StatusCode::from_u16(status).unwrap().into_response();
    }
    let bad = (
        StatusCode::BAD_REQUEST,
        axum::Json(json!({"error": "invalid_grant"})),
    )
        .into_response();
    let field = |key: &str| form.get(key).cloned().unwrap_or_default();
    if field("grant_type") == "refresh_token" && field("client_id") == WEB_CLIENT {
        let Some(access) = fake
            .refreshes
            .lock()
            .unwrap()
            .remove(&field("refresh_token"))
        else {
            return bad;
        };
        let rotated = fake.mint_refresh(&access);
        fake.refreshed.fetch_add(1, SeqCst);
        return axum::Json(json!({"access_token": access, "token_type": "Bearer", "expires_in": 3600, "refresh_token": rotated}))
            .into_response();
    }
    if field("grant_type") != "authorization_code"
        || field("client_id") != WEB_CLIENT
        || field("redirect_uri") != *fake.redirect_uri.lock().unwrap()
    {
        return bad;
    }
    // Codes are single-use, and only the verifier whose hash was in the request redeems one.
    let Some(grant) = fake.codes.lock().unwrap().remove(&field("code")) else {
        return bad;
    };
    if URL_SAFE_NO_PAD.encode(Sha256::digest(field("code_verifier").as_bytes())) != grant.challenge
    {
        return bad;
    }
    let mut answer = json!({"access_token": grant.token, "token_type": "Bearer", "expires_in": grant.expires_in, "id_token": "unused"});
    if fake.refresh_on_code.load(SeqCst) {
        answer["refresh_token"] = json!(fake.mint_refresh(&grant.token));
    }
    axum::Json(answer).into_response()
}

struct World {
    devices: Arc<Devices>,
    /// Where the web login keeps its sessions (the test Postgres).
    sessions: crate::store::Store,
    fake: Arc<FakeNomen>,
    url: String,
    http: reqwest::Client,
    hub: Hub,
    dir: tempfile::TempDir,
}

async fn world() -> Option<World> {
    world_with(None).await
}

/// The world, handing out `downloads` to joining machines (`/join`, `/dist`).
async fn world_with(
    downloads: Option<Arc<crate::machines::downloads::Downloads>>,
) -> Option<World> {
    let nomen = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let fake = Arc::new(FakeNomen {
        issuer: format!("http://{}", nomen.local_addr().unwrap()),
        redirect_uri: Mutex::new(String::new()),
        codes: Mutex::new(HashMap::new()),
        status: AtomicU16::new(200),
        refresh_on_code: AtomicBool::new(false),
        refreshes: Mutex::new(HashMap::new()),
        refreshed: AtomicUsize::new(0),
        revoked: Mutex::new(Vec::new()),
    });
    let app = axum::Router::new()
        .route("/oauth/v2/token", post(token))
        .route("/oauth/v2/revoke", post(revoke))
        .with_state(fake.clone());
    tokio::spawn(async move { axum::serve(nomen, app).await.unwrap() });

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    *fake.redirect_uri.lock().unwrap() = format!("{url}/auth/callback");
    let dir = tempfile::tempdir().unwrap();
    let config = Arc::new(
        HubConfig::parse(
            &json!({
                "listen": "127.0.0.1:1",
                "public_url": url,
                "auth": {"mode": "nomen", "issuer": fake.issuer, "audiences": ["hub-project"],
                         "clients": ["shippin-mcp", WEB_CLIENT], "key_file": KEY_FILE},
                "owner": OWNER,
                "web": {"client_id": WEB_CLIENT},
                "brokers": [{"name": "gold-ws", "socket": SOCKET}, {"name": "gus", "link": true},
                            {"name": "lab", "link": true}, {"name": "old", "link": true}],
                "grants": [{"subject": OTHER, "brokers": ["gus"]}],
                "boards": [{"project": "estate"}]
            })
            .to_string(),
        )
        .unwrap(),
    );
    let devices = crate::machines::devices::test_registry().await?;
    let registry = devices.clone();
    for name in ["gus", "old"] {
        let code = devices
            .issue_code(name, crate::machines::random::<16>())
            .await
            .unwrap();
        devices.enroll(name, &code, "test-key", true).await.unwrap();
    }
    devices.revoke("old").await.unwrap();
    let audit = crate::ops::audit::test_audit().await?;
    let links = Arc::new(Links::new(config.clone(), devices.clone(), audit.clone()));
    // The token check itself (Nomen introspection) has its own tests; here tokens map to
    // subjects directly so the flow around them is what is tested.
    let verifier = Verifier::Dev(HashMap::from([
        ("tok-owner".to_string(), OWNER.to_string()),
        ("tok-other".to_string(), OTHER.to_string()),
    ]));
    let sessions = crate::store::testing::postgres().await?.store;
    let web = WebLogin::new(
        &fake.issuer,
        &["hub-project".to_string()],
        WEB_CLIENT,
        &url,
        dir.path(),
        sessions.clone(),
    )
    .unwrap();
    let push = Arc::new(Push::open(dir.path(), &url, devices.clone()).unwrap());
    let state = AppState {
        verifier: Arc::new(verifier),
        hub: Hub {
            downloads,
            analytics: None,
            workflows: None,
            offers: None,
            connections: None,
            routed: Default::default(),
            push: Some(push),
            boards: Arc::new(crate::agenthub::mcp::Boards::new(&config, sessions.clone()).unwrap()),
            config,
            audit,
            links,
        },
        issue_store: Arc::new(crate::agenthub::issue_lifecycle::IssueLifecycleStore::disabled()),
        web: Some(Arc::new(web)),
        passkeys: None,
    };
    let hub = state.hub.clone();
    let app = router(true)
        .layer(axum::middleware::from_fn_with_state(state.clone(), bar))
        .merge(crate::machines::join::routes())
        .with_state(state);
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    Some(World {
        sessions,
        devices: registry,
        fake,
        url,
        http,
        hub,
        dir,
    })
}

fn location(response: &reqwest::Response) -> String {
    response.headers()[header::LOCATION]
        .to_str()
        .unwrap()
        .to_string()
}

/// `name=value` of the Set-Cookie whose name is `name`, with its attributes.
fn set_cookie(response: &reqwest::Response, name: &str) -> Option<String> {
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().unwrap().to_string())
        .find(|v| v.starts_with(&format!("{name}=")))
}

fn cookie_pair(set_cookie: &str) -> String {
    set_cookie.split(';').next().unwrap().to_string()
}

fn query(url: &str) -> HashMap<String, String> {
    let query = url.split_once('?').map(|(_, q)| q).unwrap_or_default();
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(k, v)| (k.to_string(), percent_decode(v)))
        .collect()
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            out.push(u8::from_str_radix(&value[i + 1..i + 3], 16).unwrap());
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap()
}

impl World {
    async fn get(&self, path: &str, cookie: Option<&str>) -> reqwest::Response {
        let mut request = self.http.get(format!("{}{path}", self.url));
        if let Some(cookie) = cookie {
            request = request.header(header::COOKIE, cookie);
        }
        request.send().await.unwrap()
    }

    /// Start a sign-in, then play Nomen's authorization step: issue a code bound to the
    /// challenge the hub sent. Returns the login cookie, the state and the code.
    async fn authorize(
        &self,
        return_to: &str,
        token: &str,
        expires_in: u64,
    ) -> (String, String, String) {
        let start = self
            .get(&format!("/auth/login?return_to={return_to}"), None)
            .await;
        assert_eq!(start.status(), StatusCode::SEE_OTHER);
        let to = location(&start);
        assert!(
            to.starts_with(&format!("{}/oauth/v2/authorize?", self.fake.issuer)),
            "{to}"
        );
        let params = query(&to);
        assert_eq!(params["client_id"], WEB_CLIENT);
        assert_eq!(
            params["redirect_uri"],
            format!("{}/auth/callback", self.url)
        );
        assert_eq!(params["response_type"], "code");
        assert_eq!(params["code_challenge_method"], "S256");
        assert_eq!(
            params["scope"],
            "openid profile offline_access urn:nomen:iam:user:resourceowner urn:nomen:iam:org:project:id:hub-project:aud"
        );
        let login = set_cookie(&start, "dg_login").expect("login cookie");
        assert!(
            login.contains("HttpOnly")
                && login.contains("SameSite=Lax")
                && login.contains("Max-Age=600"),
            "{login}"
        );
        let code = format!("code-{}", self.fake.codes.lock().unwrap().len());
        self.fake.codes.lock().unwrap().insert(
            code.clone(),
            Grant {
                challenge: params["code_challenge"].clone(),
                token: token.into(),
                expires_in,
            },
        );
        (cookie_pair(&login), params["state"].clone(), code)
    }

    async fn sign_in(&self, token: &str) -> String {
        let (login, state, code) = self.authorize("/account", token, 3600).await;
        let back = self
            .get(
                &format!("/auth/callback?code={code}&state={state}"),
                Some(&login),
            )
            .await;
        assert_eq!(back.status(), StatusCode::SEE_OTHER);
        cookie_pair(&set_cookie(&back, "dg_session").expect("session cookie"))
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn names_from_nomen_are_escaped_on_the_page() {
    let response = pages::account(
        &pages::Viewer {
            subject: OWNER,
            name: Some("<img src=x onerror=alert(1)>"),
            owner: false,
            hub_account: false,
            unsecured_until: None,
            events: &[],
            ways: "",
            linked: &[],
            access: None,
            plan: None,
            operator: false,
        },
        &[],
    );
    let body = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        html.contains("&lt;img src=x onerror=alert(1)&gt;") && !html.contains("<img"),
        "{html}"
    );
    assert!(html.contains("No machines yet"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_public_pages_need_no_sign_in_and_name_the_operator() {
    let w = or_skip!(world());
    for (path, heading) in [
        ("/docs", "<h1>Devagotchi</h1>"),
        ("/privacy", "<h1>Privacy policy</h1>"),
        ("/terms", "<h1>Terms of service</h1>"),
        ("/support", "<h1>Support</h1>"),
    ] {
        let response = w.get(path, None).await;
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        assert!(
            response
                .headers()
                .get(header::CONTENT_SECURITY_POLICY)
                .is_some_and(|csp| csp.to_str().unwrap().starts_with("default-src 'none'")),
            "{path} keeps the strict policy"
        );
        let body = response.text().await.unwrap();
        assert!(body.contains(heading), "{path}");
        assert!(!body.contains("<script"), "{path} has no script");
        for link in ["/docs", "/privacy", "/terms", "/support"] {
            assert!(
                body.contains(&format!("href=\"{link}\"")),
                "{path} links {link}"
            );
        }
    }
    let privacy = w.get("/privacy", None).await.text().await.unwrap();
    assert!(privacy.contains("AngryVibes LLC"));
    // The address an AI connects to is this hub's own, never a hosted one.
    let docs = w.get("/docs", None).await.text().await.unwrap();
    assert!(
        docs.contains(&format!("<code>{}/mcp</code>", w.url)),
        "{docs}"
    );
    assert!(!docs.contains("app.devagotchi.com"), "{docs}");
    assert!(docs.contains("sign in with your Shippin account"), "{docs}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_person_approves_on_the_phone_page_and_the_agent_sees_it() {
    use crate::agenthub::Actor;
    let w = or_skip!(world());
    let board = w.hub.boards.open("estate").unwrap();
    let asked = board
        .request_approval(
            &Actor::agent("claude"),
            "Restart the hub on Texas",
            "systemctl restart devagotchi-hub",
            "new build",
            "start the old build",
        )
        .unwrap();

    let owner = w.sign_in("tok-owner").await;
    let page = w.get("/approvals", Some(&owner)).await;
    assert_eq!(page.status(), StatusCode::OK);
    let body = page.text().await.unwrap();
    assert!(body.contains("Restart the hub on Texas"));
    assert!(body.contains(&format!("value=\"{}\"", asked.id)));
    assert_eq!(
        body.matches("<script").count(),
        3,
        "the app's three scripts, the hub's own files; nothing inline"
    );
    assert!(body.contains("<script src=\"/assets/reveal.js\"></script>"));
    assert!(body.contains("<script src=\"/assets/push.js\" defer></script>"));

    let form = [
        ("project", "estate".to_string()),
        ("id", asked.id.clone()),
        ("version", asked.version.to_string()),
        ("verdict", "approved".to_string()),
        ("note", "go".to_string()),
    ];
    let post = |site: &'static str| {
        w.http
            .post(format!("{}/approvals/decide", w.url))
            .header(header::COOKIE, &owner)
            .header("sec-fetch-site", site)
            .form(&form)
    };
    let refused = post("cross-site").send().await.unwrap();
    assert_eq!(
        refused.status(),
        StatusCode::FORBIDDEN,
        "another site's form is refused"
    );
    assert_eq!(board.approval_status(&asked.id).unwrap().status, "pending");

    let decided = post("same-origin").send().await.unwrap();
    assert_eq!(decided.status(), StatusCode::SEE_OTHER);
    assert_eq!(location(&decided), format!("/approvals?done={}", asked.id));
    let status = board.approval_status(&asked.id).unwrap();
    assert_eq!(status.status, "approved");
    assert_eq!(status.decided_by.as_deref(), Some(OWNER));
    assert_eq!(status.note.as_deref(), Some("go"));
    let after = w
        .get("/approvals", Some(&owner))
        .await
        .text()
        .await
        .unwrap();
    assert!(after.contains("Nothing is waiting for you."));

    let twice = post("same-origin").send().await.unwrap();
    assert_eq!(
        twice.status(),
        StatusCode::CONFLICT,
        "a decided approval stays decided"
    );

    let other = w.sign_in("tok-other").await;
    let page = w.get("/approvals", Some(&other)).await;
    assert_eq!(page.status(), StatusCode::FORBIDDEN, "only people approve");
    let signed_out = w.get("/approvals", None).await;
    assert_eq!(signed_out.status(), StatusCode::SEE_OTHER);
}

/// Configured boards decide as they always have: the owner and a grant marked human that
/// names the board; a grant that is not a person, and an account without one, never.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn configured_boards_decide_for_the_owner_and_human_grants_only() {
    use crate::{agenthub::Actor, identity::Subject};
    let w = or_skip!(world());
    let board = w.hub.boards.open("estate").unwrap();
    let asked = board
        .request_approval(&Actor::agent("claude"), "Deploy", "deploy", "", "")
        .unwrap();

    // Over HTTP: OTHER holds no board grant and is not a person; its decision is refused.
    let other = w.sign_in("tok-other").await;
    let refused = w
        .http
        .post(format!("{}/approvals/decide", w.url))
        .header(header::COOKIE, &other)
        .header("sec-fetch-site", "same-origin")
        .form(&[
            ("project", "estate".to_string()),
            ("id", asked.id.clone()),
            ("version", asked.version.to_string()),
            ("verdict", "approved".to_string()),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(refused.status(), StatusCode::FORBIDDEN);
    assert_eq!(board.approval_status(&asked.id).unwrap().status, "pending");

    // The resolver on a config with both kinds of grant on `estate`.
    let config = Arc::new(
        HubConfig::parse(
            &json!({
                "listen": "127.0.0.1:1",
                "public_url": w.url,
                "auth": {"mode": "dev", "tokens_file": w.dir.path().join("tokens.json")},
                "owner": OWNER,
                "brokers": [],
                "grants": [
                    {"subject": "300000000000000003", "boards": ["estate"], "human": true},
                    {"subject": "300000000000000004", "boards": ["estate"]}
                ],
                "boards": [{"project": "estate"}]
            })
            .to_string(),
        )
        .unwrap(),
    );
    let hub = Hub {
        boards: Arc::new(crate::agenthub::mcp::Boards::new(&config, w.sessions.clone()).unwrap()),
        config,
        ..w.hub.clone()
    };
    let person = |id: &str| Subject {
        id: id.into(),
        via: "nomen",
        tenant: None,
        name: None,
        org: None,
        provisional_until: None,
        agent: None,
        claim_code: None,
        client: None,
        provisional_warning: false,
        access: None,
    };
    for (id, decides) in [
        (OWNER, true),
        ("300000000000000003", true),
        ("300000000000000004", false),
        ("300000000000000005", false),
    ] {
        let subject = person(id);
        assert_eq!(
            hub.approval_board(&subject, "estate").is_ok(),
            decides,
            "{id}"
        );
        assert_eq!(
            hub.approval_boards(&subject)
                .unwrap()
                .iter()
                .map(|(p, _)| p.as_str())
                .collect::<Vec<_>>(),
            if decides { vec!["estate"] } else { vec![] },
            "{id}"
        );
        assert_eq!(hub.may_approve(&subject), decides, "{id}");
    }
    let refused = hub
        .approval_board(&person("300000000000000004"), "estate")
        .err()
        .unwrap();
    assert_eq!(refused.code, "humans_only");
    let unknown = hub
        .approval_board(&person("300000000000000005"), "estate")
        .err()
        .unwrap();
    assert_eq!(unknown.code, "not_granted");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn notify_this_phone_then_an_approval_buzzes_it_encrypted() {
    use crate::agenthub::Actor;
    use aws_lc_rs::{
        agreement::{ECDH_P256, PrivateKey},
        rand::{SecureRandom, SystemRandom},
    };

    // A fake push service: records what it is sent; `/gone/*` answers 410.
    type Seen = Arc<Mutex<Vec<(axum::http::HeaderMap, Vec<u8>)>>>;
    let seen: Seen = Arc::default();
    async fn receive(
        State(seen): State<Seen>,
        axum::extract::Path(id): axum::extract::Path<String>,
        headers: axum::http::HeaderMap,
        body: axum::body::Bytes,
    ) -> StatusCode {
        seen.lock().unwrap().push((headers, body.to_vec()));
        if id.starts_with("gone") {
            StatusCode::GONE
        } else {
            StatusCode::CREATED
        }
    }
    let service = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", service.local_addr().unwrap());
    let app = axum::Router::new()
        .route("/push/{id}", post(receive))
        .with_state(seen.clone());
    tokio::spawn(async move { axum::serve(service, app).await.unwrap() });

    let w = or_skip!(world());
    let page = {
        let owner = w.sign_in("tok-owner").await;
        let body = w.get("/approvals", Some(&owner)).await;
        let csp = body.headers()[header::CONTENT_SECURITY_POLICY]
            .to_str()
            .unwrap()
            .to_string();
        assert!(csp.contains("script-src 'self'") && csp.contains("worker-src 'self'"));
        (owner, body.text().await.unwrap())
    };
    let (owner, html) = page;
    let key = w.hub.push.as_ref().unwrap().public_key.clone();
    assert!(
        html.contains(&format!("data-key=\"{key}\"")),
        "the page carries the public key only"
    );
    assert!(html.contains("/assets/push.js") && html.contains("/manifest.webmanifest"));
    for (path, kind) in [
        ("/sw.js", "text/javascript"),
        ("/assets/push.js", "text/javascript"),
        ("/manifest.webmanifest", "application/manifest+json"),
        ("/assets/icon.svg", "image/svg+xml"),
    ] {
        let r = w.get(path, None).await;
        assert_eq!(r.status(), StatusCode::OK, "{path}");
        assert!(
            r.headers()[header::CONTENT_TYPE]
                .to_str()
                .unwrap()
                .starts_with(kind),
            "{path}"
        );
    }

    let phone = PrivateKey::generate(&ECDH_P256).unwrap();
    let mut auth = [0u8; 16];
    SystemRandom::new().fill(&mut auth).unwrap();
    let b64 = |b: &[u8]| URL_SAFE_NO_PAD.encode(b);
    let subscribe = |endpoint: String, site: &'static str, cookie: String| {
        let body = json!({"endpoint": endpoint, "keys": {"p256dh": b64(phone.compute_public_key().unwrap().as_ref()), "auth": b64(&auth)}});
        w.http
            .post(format!("{}/push/subscribe", w.url))
            .header(header::COOKIE, cookie)
            .header("sec-fetch-site", site)
            .json(&body)
    };
    let refused = subscribe(format!("{base}/push/phone"), "cross-site", owner.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(refused.status(), StatusCode::FORBIDDEN);
    let other = w.sign_in("tok-other").await;
    let refused = subscribe(format!("{base}/push/phone"), "same-origin", other)
        .send()
        .await
        .unwrap();
    assert_eq!(
        refused.status(),
        StatusCode::FORBIDDEN,
        "only people get approval pushes"
    );
    for endpoint in [
        format!("{base}/push/phone"),
        format!("{base}/push/gone-tablet"),
    ] {
        let ok = subscribe(endpoint, "same-origin", owner.clone())
            .send()
            .await
            .unwrap();
        assert_eq!(ok.status(), StatusCode::NO_CONTENT);
    }

    let board = w.hub.boards.open("estate").unwrap();
    let asked = board
        .request_approval(
            &Actor::agent("claude"),
            "Restart the hub on Texas",
            "systemctl restart devagotchi-hub",
            "",
            "",
        )
        .unwrap();
    assert_eq!(
        w.hub.announce_approval("estate", &asked).await,
        2,
        "the owner's two phones are sent the push"
    );
    for _ in 0..100 {
        if seen.lock().unwrap().len() >= 2 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 2, "both subscriptions were tried");
    let (headers, body) = &seen[0];
    assert_eq!(headers["content-encoding"], "aes128gcm");
    assert_eq!(headers["ttl"], "86400");
    let authorization = headers["authorization"].to_str().unwrap();
    assert!(
        authorization.starts_with("vapid t=") && authorization.ends_with(&format!(", k={key}"))
    );
    let payload: Value =
        serde_json::from_slice(&crate::notify::push::decrypt(&phone, &auth, body)).unwrap();
    assert_eq!(payload["title"], "Approval needed");
    assert_eq!(payload["body"], "Restart the hub on Texas");
    assert_eq!(payload["url"], format!("/approvals#estate-{}", asked.id));
    assert!(
        !payload.to_string().contains("systemctl"),
        "the command never travels in a push"
    );
    let left = w
        .hub
        .push
        .as_ref()
        .unwrap()
        .devices()
        .push_for(OWNER)
        .await
        .unwrap();
    assert_eq!(
        left.len(),
        1,
        "the subscription the service called gone is dropped"
    );
    assert!(left[0].endpoint.ends_with("/push/phone"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_page_says_whether_you_are_signed_in_with_one_tap_in_or_out() {
    let w = or_skip!(world());
    // Signed out: `/` explains and offers one way in; every page offers Sign in back to it.
    let home = w.get("/", None).await;
    assert_eq!(
        home.status(),
        StatusCode::OK,
        "no bounce to the identity provider"
    );
    let html = home.text().await.unwrap();
    assert!(html.contains("Sign in with Shippin"), "{html}");
    let docs = w.get("/docs", None).await.text().await.unwrap();
    assert!(
        docs.contains("href=\"/auth/login?return_to=%2Fdocs\">Sign in</a>"),
        "{docs}"
    );
    assert!(!docs.contains("Sign out"));

    // Signed in: `/` is your machines, and every page has Approvals, Machines and Sign out.
    let session = w.sign_in("tok-owner").await;
    let home = w.get("/", Some(&session)).await;
    assert_eq!(home.status(), StatusCode::SEE_OTHER);
    assert_eq!(location(&home), "/account");
    let framed = w
        .get("/approvals", Some(&session))
        .await
        .text()
        .await
        .unwrap();
    assert!(
        framed.contains("id=\"me-link\""),
        "the app's bar goes to the person's page: {framed}"
    );
    assert!(!framed.contains(">Sign in</a>"), "{framed}");
    for path in ["/docs", "/account"] {
        let html = w.get(path, Some(&session)).await.text().await.unwrap();
        assert!(
            html.contains("<form method=\"post\" action=\"/auth/logout\"><button type=\"submit\">Sign out</button></form>")
                && html.contains("<a href=\"/approvals\">Approvals</a>")
                && !html.contains(">Sign in</a>"),
            "{path}: {html}"
        );
    }

    // One tap out, and the bar says so straight away.
    let out = w
        .http
        .post(format!("{}/auth/logout", w.url))
        .header(header::COOKIE, &session)
        .send()
        .await
        .unwrap();
    assert_eq!(location(&out), "/signed-out");
    let after = w.get("/docs", Some(&session)).await.text().await.unwrap();
    assert!(
        after.contains(">Sign in</a>") && !after.contains("Sign out"),
        "{after}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_new_board_says_how_to_start_and_one_tap_tests_the_phone() {
    use crate::agenthub::{Actor, Filter};
    use aws_lc_rs::{
        agreement::{ECDH_P256, PrivateKey},
        rand::{SecureRandom, SystemRandom},
    };

    type Seen = Arc<Mutex<Vec<Vec<u8>>>>;
    let seen: Seen = Arc::default();
    async fn receive(State(seen): State<Seen>, body: axum::body::Bytes) -> StatusCode {
        seen.lock().unwrap().push(body.to_vec());
        StatusCode::CREATED
    }
    let service = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", service.local_addr().unwrap());
    let app = axum::Router::new()
        .route("/push/phone", post(receive))
        .with_state(seen.clone());
    tokio::spawn(async move { axum::serve(service, app).await.unwrap() });

    let w = or_skip!(world());
    let owner = w.sign_in("tok-owner").await;
    let html = w
        .get("/approvals", Some(&owner))
        .await
        .text()
        .await
        .unwrap();
    assert!(
        html.contains("action=\"/approvals/test\"") && html.contains("Send me a test approval"),
        "{html}"
    );
    // Opening the never-used board put the getting-started steps on it.
    let board = w.hub.boards.open("estate").unwrap();
    let tickets = board.list(&Filter::default()).unwrap();
    assert_eq!(tickets.len(), 1);
    assert_eq!(tickets[0].title, crate::site::builtin().welcome("").0);
    assert!(
        tickets[0]
            .description
            .contains(&format!("{}/approvals", w.url))
    );

    let post_test = |cookie: &str, site: &'static str| {
        w.http
            .post(format!("{}/approvals/test", w.url))
            .header(header::COOKIE, cookie.to_string())
            .header("sec-fetch-site", site)
            .send()
    };
    let no_phone = post_test(&owner, "same-origin").await.unwrap();
    assert_eq!(no_phone.status(), StatusCode::SEE_OTHER);
    assert_eq!(location(&no_phone), "/approvals?test=no-phone");
    let html = w
        .get("/approvals?test=no-phone", Some(&owner))
        .await
        .text()
        .await
        .unwrap();
    assert!(html.contains("Tap Notify this phone first"), "{html}");
    assert_eq!(
        post_test(&owner, "cross-site").await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    let other = w.sign_in("tok-other").await;
    assert_eq!(
        post_test(&other, "same-origin").await.unwrap().status(),
        StatusCode::FORBIDDEN,
        "an account with no board it decides on cannot send one"
    );

    let phone = PrivateKey::generate(&ECDH_P256).unwrap();
    let mut auth = [0u8; 16];
    SystemRandom::new().fill(&mut auth).unwrap();
    let b64 = |b: &[u8]| URL_SAFE_NO_PAD.encode(b);
    let subscribed = w
        .http
        .post(format!("{}/push/subscribe", w.url))
        .header(header::COOKIE, &owner)
        .header("sec-fetch-site", "same-origin")
        .json(&json!({"endpoint": format!("{base}/push/phone"),
                      "keys": {"p256dh": b64(phone.compute_public_key().unwrap().as_ref()), "auth": b64(&auth)}}))
        .send()
        .await
        .unwrap();
    assert_eq!(subscribed.status(), StatusCode::NO_CONTENT);

    let sent = post_test(&owner, "same-origin").await.unwrap();
    assert_eq!(location(&sent), "/approvals?test=sent");
    for _ in 0..100 {
        if !seen.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let pushes = seen.lock().unwrap().clone();
    assert_eq!(
        pushes.len(),
        1,
        "the test goes to the person who asked, once"
    );
    let payload: Value =
        serde_json::from_slice(&crate::notify::push::decrypt(&phone, &auth, &pushes[0])).unwrap();
    assert_eq!(payload["title"], "Approval needed");
    assert_eq!(payload["body"], "Test: tap Approve");

    // The owner's AI asks for something real with the owner's own account.
    board
        .request_approval(
            &Actor::agent(OWNER),
            "Restart nginx",
            "systemctl restart nginx",
            "",
            "",
        )
        .unwrap();
    let html = w
        .get("/approvals?test=sent", Some(&owner))
        .await
        .text()
        .await
        .unwrap();
    assert!(html.contains("Test sent."), "{html}");
    assert!(
        html.contains("<h2>Test: tap Approve</h2><p class=\"meta\">Devagotchi asks"),
        "{html}"
    );
    assert!(
        html.contains("<h2>Restart nginx</h2><p class=\"meta\">Your AI asks"),
        "{html}"
    );
}

mod machines;
mod nomen_login;

// ------------------------------------------------ first connect (spec first-connect, A2)

mod first_connect {
    use reqwest::header;
    use serde_json::{Value, json};

    use crate::identity::accounts::oauth::s256;
    use crate::tenancy::tests::tenants::*;

    const REDIRECT: &str = "http://127.0.0.1:9/callback";

    /// A hosted hub as the spec runs it: no `anonymous_connect`, Google configured (never
    /// reached: only its button is drawn).
    async fn hub() -> Option<Hosted> {
        hosted_with(None, |dir, _| {
            let secret = dir.join("google-secret");
            std::fs::write(&secret, "s3cret\n").unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o600)).unwrap();
            }
            json!({
                "tenancy": { "mode": "multi", "starts_per_hour": 100 },
                "auth": { "providers": [{ "kind": "google", "issuer": "https://accounts.google.test",
                    "client_id": "client-1", "client_secret_file": secret.to_string_lossy() }] }
            })
        })
        .await
    }

    /// A newly registered AI app and its authorize request (path and query).
    async fn app(hub: &Hosted, name: &str) -> (String, String) {
        let registered: Value = hub
            .http
            .post(format!("{}/oauth/register", hub.url))
            .json(&json!({ "client_name": name, "redirect_uris": [REDIRECT] }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let client = registered["client_id"].as_str().unwrap().to_string();
        let query = format!(
            "/oauth/authorize?response_type=code&client_id={client}&redirect_uri={}&code_challenge={}&code_challenge_method=S256&state=s1&resource={}",
            web_encode(REDIRECT),
            s256(&"v".repeat(64)),
            web_encode(&format!("{}/mcp", hub.url))
        );
        (client, query)
    }

    fn location(response: &reqwest::Response) -> String {
        response.headers()[header::LOCATION]
            .to_str()
            .unwrap()
            .to_string()
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn sign_up_on_the_one_page_then_straight_back_to_the_app() {
        let hub = or_skip!(hub());
        let (client, authorize) = app(&hub, "Claude").await;
        // No session: to the sign-in page, carrying this very request.
        let first = hub.get(&authorize, None).await;
        assert_eq!(first.status(), 303);
        let login = location(&first);
        assert!(login.starts_with("/auth/login?return_to="), "{login}");
        let return_to = query_param(&login, "return_to").unwrap();
        assert!(
            return_to.starts_with("/oauth/authorize?") && return_to.contains("state=s1"),
            "{return_to}"
        );
        // The page: Google, "or", email and password, sign in / sign up, forgot. Nothing else.
        let (status, page) = Hosted::text(hub.get(&login, None).await).await;
        assert_eq!(status, 200, "{page}");
        for needle in [
            "/auth/provider/google?return_to=",
            "Continue with Google",
            ">or<",
            "action=\"/auth/password\"",
            "name=\"email\"",
            "name=\"password\"",
            "tab=up",
            "Forgot password?",
        ] {
            assert!(page.contains(needle), "{needle}: {page}");
        }
        let lower = page.to_lowercase();
        for absent in ["passkey", "secure", "connect"] {
            assert!(!lower.contains(absent), "{absent}: {page}");
        }
        let up = Hosted::text(hub.get(&format!("{login}&tab=up"), None).await)
            .await
            .1;
        assert!(
            up.contains("action=\"/auth/password/new\"") && up.contains("Create account"),
            "{up}"
        );
        // Sign up: back to the authorize request, signed in and fresh.
        let made = hub
            .form(
                "/auth/password/new",
                None,
                &[
                    ("email", "new@example.com"),
                    ("password", "a good password"),
                    ("return_to", &return_to),
                ],
            )
            .await;
        assert_eq!(made.status(), 303);
        assert_eq!(location(&made), return_to);
        let session = set_cookie(&made, "dg_session").expect("signed in");
        let fresh = set_cookie(&made, "dvg_fresh").expect("a fresh sign-in");
        // That request now goes straight to the app with its code: no Connect page.
        let back = hub
            .get(&return_to, Some(&format!("{session}; {fresh}")))
            .await;
        assert_eq!(back.status(), 303);
        let to_app = location(&back);
        assert!(to_app.starts_with(REDIRECT), "{to_app}");
        assert_eq!(query_param(&to_app, "state").as_deref(), Some("s1"));
        let code = query_param(&to_app, "code").expect("a code");
        assert_eq!(
            set_cookie(&back, "dvg_fresh").as_deref(),
            Some("dvg_fresh="),
            "spent"
        );
        // The app's tokens, and whoami: secured, nothing to secure.
        let tokens: Value = hub
            .http
            .post(format!("{}/oauth/token", hub.url))
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", &code),
                ("redirect_uri", REDIRECT),
                ("client_id", &client),
                ("code_verifier", &"v".repeat(64)),
            ])
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let mut person = Person {
            cookie: session.clone(),
            token: tokens["access_token"]
                .as_str()
                .expect("a token")
                .to_string(),
            session: None,
        };
        let who = hub.call(&mut person, "whoami", json!({})).await;
        assert!(
            who["structuredContent"].get("secure_url").is_none(),
            "{who}"
        );
        // A second app later, same browser, no fresh sign-in: the Connect page, as today.
        let (_, second) = app(&hub, "Codex").await;
        let (status, page) = Hosted::text(hub.get(&second, Some(&session)).await).await;
        assert_eq!(status, 200);
        assert!(page.contains("<h1>Connect Codex?</h1>"), "{page}");
        // The fresh cookie of one request does not skip another's Connect page.
        let (status, page) =
            Hosted::text(hub.get(&second, Some(&format!("{session}; {fresh}"))).await).await;
        assert_eq!(status, 200);
        assert!(page.contains("<h1>Connect Codex?</h1>"), "{page}");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_wrong_password_says_so_on_the_same_page() {
        let hub = or_skip!(hub());
        hub.accounts
            .sign_up_password("kim@example.com", "kims password")
            .await
            .unwrap();
        let (_, authorize) = app(&hub, "Claude").await;
        let wrong = hub
            .form(
                "/auth/password",
                None,
                &[
                    ("email", "kim@example.com"),
                    ("password", "nope nope"),
                    ("return_to", &authorize),
                ],
            )
            .await;
        assert!(set_cookie(&wrong, "dg_session").is_none());
        let (status, page) = Hosted::text(wrong).await;
        assert_eq!(status, 400);
        assert!(
            page.contains("Email or password is wrong.")
                && page.contains("action=\"/auth/password\""),
            "{page}"
        );
        let right = hub
            .form(
                "/auth/password",
                None,
                &[
                    ("email", "kim@example.com"),
                    ("password", "kims password"),
                    ("return_to", &authorize),
                ],
            )
            .await;
        assert_eq!(right.status(), 303);
        assert_eq!(location(&right), authorize);
        // Forgot password on a hub that sends no mail, with Google configured: name Google.
        let (status, page) = Hosted::text(hub.get("/auth/password/forgot", None).await).await;
        assert_eq!(status, 200);
        assert!(
            page.contains("Password reset isn") && page.contains("Sign in with Google"),
            "{page}"
        );
        assert!(!page.contains("devagotchi hub owner"), "{page}");
    }
}
