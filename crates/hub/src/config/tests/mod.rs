//! Parsing the hub's config: each section's checks, the environment over the file, and
//! what a parsed config answers.

use super::*;

/// Absolute paths in the platform's own form: `/…` is not absolute on Windows.
#[cfg(unix)]
const TOKENS: &str = "/tmp/tokens.json";
#[cfg(windows)]
const TOKENS: &str = r"C:\tmp\tokens.json";
#[cfg(unix)]
const SOCKET: &str = "/run/broker.sock";
#[cfg(windows)]
const SOCKET: &str = r"\\.\pipe\devagotchi-broker-test";

fn good() -> serde_json::Value {
    serde_json::json!({
        "listen": "127.0.0.1:8787",
        "public_url": "http://127.0.0.1:8787",
        "auth": { "mode": "dev", "tokens_file": TOKENS },
        "brokers": [ { "name": "gold-ws", "socket": SOCKET } ],
        "grants": [ { "subject": "jesse", "brokers": ["gold-ws"] } ]
    })
}

fn parse(value: &serde_json::Value) -> BridgeResult<HubConfig> {
    HubConfig::parse(&value.to_string())
}

#[test]
fn accepts_a_minimal_dev_config_and_resolves_grants() {
    let config = parse(&good()).unwrap();
    assert_eq!(config.brokers_for("jesse").len(), 1);
    assert!(config.brokers_for("someone-else").is_empty());
    assert!(config.broker("gold-ws").is_some());
}

#[test]
fn account_boards_need_real_account_ids_and_no_directory() {
    let mut hosted = good();
    hosted["account_boards"] = serde_json::json!({});
    let config = parse(&hosted).unwrap();
    let accounts = config.account_boards.as_ref().unwrap();
    assert!(accounts.free_limit.is_none() && accounts.unlimited.is_empty());

    // Boards are in the hub's Postgres: there is no directory to name.
    let mut dir = good();
    dir["account_boards"] = serde_json::json!({ "dir": "/srv/boards" });
    assert_eq!(parse(&dir).unwrap_err().code, "invalid_config");

    let mut email = good();
    email["account_boards"] = serde_json::json!({ "unlimited": ["someone@example.test"] });
    assert_eq!(parse(&email).unwrap_err().code, "invalid_config");

    let mut unknown = good();
    unknown["account_boards"] = serde_json::json!({ "limit": 3 });
    assert_eq!(
        parse(&unknown).unwrap_err().code,
        "invalid_config",
        "unknown field fails closed"
    );
}

#[test]
fn dev_tokens_are_for_tests_a_reachable_hub_refuses_them() {
    for public_url in [
        "https://hub.example",
        "http://hub.example:8787",
        "https://127.0.0.1",
    ] {
        let mut reachable = good();
        reachable["public_url"] = serde_json::json!(public_url);
        let error = parse(&reachable).unwrap_err();
        assert_eq!(error.code, "invalid_config", "{public_url}");
        assert!(
            error.message.contains("nomen or oidc"),
            "{public_url}: {}",
            error.message
        );
    }
    for public_url in ["http://localhost:8787", "http://[::1]:8787"] {
        let mut local = good();
        local["public_url"] = serde_json::json!(public_url);
        parse(&local).unwrap_or_else(|e| panic!("{public_url}: {}", e.message));
    }
}

#[test]
fn unknown_fields_unknown_brokers_and_bad_names_fail_closed() {
    let mut unknown = good();
    let grants = unknown.as_object_mut().unwrap().remove("grants").unwrap();
    unknown["grant"] = grants;
    assert_eq!(parse(&unknown).unwrap_err().code, "invalid_config");

    let mut dangling = good();
    dangling["grants"][0]["brokers"] = serde_json::json!(["other"]);
    assert_eq!(parse(&dangling).unwrap_err().code, "invalid_config");

    let mut bad_name = good();
    bad_name["brokers"][0]["name"] = serde_json::json!("Gold WS");
    assert_eq!(parse(&bad_name).unwrap_err().code, "invalid_config");

    let mut relative = good();
    relative["brokers"][0]["socket"] = serde_json::json!("broker.sock");
    assert_eq!(parse(&relative).unwrap_err().code, "invalid_config");

    let mut both = good();
    both["brokers"][0]["link"] = serde_json::json!(true);
    assert_eq!(parse(&both).unwrap_err().code, "invalid_config");

    let mut neither = good();
    neither["brokers"][0]
        .as_object_mut()
        .unwrap()
        .remove("socket");
    assert_eq!(parse(&neither).unwrap_err().code, "invalid_config");

    let mut relative_tokens = good();
    relative_tokens["auth"]["tokens_file"] = serde_json::json!("tokens.json");
    assert_eq!(parse(&relative_tokens).unwrap_err().code, "invalid_config");
}

