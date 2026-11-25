//! A hosted hub (`tenancy.mode multi`) end to end over HTTP, the whole router as it serves:
//! two people connect an AI client with nothing but a Connect press, each gets an account
//! and a tenant of their own, and then tenant A drives every MCP tool and every account page
//! with tenant B's ids. Nothing of B's is revealed to A, and nothing of B's changes.

use std::{net::SocketAddr, sync::Arc};

use axum::http::header;
use serde_json::{Value, json};

use crate::identity::{self, accounts::oauth::s256, auth::Verifier};
use crate::{AppState, agenthub, config::HubConfig, machines, mcp::Hub, ops, serve, store, web};

pub(crate) struct Hosted {
    pub(crate) url: String,
    pub(crate) http: reqwest::Client,
    pub(crate) accounts: Arc<identity::accounts::Accounts>,
    pub(crate) devices: Arc<machines::devices::Devices>,
    pub(crate) config: Arc<HubConfig>,
    /// The hub as the router serves it, for what a test drives under HTTP.
    pub(crate) hub: Hub,
    pub(crate) dir: tempfile::TempDir,
}

/// A person's browser and AI client after their first connect.
pub(crate) struct Person {
    pub(crate) cookie: String,
    pub(crate) token: String,
    pub(crate) session: Option<String>,
}

pub(super) async fn hosted() -> Option<Hosted> {
    hosted_on(None).await
}

/// A hosted hub whose store is `url` (a Postgres database under test), or a new database on
/// the test Postgres; `None` without a test Postgres.
async fn hosted_on(store: Option<&str>) -> Option<Hosted> {
    hosted_with(store, |_, _| json!({})).await
}

/// A hosted hub with more config: `extra(dir, url)` is merged in (its `auth` into `auth`).
pub(crate) async fn hosted_with(
    store: Option<&str>,
    extra: impl FnOnce(&std::path::Path, &str) -> Value,
) -> Option<Hosted> {
    hosted_with_ingress(
        store,
        extra,
        crate::agenthub::issue_lifecycle::IssueLifecycleStore::disabled(),
    )
    .await
}

