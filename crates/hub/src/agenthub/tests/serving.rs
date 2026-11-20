//! A real hub for the bound-project tests (`served`, `scanned`, `sourced`): served in process
//! on a free port, with dev tokens; an account signed in on disk; the board command line run
//! against it; one MCP tool call over HTTP.

use std::{collections::HashMap, ffi::OsString, path::Path, sync::Arc};

use serde_json::{Value, json};

use crate::{
    AppState,
    agenthub::cli::{self, Invocation},
    config::HubConfig,
    identity::auth::Verifier,
    login::{self, Account, Stored},
    machines::link::Links,
    mcp::Hub,
};

/// The account `tok-member` signs in as.
pub(crate) const MEMBER: &str = "300000000000000002";

pub(crate) async fn hub(dir: &Path) -> Option<String> {
    hub_with(dir, json!({}), &[]).await
}

/// The hub, with `extra` merged into its config and more dev tokens.
pub(crate) async fn hub_with(dir: &Path, extra: Value, tokens: &[(&str, &str)]) -> Option<String> {
    Some(hub_audited(dir, extra, tokens).await?.0)
}

/// [`hub_with`], and the audit chain it writes, for a test that reads the lines a call left.
pub(crate) async fn hub_audited(
    dir: &Path,
    extra: Value,
    tokens: &[(&str, &str)],
) -> Option<(String, std::sync::Arc<crate::ops::audit::Audit>)> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut value = json!({
        "listen": "127.0.0.1:1",
        "public_url": url,
        "auth": {"mode": "dev", "tokens_file": dir.join("tokens.json")},
        "brokers": [],
        "grants": [],
        "account_boards": {}
    });
    for (key, more) in extra.as_object().unwrap() {
        value[key] = more.clone();
    }
    let config = Arc::new(HubConfig::parse(&value.to_string()).unwrap());
    let devices = crate::machines::devices::test_registry().await?;
    let audit = crate::ops::audit::test_audit().await?;
    let links = Arc::new(Links::new(config.clone(), devices.clone(), audit.clone()));
    let store = crate::store::testing::postgres().await?.store;
    let state = AppState {
        verifier: Arc::new(Verifier::Dev(
            std::iter::once(("tok-member".to_string(), MEMBER.to_string()))
                .chain(tokens.iter().map(|(t, s)| (t.to_string(), s.to_string())))
                .collect::<HashMap<_, _>>(),
        )),
        hub: Hub {
            downloads: None,
            analytics: None,
            workflows: None,
            offers: None,
            connections: None,
            routed: Default::default(),
            push: None,
            boards: Arc::new(crate::agenthub::mcp::Boards::new(&config, store).unwrap()),
            config,
            audit: audit.clone(),
            links,
        },
        issue_store: Arc::new(crate::agenthub::issue_lifecycle::IssueLifecycleStore::disabled()),
        web: None,
        passkeys: None,
    };
    let app = crate::serve::app(state);
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    Some((url, audit))
}

/// `path` holds a signed-in account: [`MEMBER`], with `token`.
pub(crate) fn signed_in(path: &Path, token: &str) {
    let now = login::unix_now();
    login::write(
        path,
        &Stored::Account(Account::for_tests(
            "https://id.example",
            MEMBER,
            token,
            now + 3600,
        )),
    )
    .unwrap();
}

/// The board command line in `cwd`, signed in as `account`, as seat `actor`: its exit code,
/// its JSON (records and errors alike are on stdout), and everything it wrote.
pub(crate) fn run(
    cwd: &Path,
    account: &Path,
    actor: Option<&str>,
    args: &[&str],
) -> (u8, Value, String) {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = cli::run(
        Invocation {
            args: args.iter().map(OsString::from).collect(),
            cwd: cwd.to_path_buf(),
            actor_env: actor.map(str::to_string),
            session_env: None,
            os_user: Some("jesse".into()),
            stdout_is_terminal: false,
            account_file: Some(account.to_path_buf()),
            plugin_host: None,
            storage_env: None,
            hub: Some(std::sync::Arc::new(crate::login::client::AccountHub)),
        },
        &mut out,
        &mut err,
    );
    let out = String::from_utf8(out).unwrap();
    let value = serde_json::from_str(out.trim()).unwrap_or(Value::Null);
    (code, value, out + &String::from_utf8(err).unwrap())
}

/// One tool call over MCP as `token`, in a session of its own: initialize, then the call;
/// its `result`.
pub(crate) async fn tool(url: &str, token: &str, name: &str, arguments: Value) -> Value {
    tool_as(url, token, None, name, arguments).await
}

/// [`tool`], naming a seat (`X-Devagotchi-Seat`) when `seat` is given.
pub(crate) async fn tool_as(
    url: &str,
    token: &str,
    seat: Option<&str>,
    name: &str,
    arguments: Value,
) -> Value {
    let http = reqwest::Client::new();
    let post = |body: Value, session: Option<&str>| {
        let mut request = http
            .post(format!("{url}/mcp"))
            .bearer_auth(token)
            .header("accept", "application/json, text/event-stream")
            .json(&body);
        if let Some(seat) = seat {
            request = request.header(crate::agenthub::mcp::SEAT_HEADER, seat);
        }
        if let Some(session) = session {
            request = request.header("mcp-session-id", session);
        }
        request.send()
    };
    let init = post(
        json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18", "capabilities": {},
            "clientInfo": { "name": "board_tests", "version": "1" } } }),
        None,
    )
    .await
    .unwrap();
    let session = init.headers()["mcp-session-id"]
        .to_str()
        .unwrap()
        .to_string();
    post(
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        Some(&session),
    )
    .await
    .unwrap();
    let text = post(
        json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                "params": { "name": name, "arguments": arguments } }),
        Some(&session),
    )
    .await
    .unwrap()
    .text()
    .await
    .unwrap();
    let data = text
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim)
        .find(|data| data.starts_with('{'))
        .unwrap_or(text.trim());
    serde_json::from_str::<Value>(data).unwrap()["result"].clone()
}