#[test]
fn a_linked_broker_needs_no_socket() {
    let mut linked = good();
    linked["brokers"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({ "name": "gus", "link": true }));
    linked["grants"][0]["brokers"] = serde_json::json!(["gold-ws", "gus"]);
    let config = parse(&linked).unwrap();
    assert!(config.broker("gus").unwrap().link);
    assert_eq!(config.brokers_for("jesse").len(), 2);
}

#[test]
fn oidc_requires_https_issuer_and_an_audience() {
    let mut oidc = good();
    oidc["auth"] =
        serde_json::json!({ "mode": "oidc", "issuer": "https://id.example", "audiences": ["hub"] });
    assert!(parse(&oidc).is_ok());

    let mut no_aud = oidc.clone();
    no_aud["auth"]["audiences"] = serde_json::json!([]);
    assert_eq!(parse(&no_aud).unwrap_err().code, "invalid_config");

    let mut plain = oidc.clone();
    plain["auth"]["issuer"] = serde_json::json!("http://id.example");
    assert_eq!(parse(&plain).unwrap_err().code, "invalid_config");
}

fn nomen() -> serde_json::Value {
    let mut config = good();
    config["auth"] = serde_json::json!({
        "mode": "nomen",
        "issuer": "https://id.shippin.cloud",
        "audiences": ["300000000000000500"],
        "clients": ["300000000000000600"],
        "key_file": TOKENS,
    });
    config["owner"] = serde_json::json!("300000000000000001");
    config["brokers"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({ "name": "gold-host", "link": true }));
    config
}

#[test]
fn trusted_issuers_must_be_https_distinct_and_never_beside_dev_tokens() {
    let site = serde_json::json!({
        "issuer": "https://site.example/auth/v1", "audiences": ["authenticated"] });
    let mut config = nomen();
    config["trusted_issuers"] = serde_json::json!([site.clone()]);
    assert_eq!(parse(&config).unwrap().trusted_issuers.len(), 1);

    config["trusted_issuers"] = serde_json::json!([site.clone(), site.clone()]);
    assert!(parse(&config).unwrap_err().message.contains("listed twice"));

    config["trusted_issuers"] = serde_json::json!([{
        "issuer": "https://id.shippin.cloud", "audiences": ["x"] }]);
    assert!(parse(&config).unwrap_err().message.contains("listed twice"));

    config["trusted_issuers"] = serde_json::json!([{
        "issuer": "http://site.example", "audiences": ["authenticated"] }]);
    assert!(parse(&config).unwrap_err().message.contains("https"));

    config["trusted_issuers"] = serde_json::json!([{
        "issuer": "https://site.example", "audiences": [] }]);
    assert!(parse(&config).unwrap_err().message.contains("audience"));

    let mut dev = good();
    dev["trusted_issuers"] = serde_json::json!([site]);
    assert!(parse(&dev).unwrap_err().message.contains("dev tokens"));
}

#[test]
fn nomen_sign_in_needs_an_owner_who_reaches_every_broker() {
    let config = parse(&nomen()).unwrap();
    assert!(config.is_owner("300000000000000001"));
    let owned: Vec<_> = config
        .brokers_for("300000000000000001")
        .iter()
        .map(|b| b.name.as_str())
        .collect();
    assert_eq!(owned, ["gold-ws", "gold-host"]);
    assert_eq!(config.brokers_for("jesse").len(), 1);
    assert!(!config.is_owner("jesse"));

    let mut ownerless = nomen();
    ownerless.as_object_mut().unwrap().remove("owner");
    assert_eq!(parse(&ownerless).unwrap_err().code, "invalid_config");

    let mut no_key = nomen();
    no_key["auth"]["key_file"] = serde_json::json!("hub-key.json");
    assert_eq!(parse(&no_key).unwrap_err().code, "invalid_config");

    let mut no_aud = nomen();
    no_aud["auth"]["audiences"] = serde_json::json!([" "]);
    assert_eq!(parse(&no_aud).unwrap_err().code, "invalid_config");

    let mut any_client = nomen();
    any_client["auth"]["clients"] = serde_json::json!([]);
    assert_eq!(parse(&any_client).unwrap_err().code, "invalid_config");
}

#[test]
fn web_sign_in_needs_nomen_an_accepted_client_and_https() {
    let mut web = nomen();
    web["auth"]["clients"] = serde_json::json!(["shippin-mcp", "devagotchi-web"]);
    web["web"] = serde_json::json!({ "client_id": "devagotchi-web" });
    assert_eq!(
        parse(&web).unwrap().web.unwrap().client_id,
        "devagotchi-web"
    );

    // The web app's tokens are checked like any other: it must be an accepted client.
    let mut unlisted = web.clone();
    unlisted["web"]["client_id"] = serde_json::json!("some-other-app");
    assert_eq!(parse(&unlisted).unwrap_err().code, "invalid_config");

    let mut plain = web.clone();
    plain["public_url"] = serde_json::json!("http://hub.example");
    assert_eq!(parse(&plain).unwrap_err().code, "invalid_config");
    plain["public_url"] = serde_json::json!("http://127.0.0.1:8787");
    assert!(parse(&plain).is_ok(), "plain http on loopback, for tests");

    let mut dev = good();
    dev["web"] = serde_json::json!({ "client_id": "devagotchi-web" });
    assert_eq!(parse(&dev).unwrap_err().code, "invalid_config");

    let mut extra = web;
    extra["web"]["secret"] = serde_json::json!("x");
    assert_eq!(parse(&extra).unwrap_err().code, "invalid_config");
}