pub(crate) async fn hosted_with_ingress(
    store: Option<&str>,
    extra: impl FnOnce(&std::path::Path, &str) -> Value,
    ingress: crate::agenthub::issue_lifecycle::IssueLifecycleStore,
) -> Option<Hosted> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let dir = tempfile::tempdir().unwrap();
    let mut value = json!({
        "listen": "127.0.0.1:1",
        "public_url": url,
        "auth": { "mode": "hub" },
        "tenancy": { "mode": "multi", "starts_per_hour": 100, "anonymous_connect": true },
        "account_boards": { "free_limit": 5 },
        "brokers": [{ "name": "wi1", "link": true }],
        "grants": []
    });
    for (key, more) in extra(dir.path(), &url).as_object().unwrap() {
        match (key.as_str(), value.get_mut(key)) {
            ("auth", Some(auth)) => {
                for (k, v) in more.as_object().unwrap() {
                    auth[k] = v.clone();
                }
            }
            _ => value[key] = more.clone(),
        }
    }
    let config = Arc::new(HubConfig::parse(&value.to_string()).unwrap());
    let store_url = match store {
        Some(url) => url.to_string(),
        None => store::testing::empty_postgres().await?,
    };
    // As `run` wires them, with what the owner saved on /admin over the file.
    let store = store::open(&store_url).await.unwrap();
    let accounts = Arc::new(identity::configured_accounts(store, &config).await.unwrap());
    ops::hub_settings::apply_saved(&config, &accounts)
        .await
        .unwrap();
    // As `run` wires it: a hosted hub counts unless its config says not to.
    let analytics = config.counts_visits().then(|| {
        ops::analytics::Analytics::start_with(accounts.store().clone(), config.live_handle(), &url)
    });
    // The real server uses one PostgreSQL store for accounts, machine enrollment and
    // access transactions. A second test database hides broken cross-owner wiring.
    let devices = Arc::new(machines::devices::Devices::new(accounts.store().clone()));
    let audit = crate::ops::audit::test_audit().await?;
    let changes = Arc::new(tokio::sync::Notify::new());
    let links = Arc::new(
        machines::link::Links::new(config.clone(), devices.clone(), audit.clone())
            .gated(accounts.enroll_gate())
            .counting(analytics.clone())
            .signalling(changes.clone()),
    );
    // As `run` wires them: phones on a hub with its own accounts, and the build it hands out
    // when the config names one (`workflows/tests.rs` does).
    let push = Some(Arc::new(
        crate::notify::push::Push::open(dir.path(), &url, devices.clone()).unwrap(),
    ));
    let downloads = config.downloads.as_ref().map(|dir| {
        Arc::new(crate::machines::downloads::Downloads::open(std::path::Path::new(dir)).unwrap())
    });
    let boards = Arc::new(agenthub::mcp::Boards::new(&config, accounts.store().clone()).unwrap());
    let board_changes = agenthub::mcp::Boards::postgres_listener(&store_url)
        .await
        .unwrap();
    tokio::spawn(boards.clone().listen_postgres(board_changes));
    tokio::spawn(boards.clone().reap_on_time());
    let state = AppState {
        verifier: Arc::new(
            Verifier::from_hub_config(&config, accounts.store(), Some(accounts.clone()))
                .await
                .unwrap(),
        ),
        hub: Hub {
            boards,
            config: config.clone(),
            audit,
            links,
            push,
            downloads,
            analytics,
            workflows: Some(Arc::new(crate::workflows::Workflows::new(
                accounts.clone(),
                changes,
                config.live_handle(),
            ))),
            offers: Some(Arc::new(
                crate::offers::Offers::from_config(&config, accounts.clone()).unwrap(),
            )),
            connections: Some(Arc::new(
                crate::connections::Connections::new(
                    accounts.clone(),
                    crate::identity::Sealer::ephemeral(),
                )
                .unwrap(),
            )),
            routed: Default::default(),
        },
        issue_store: Arc::new(ingress),
        web: None,
        passkeys: web::HubSignIn::new(config.clone(), accounts.clone())
            .map(|hub| hub.with_providers(web::Providers::from_config(&config).unwrap()))
            .map(Arc::new),
    };
    let hub = state.hub.clone();
    let app = serve::app(state);
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap()
    });
    Some(Hosted {
        url,
        http: reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap(),
        accounts,
        devices,
        config,
        hub,
        dir,
    })
}

pub(crate) fn set_cookie(response: &reqwest::Response, name: &str) -> Option<String> {
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|v| v.starts_with(&format!("{name}=")))
        .map(|v| v.split(';').next().unwrap().to_string())
}

pub(crate) fn query_param(url: &str, key: &str) -> Option<String> {
    url.split_once('?')?.1.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == key).then(|| {
            percent_encoding::percent_decode_str(v)
                .decode_utf8_lossy()
                .into_owned()
        })
    })
}

impl Hosted {
    pub(crate) async fn get(&self, path: &str, cookie: Option<&str>) -> reqwest::Response {
        let mut request = self.http.get(format!("{}{path}", self.url));
        if let Some(cookie) = cookie {
            request = request.header(header::COOKIE, cookie);
        }
        request.send().await.unwrap()
    }

    pub(crate) async fn form(
        &self,
        path: &str,
        cookie: Option<&str>,
        form: &[(&str, &str)],
    ) -> reqwest::Response {
        let mut request = self
            .http
            .post(format!("{}{path}", self.url))
            .header(header::ORIGIN, &self.url)
            .form(form);
        if let Some(cookie) = cookie {
            request = request.header(header::COOKIE, cookie);
        }
        request.send().await.unwrap()
    }

    pub(crate) async fn text(response: reqwest::Response) -> (u16, String) {
        let status = response.status().as_u16();
        (status, response.text().await.unwrap())
    }

