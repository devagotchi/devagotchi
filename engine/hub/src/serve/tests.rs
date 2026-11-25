//! What `run` builds for each auth mode, short of listening, and the router it serves.

use super::*;
use axum::http::StatusCode;

use crate::{config::HubConfig, test_keys};

/// What `run` builds for an `auth` block, short of listening: the accounts and passkeys
/// of `auth.mode hub`, and the verifier.
async fn state_for(
    dir: &std::path::Path,
    auth: serde_json::Value,
    owner: Option<&str>,
) -> Option<AppState> {
    let mut value = serde_json::json!({
        "listen": "127.0.0.1:1",
        "public_url": "http://127.0.0.1:9",
        "auth": auth,
        "brokers": [],
        "grants": []
    });
    if let Some(owner) = owner {
        value["owner"] = owner.into();
    }
    let config = Arc::new(HubConfig::parse(&value.to_string()).unwrap());
    config::prepare_state_dir(dir).unwrap();
    let accounts = if config.has_accounts() {
        Some(Arc::new(
            identity::accounts::Accounts::open(store::testing::postgres().await?.store)
                .await
                .unwrap()
                .with_tokens(config.token_config()),
        ))
    } else {
        None
    };
    let passkeys = accounts
        .clone()
        .and_then(|accounts| web::HubSignIn::new(config.clone(), accounts).map(Arc::new));
    let devices = machines::devices::test_registry().await?;
    let audit = crate::ops::audit::test_audit().await?;
    let links = Arc::new(Links::new(config.clone(), devices, audit.clone()));
    let test_store = crate::store::testing::postgres().await?;
    Some(AppState {
        verifier: Arc::new(
            Verifier::from_hub_config(&config, &test_store.store, accounts)
                .await
                .unwrap(),
        ),
        hub: Hub {
            boards: Arc::new(
                agenthub::mcp::Boards::new(&config, test_store.store.clone()).unwrap(),
            ),
            config,
            audit,
            links,
            push: None,
            downloads: None,
            analytics: None,
            workflows: None,
            offers: None,
            connections: None,
            routed: Default::default(),
        },
        issue_store: Arc::new(crate::agenthub::issue_lifecycle::IssueLifecycleStore::disabled()),
        web: None,
        passkeys,
    })
}

/// Every auth mode builds the router (axum panics on a path mounted twice) and mounts
/// `/claim`, `/.well-known/oauth-authorization-server` and `/oauth/*` only where the
/// hub has its own accounts. There is one account system: no `/token`, no admin settings.
#[tokio::test]
async fn app_builds_in_every_auth_mode() {
    let dir = tempfile::tempdir().unwrap();
    let private = |name: &str, text: String| -> String {
        let path = dir.path().join(name);
        std::fs::write(&path, text).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        path.to_string_lossy().into_owned()
    };
    let tokens = private(
        "tokens.json",
        serde_json::json!({ "dev-token-0123456789": "jesse" }).to_string(),
    );
    let key = test_keys::rsa();
    let key_file = private(
        "nomen-key.json",
        serde_json::json!({
            "type": "application", "keyId": "k", "key": key.pem, "appId": "1", "clientId": "c"
        })
        .to_string(),
    );
    let nomen = serde_json::json!({
        "issuer": "https://id.example.test",
        "audiences": ["project"],
        "clients": ["client"],
        "key_file": key_file
    });
    let mut nomen_mode = nomen.clone();
    nomen_mode["mode"] = "nomen".into();
    let cases = [
        (
            "dev",
            serde_json::json!({ "mode": "dev", "tokens_file": tokens }),
            None,
        ),
        ("nomen", nomen_mode, Some("300000000000000001")),
        ("hub", serde_json::json!({ "mode": "hub" }), None),
        (
            "hub-nomen",
            serde_json::json!({ "mode": "hub", "nomen": nomen }),
            None,
        ),
    ];
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    for (mode, auth, owner) in cases {
        let state = or_skip!(state_for(&dir.path().join(mode), auth, owner));
        let hub_mode = mode.starts_with("hub");
        assert_eq!(state.passkeys.is_some(), hub_mode, "{mode}");
        // Building the router is the check that matters: a path mounted twice panics.
        let app = app(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .unwrap()
        });

        let health = http.get(format!("{base}/healthz")).send().await.unwrap();
        assert_eq!(health.status(), StatusCode::OK, "{mode}");
        let health: serde_json::Value = health.json().await.unwrap();
        assert!(health.get("auth").is_none(), "{mode}: {health}");

        let metadata = http
            .get(format!("{base}/.well-known/oauth-authorization-server"))
            .send()
            .await
            .unwrap();
        let status = metadata.status();
        let metadata: serde_json::Value = metadata.json().await.unwrap();
        let token_endpoint = metadata["token_endpoint"].as_str().unwrap_or_default();
        if hub_mode {
            assert_eq!(status, StatusCode::OK, "{mode}: {metadata}");
            assert_eq!(token_endpoint, "http://127.0.0.1:9/oauth/token", "{mode}");
        } else {
            assert_eq!(status, StatusCode::NOT_FOUND, "{mode}: {metadata}");
        }

        let claim = http.get(format!("{base}/claim")).send().await.unwrap();
        let claim_status = claim.status();
        let claim = claim.text().await.unwrap();
        if hub_mode {
            assert_eq!(claim_status, StatusCode::OK, "{mode}");
            assert!(claim.contains("Claim this hub"), "{mode}: {claim}");
        }

        // The hub's own token endpoint answers only in hub mode (elsewhere it is mounted
        // and says not_found).
        let token = http
            .post(format!("{base}/oauth/token"))
            .form(&[("grant_type", "authorization_code")])
            .send()
            .await
            .unwrap();
        assert_eq!(
            token.status() != StatusCode::NOT_FOUND,
            hub_mode,
            "{mode}: /oauth/token {}",
            token.status()
        );
        let token = http
            .post(format!("{base}/token"))
            .form(&[("grant_type", "authorization_code")])
            .send()
            .await
            .unwrap();
        assert_eq!(token.status(), StatusCode::NOT_FOUND, "{mode}: /token");
        let settings = http
            .get(format!("{base}/api/v1/admin/settings"))
            .send()
            .await
            .unwrap();
        assert_eq!(
            settings.status(),
            StatusCode::NOT_FOUND,
            "{mode}: admin settings"
        );
    }
}