#[test]
fn an_email_or_a_name_is_never_an_identity() {
    let mut email_owner = nomen();
    email_owner["owner"] = serde_json::json!("jessay@gmail.com");
    let error = parse(&email_owner).unwrap_err();
    assert_eq!(error.code, "invalid_config");
    assert!(error.message.contains("account ID"), "{}", error.message);

    let mut email_grant = good();
    email_grant["grants"][0]["subject"] = serde_json::json!("jessay@gmail.com");
    assert_eq!(parse(&email_grant).unwrap_err().code, "invalid_config");

    let mut name_grant = good();
    name_grant["grants"][0]["subject"] = serde_json::json!("Pat Smith");
    assert_eq!(parse(&name_grant).unwrap_err().code, "invalid_config");
}

#[test]
fn plain_http_issuers_only_on_loopback() {
    for issuer in [
        "http://127.0.0.1:9999",
        "http://localhost:1",
        "http://[::1]:2",
    ] {
        let mut local = nomen();
        local["auth"]["issuer"] = serde_json::json!(issuer);
        assert!(parse(&local).is_ok(), "{issuer}");
    }
    for issuer in [
        "http://id.shippin.cloud",
        "http://127.0.0.1.evil.example",
        "http://user@127.0.0.1:1",
        "ftp://127.0.0.1",
    ] {
        let mut remote = nomen();
        remote["auth"]["issuer"] = serde_json::json!(issuer);
        assert_eq!(
            parse(&remote).unwrap_err().code,
            "invalid_config",
            "{issuer}"
        );
    }
}

#[test]
fn boards_are_granted_like_brokers_and_people_are_declared() {
    let mut with_boards = good();
    with_boards["boards"] = serde_json::json!([{ "project": "devagotchi" }]);
    with_boards["grants"] = serde_json::json!([
        { "subject": "jesse", "brokers": ["gold-ws"], "boards": ["devagotchi"], "human": true },
        { "subject": "claude-on-gus", "boards": ["devagotchi"] },
        { "subject": "reader", "brokers": ["gold-ws"] }
    ]);
    let config = parse(&with_boards).unwrap();
    assert_eq!(config.boards_for("jesse").len(), 1);
    assert_eq!(config.boards_for("claude-on-gus").len(), 1);
    assert!(config.boards_for("reader").is_empty());
    assert!(config.boards_for("nobody").is_empty());
    assert!(config.is_human("jesse"));
    assert!(
        !config.is_human("claude-on-gus"),
        "an agent's grant never makes it a person"
    );
    assert!(
        config.brokers_for("claude-on-gus").is_empty(),
        "a board grant is not a broker grant"
    );
    assert!(config.board("devagotchi").is_some());

    // A hub may serve boards alone, with no broker at all, or nothing yet.
    let mut boards_only = with_boards.clone();
    boards_only["brokers"] = serde_json::json!([]);
    boards_only["grants"] = serde_json::json!([{ "subject": "jesse", "boards": ["devagotchi"] }]);
    assert!(parse(&boards_only).is_ok());
    let mut nothing = boards_only.clone();
    nothing["boards"] = serde_json::json!([]);
    nothing["grants"] = serde_json::json!([]);
    let empty = parse(&nothing).unwrap();
    assert!(empty.brokers_for("jesse").is_empty() && empty.boards_for("jesse").is_empty());

    let mut dangling = with_boards.clone();
    dangling["grants"][1]["boards"] = serde_json::json!(["other"]);
    assert_eq!(parse(&dangling).unwrap_err().code, "invalid_config");

    // A board is in the hub's Postgres: a directory for it is refused.
    let mut dir = with_boards.clone();
    dir["boards"][0]["dir"] = serde_json::json!("/srv/projects/devagotchi");
    assert_eq!(parse(&dir).unwrap_err().code, "invalid_config");

    let mut bad_name = with_boards.clone();
    bad_name["boards"][0]["project"] = serde_json::json!("Devagotchi Main");
    assert_eq!(parse(&bad_name).unwrap_err().code, "invalid_config");

    let mut twice = with_boards;
    twice["boards"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({ "project": "devagotchi" }));
    assert_eq!(parse(&twice).unwrap_err().code, "invalid_config");
}

mod hub_mode;