    /// Someone new connects an AI client: the consent page, one Connect, the code back at
    /// the client, the client's tokens. No sign-in, no form, no claim.
    pub(crate) async fn first_connect(&self, app: &str) -> Person {
        let redirect = "http://127.0.0.1:9/callback";
        let registered: Value = self
            .http
            .post(format!("{}/oauth/register", self.url))
            .json(&json!({ "client_name": app, "redirect_uris": [redirect] }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let client = registered["client_id"].as_str().unwrap().to_string();
        let verifier = "v".repeat(64);
        let challenge = s256(&verifier);
        let mcp = format!("{}/mcp", self.url);
        let query = format!(
            "response_type=code&client_id={client}&redirect_uri={}&code_challenge={challenge}&code_challenge_method=S256&state=s1&resource={}",
            web_encode(redirect),
            web_encode(&mcp)
        );
        let (status, page) =
            Self::text(self.get(&format!("/oauth/authorize?{query}"), None).await).await;
        assert_eq!(status, 200, "{page}");
        assert!(page.contains(&format!("<h1>Connect {app}?</h1>")), "{page}");
        assert!(
            page.contains(&format!("your new {} account", self.config.site().name())),
            "{page}"
        );
        assert!(page.contains("nothing can recover it"), "{page}");
        let answer = self
            .form(
                "/oauth/authorize",
                None,
                &[
                    ("client_id", &client),
                    ("redirect_uri", redirect),
                    ("code_challenge", &challenge),
                    ("resource", &mcp),
                    ("scope", "mcp offline_access"),
                    ("state", "s1"),
                    ("decision", "approve"),
                ],
            )
            .await;
        assert_eq!(answer.status(), 303);
        let cookie = set_cookie(&answer, "dg_session").expect("the new account's session");
        let location = answer.headers()[header::LOCATION]
            .to_str()
            .unwrap()
            .to_string();
        assert!(location.starts_with(redirect), "{location}");
        assert_eq!(query_param(&location, "state").as_deref(), Some("s1"));
        let code = query_param(&location, "code").expect("a code");
        let tokens: Value = self
            .http
            .post(format!("{}/oauth/token", self.url))
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", &code),
                ("redirect_uri", redirect),
                ("client_id", &client),
                ("code_verifier", &verifier),
            ])
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        Person {
            cookie,
            token: tokens["access_token"].as_str().unwrap().to_string(),
            session: None,
        }
    }

    /// An MCP request as `person`'s AI client; the answer's `result` (or the JSON-RPC error).
    pub(crate) async fn mcp(&self, person: &mut Person, method: &str, params: Value) -> Value {
        if person.session.is_none() && method != "initialize" {
            let init = self
                .http
                .post(format!("{}/mcp", self.url))
                .bearer_auth(&person.token)
                .header(header::ACCEPT, "application/json, text/event-stream")
                .json(
                    &json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {
                    "protocolVersion": "2025-06-18", "capabilities": {},
                    "clientInfo": { "name": "tenant_tests", "version": "1" } } }),
                )
                .send()
                .await
                .unwrap();
            assert_eq!(init.status(), 200);
            let session = init.headers()["mcp-session-id"]
                .to_str()
                .unwrap()
                .to_string();
            self.http
                .post(format!("{}/mcp", self.url))
                .bearer_auth(&person.token)
                .header(header::ACCEPT, "application/json, text/event-stream")
                .header("mcp-session-id", &session)
                .json(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
                .send()
                .await
                .unwrap();
            person.session = Some(session);
        }
        let answer = self
            .http
            .post(format!("{}/mcp", self.url))
            .bearer_auth(&person.token)
            .header(header::ACCEPT, "application/json, text/event-stream")
            .header("mcp-session-id", person.session.as_deref().unwrap())
            .json(&json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }))
            .send()
            .await
            .unwrap();
        assert_eq!(answer.status(), 200, "{method}");
        let text = answer.text().await.unwrap();
        // JSON, or one server-sent event carrying it.
        let json = text
            .lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .map(str::trim)
            .find(|data| data.starts_with('{'))
            .unwrap_or(text.trim());
        let body: Value = serde_json::from_str(json).unwrap_or_else(|_| panic!("{method}: {text}"));
        body.get("result").cloned().unwrap_or(body)
    }

    /// One tool call. A successful result is held to the tool's declared output schema, as
    /// a client that checks holds it (`mcp/output_schema.rs`): every test that calls a tool
    /// through here checks that too.
    pub(crate) async fn call(&self, person: &mut Person, tool: &str, arguments: Value) -> Value {
        let answer = self
            .mcp(
                person,
                "tools/call",
                json!({ "name": tool, "arguments": arguments }),
            )
            .await;
        crate::mcp::output_schema::assert_result(tool, &answer);
        answer
    }
}

pub(crate) fn web_encode(value: &str) -> String {
    percent_encoding::utf8_percent_encode(value, percent_encoding::NON_ALPHANUMERIC).to_string()
}

/// Whether a tool call came back refused (an error result, or a JSON-RPC error).
pub(crate) fn refused(answer: &Value) -> bool {
    answer["isError"] == true || answer.get("error").is_some() || answer.get("code").is_some()
}

pub(crate) use super::schema::fill;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tenant_a_reaches_nothing_of_tenant_bs_through_any_tool_or_page() {
    isolation(or_skip!(hosted())).await;
}

async fn isolation(hub: Hosted) {
    let mut alice = hub.first_connect("Claude").await;
    let mut bob = hub.first_connect("Codex").await;

    // Each is a new account, not secured, in a tenant of its own.
    let who_a = hub.call(&mut alice, "whoami", json!({})).await["structuredContent"].clone();
    let who_b = hub.call(&mut bob, "whoami", json!({})).await["structuredContent"].clone();
    let (tenant_a, tenant_b) = (
        who_a["tenant"].as_str().unwrap(),
        who_b["tenant"].as_str().unwrap(),
    );
    assert!(tenant_a.starts_with("t-") && tenant_b.starts_with("t-") && tenant_a != tenant_b);
    assert_ne!(who_a["subject"], who_b["subject"]);
    assert_eq!(who_a["via"], "hub");
    assert_eq!(who_a["owner"], false);
    assert!(who_a["provisional_until"].as_u64().is_some());
    assert_eq!(
        who_a["secure_url"].as_str(),
        Some(format!("{}/account#secure", hub.url).as_str())
    );
    // Neither reaches the operator's configured machine.
    assert_eq!(who_a["brokers"], json!([]));

    // B's things: a machine that joined, a board with a ticket, an approval waiting.
    let added = hub
        .call(&mut bob, "machine_add", json!({ "name": "laptop" }))
        .await;
    let added = &added["structuredContent"];
    assert_eq!(added["name"], "laptop", "B sees the name it gave");
    let stored = added["manual"]
        .as_str()
        .unwrap()
        .split("--device ")
        .nth(1)
        .unwrap()
        .split(' ')
        .next()
        .unwrap()
        .to_string();
    assert!(
        stored.ends_with("-laptop") && stored.starts_with('t'),
        "{stored}"
    );
    hub.enroll(&stored, added["code"].as_str().unwrap(), "bob-laptop-key")
        .await
        .unwrap();
    let bound = hub
        .call(&mut bob, "board_bind", json!({ "project": "secret-plans" }))
        .await;
    assert!(!refused(&bound), "{bound}");
    let ticket = hub
        .call(
            &mut bob,
            "board_create",
            json!({ "project": "secret-plans", "title": "BOB-PRIVATE-TICKET" }),
        )
        .await;
    let ticket_id = ticket["structuredContent"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let approval = hub
        .call(
            &mut bob,
            "approval_request",
            json!({ "project": "secret-plans", "title": "BOB-PRIVATE-APPROVAL", "what": "x" }),
        )
        .await;
    let approval_id = approval["structuredContent"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let b_machines = hub.call(&mut bob, "machines", json!({})).await["structuredContent"].clone();
    assert_eq!(b_machines["machines"][0]["name"], "laptop");
    let b_ticket = hub
        .call(
            &mut bob,
            "board_get",
            json!({ "project": "secret-plans", "id": ticket_id }),
        )
        .await["structuredContent"]
        .clone();

    // What must never reach A.
    let secrets = [
        who_b["subject"].as_str().unwrap().to_string(),
        tenant_b.to_string(),
        stored.clone(),
        "BOB-PRIVATE-TICKET".to_string(),
        "BOB-PRIVATE-APPROVAL".to_string(),
        "bob-laptop-key".to_string(),
    ];
    // A name A itself sent may come back in a refusal ("no broker named …"): that tells A
    // nothing it did not say.
    let leaks_beyond = |what: &str, text: &str, sent: &str| {
        for secret in secrets
            .iter()
            .filter(|secret| !sent.contains(secret.as_str()))
        {
            assert!(
                !text.contains(secret.as_str()),
                "{what} revealed {secret}: {text}"
            );
        }
    };
    let leaks = |what: &str, text: &str| leaks_beyond(what, text, "");

    // Every tool the hub serves, as A, with B's ids wherever a tool takes one: B's machine
    // by the name B knows it and by the name it is stored under. The two that make
    // something of A's own (a machine, a board) go last, so the others meet only B's.
    let listed = hub.mcp(&mut alice, "tools/list", json!({})).await;
    let tools: Vec<Value> = listed["tools"].as_array().unwrap().clone();
    assert!(tools.len() >= 30, "{} tools", tools.len());
    let making = |tool: &Value| matches!(tool["name"].as_str(), Some("board_bind" | "machine_add"));
    let ids = |broker: &str| {
        json!({
            "broker": broker, "machine": broker, "project": "secret-plans", "id": ticket_id, "ticket": ticket_id,
            "operation_id": "op-1", "path": "/etc/hostname", "paths": ["/etc/hostname"],
            "source": "/etc/hostname", "destination": "/tmp/x", "name": broker,
            "command": "id", "title": "from A", "what": "x",
        })
    };
    let mut driven = std::collections::BTreeSet::new();
    for broker in ["laptop", stored.as_str()] {
        for tool in tools.iter().filter(|tool| !making(tool)) {
            let name = tool["name"].as_str().unwrap();
            let arguments = fill(&tool["inputSchema"], &tool["inputSchema"], &ids(broker));
            let answer = hub.call(&mut alice, name, arguments.clone()).await;
            leaks_beyond(name, &answer.to_string(), &arguments.to_string());
            // `workflow_list` takes nothing: it lists the site's workflows, never anyone's.
            // `offers` and `offers_accept` name no machine, board or workflow: they read
            // and keep the caller's own tenant's yes, never another's. `connections` lists the
            // caller's own tenant's connections only (`connection_call` with B's name is
            // refused below like every other tool). `get_recent_tool_calls`, `get_usage_stats`
            // and the lexicon's tools read the caller's own audit or words, whatever it names.
            const OWN: [&str; 8] = [
                "whoami",
                "machines",
                "workflow_list",
                "offers",
                "offers_accept",
                "connections",
                "get_recent_tool_calls",
                "get_usage_stats",
            ];
            if !OWN.contains(&name) && !crate::kernel::TOOLS.contains(&name) {
                // Every other tool names a machine, a board or a workflow of B's here:
                // refused by the tool itself, never for want of arguments it could read.
                assert!(
                    refused(&answer),
                    "{name}({arguments}) was not refused: {answer}"
                );
                assert!(
                    !answer.to_string().contains("failed to deserialize"),
                    "{name}({arguments}) never reached the tool: {answer}"
                );
            }
            driven.insert(name.to_string());
        }
        // The routed tools fail at the route, not on their arguments: B's machine is
        // unknown to A by either name.
        for (tool, arguments) in [
            ("run", json!({ "machine": broker, "command": "id" })),
            ("machine_inspect", json!({ "machine": broker })),
            (
                "read_file",
                json!({ "machine": broker, "path": "/etc/hostname" }),
            ),
        ] {
            let answer = hub.call(&mut alice, tool, arguments).await;
            assert!(
                answer.to_string().contains("unknown_broker"),
                "{tool}: {answer}"
            );
        }
        let answer = hub
            .call(
                &mut alice,
                "board_get",
                json!({ "project": "secret-plans", "id": ticket_id }),
            )
            .await;
        assert!(answer.to_string().contains("unknown_board"), "{answer}");
    }
    for tool in tools.iter().filter(|tool| making(tool)) {
        let name = tool["name"].as_str().unwrap();
        let arguments = fill(&tool["inputSchema"], &tool["inputSchema"], &ids("laptop"));
        let answer = hub.call(&mut alice, name, arguments).await;
        leaks(name, &answer.to_string());
        assert!(!refused(&answer), "A makes its own {name}: {answer}");
        driven.insert(name.to_string());
    }
    let served: std::collections::BTreeSet<String> = tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(driven, served, "every tool was driven");
    // A's own board of the same name is A's: B's ticket is not on it.
    let answer = hub
        .call(
            &mut alice,
            "board_get",
            json!({ "project": "secret-plans", "id": ticket_id }),
        )
        .await;
    assert!(refused(&answer), "{answer}");
    leaks("board_get", &answer.to_string());
    // A's own "laptop" is A's, beside B's: names are each tenant's own.
    let a_machines = hub.call(&mut alice, "machines", json!({})).await["structuredContent"].clone();
    leaks("machines", &a_machines.to_string());
    assert_eq!(
        a_machines["machines"].as_array().unwrap().len(),
        0,
        "A's own laptop has not joined yet: {a_machines}"
    );

    // Every account page, as A in A's browser, with B's ids.
    for path in [
        "/",
        "/account",
        "/approvals",
        "/api/v1/me",
        "/device",
        "/claim",
    ] {
        let (_, page) = Hosted::text(hub.get(path, Some(&alice.cookie)).await).await;
        leaks(path, &page);
    }
    let (status, _) = Hosted::text(
        hub.form(
            "/machines/revoke",
            Some(&alice.cookie),
            &[("name", "laptop")],
        )
        .await,
    )
    .await;
    assert_ne!(status, 500);
    let (_, page) = Hosted::text(
        hub.form(
            "/machines/revoke",
            Some(&alice.cookie),
            &[("name", stored.as_str())],
        )
        .await,
    )
    .await;
    leaks_beyond("/machines/revoke", &page, &stored);
    let (_, page) = Hosted::text(
        hub.form(
            "/approvals/decide",
            Some(&alice.cookie),
            &[
                ("project", "secret-plans"),
                ("id", &approval_id),
                ("version", "1"),
                ("verdict", "approved"),
            ],
        )
        .await,
    )
    .await;
    leaks("/approvals/decide", &page);
    let me: Value = hub
        .http
        .get(format!("{}/api/v1/me", hub.url))
        .bearer_auth(&alice.token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    leaks("/api/v1/me", &me.to_string());

    // B's things are as they were.
    let device = hub.devices.get(&stored).await.unwrap().unwrap();
    assert!(device.revoked_at.is_none(), "B's machine is not revoked");
    assert_eq!(device.owner.as_deref(), who_b["subject"].as_str());
    assert_eq!(
        hub.call(&mut bob, "machines", json!({})).await["structuredContent"],
        b_machines
    );
    assert_eq!(
        hub.call(
            &mut bob,
            "board_get",
            json!({ "project": "secret-plans", "id": ticket_id })
        )
        .await["structuredContent"],
        b_ticket
    );
    let waiting = hub
        .call(
            &mut bob,
            "approval_wait",
            json!({ "project": "secret-plans", "id": approval_id, "timeout_ms": 0 }),
        )
        .await;
    assert_eq!(
        waiting["structuredContent"]["status"], "pending",
        "{waiting}"
    );

    // Each tenant's chain is whole and holds its own steps only.
    for (tenant, account) in [(tenant_a, &who_a["subject"]), (tenant_b, &who_b["subject"])] {
        assert!(hub.accounts.chain_intact(tenant).await.unwrap());
        for event in hub.accounts.events(tenant, 100).await.unwrap() {
            assert!(
                event.actor == "hub" || json!(event.actor) == *account,
                "{event:?}"
            );
        }
    }
}
