//! Every CLI call opens a fresh authenticated HTTP MCP session against a real hub.
use super::serving::{hub, run, signed_in};
use crate::agenthub::cli::{EXIT_CONFLICT, EXIT_OK};

pub(crate) async fn local_accounts_hub()
-> Option<(String, std::sync::Arc<crate::identity::accounts::Accounts>)> {
    use crate::{
        config::HubConfig,
        identity::{accounts::Accounts, auth::Verifier},
        machines::link::Links,
    };
    use std::sync::Arc;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let accounts = Arc::new(
        Accounts::open(crate::store::testing::postgres().await?.store)
            .await
            .unwrap(),
    );
    let config = Arc::new(
        HubConfig::parse(
            &serde_json::json!({
                "listen":"127.0.0.1:1", "public_url":url,
                "auth":{"mode":"hub"}, "brokers":[], "grants":[], "account_boards":{}
            })
            .to_string(),
        )
        .unwrap(),
    );
    let devices = crate::machines::devices::test_registry().await?;
    let audit = crate::ops::audit::test_audit().await?;
    let state = crate::AppState {
        verifier: Arc::new(
            Verifier::from_hub_config(&config, accounts.store(), Some(accounts.clone()))
                .await
                .unwrap(),
        ),
        hub: crate::mcp::Hub {
            downloads: None,
            analytics: None,
            workflows: None,
            offers: None,
            connections: None,
            routed: Default::default(),
            push: None,
            boards: Arc::new(
                crate::agenthub::mcp::Boards::new(&config, accounts.store().clone()).unwrap(),
            ),
            links: Arc::new(Links::new(config.clone(), devices, audit.clone())),
            config: config.clone(),
            audit,
        },
        issue_store: Arc::new(crate::agenthub::issue_lifecycle::IssueLifecycleStore::disabled()),
        web: None,
        passkeys: crate::web::HubSignIn::new(config, accounts.clone()).map(Arc::new),
    };
    tokio::spawn(async move {
        axum::serve(listener, crate::serve::app(state))
            .await
            .unwrap()
    });
    Some((url, accounts))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bound_cli_revoked_key_cannot_replay_and_replacement_keeps_request_identity() {
    use crate::identity::accounts::api_keys::{NewKey, Scope};
    let (url, accounts) = or_skip!(local_accounts_hub());
    let owner = accounts
        .ensure_owner(None, "Owner", [47; 16])
        .await
        .unwrap();
    let new_key = NewKey {
        name: "CLI retry test".into(),
        hours: 24,
        scopes: vec![Scope::BoardsRead, Scope::BoardsWrite],
        ..Default::default()
    };
    let (key, secret) = accounts
        .create_key(&owner.tenant, &owner.id, &new_key)
        .await
        .unwrap();
    let (replacement, replacement_secret) = accounts
        .create_key(&owner.tenant, &owner.id, &new_key)
        .await
        .unwrap();
    assert_ne!(key.id, replacement.id);
    let runtime = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("demo");
        std::fs::create_dir_all(&project).unwrap();
        let account_file = dir.path().join("account.json");
        let sign_in = |token: &str| {
            crate::login::write(
                &account_file,
                &crate::login::Stored::Account(crate::login::Account::for_tests(
                    &url,
                    &owner.id,
                    token,
                    crate::login::unix_now() + 3600,
                )),
            )
            .unwrap();
        };
        let ok = |args: &[&str]| {
            let (code, result, error) = run(&project, &account_file, None, args);
            assert_eq!(code, EXIT_OK, "{args:?}: {result} {error}");
            result
        };
        sign_in(&secret);
        ok(&["bind", "--hub", &url]);
        let create = [
            "new",
            "keep across key revocation",
            "--request-id",
            "revocation-retry",
        ];
        let original = ok(&create);
        assert_eq!(ok(&create)["replayed"], true);
        let before = ok(&["verify"])["entries"].clone();
        runtime
            .block_on(accounts.revoke_key(&owner.tenant, &owner.id, &key.id))
            .unwrap();
        let (code, error, _) = run(&project, &account_file, None, &create);
        assert_ne!(
            code, EXIT_OK,
            "revoked credential cannot disclose saved result: {error}"
        );
        sign_in(&replacement_secret);
        let replay = ok(&create);
        assert_eq!(replay["replayed"], true);
        assert_eq!(replay["id"], original["id"]);
        assert_eq!(ok(&["verify"])["entries"], before);
        for entry in std::fs::read_dir(project.join(".devagotchi/requests")).unwrap() {
            let path = entry.unwrap().path();
            if path
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                let record = std::fs::read_to_string(path).unwrap();
                assert!(!record.contains(&secret));
                assert!(!record.contains(&replacement_secret));
            }
        }
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bound_cli_replays_original_results_without_mutating_current_lease() {
    let dir = tempfile::tempdir().unwrap();
    let url = or_skip!(hub(dir.path()));
    let project = dir.path().join("demo");
    std::fs::create_dir_all(&project).unwrap();
    let account = dir.path().join("account.json");
    signed_in(&account, "tok-member");
    tokio::task::spawn_blocking(move || {
        let ok = |args: &[&str]| {
            let (code, result, error) = run(&project, &account, None, args);
            assert_eq!(code, EXIT_OK, "{args:?}: {result} {error}");
            result
        };
        ok(&["bind", "--hub", &url]);
        let bound_events = ok(&["verify"])["entries"].as_u64().unwrap();
        let create = [
            "new",
            "one task across sessions",
            "--request-id",
            "cli-create",
        ];
        let first = ok(&create);
        let id = first["id"].as_str().unwrap();
        let replay = ok(&create);
        assert_eq!(replay["id"], first["id"]);
        assert_eq!(replay["replayed"], true);
        assert_eq!(ok(&["verify"])["entries"], bound_events + 1);
        let note = [
            "note",
            id,
            "originally no fence",
            "--request-id",
            "cli-note",
        ];
        let noted = ok(&note);
        let claim = ["claim", id, "--request-id", "cli-claim"];
        let release = ["release", id, "--request-id", "cli-release"];
        let old = ok(&claim);
        ok(&release);
        assert_eq!(ok(&release)["replayed"], true, "cache was removed");
        let current = ok(&["claim", id, "--request-id", "cli-claim-new"]);
        assert!(current["fence"].as_i64().unwrap() > old["fence"].as_i64().unwrap());
        let cache = project.join(".devagotchi/claims/jesse.json");
        let original_cache = std::fs::read(&cache).unwrap();
        let replay = ok(&claim);
        assert_eq!(replay["replayed"], true);
        assert_eq!(replay["fence"], old["fence"]);
        assert_eq!(replay["lease_expires_at"], old["lease_expires_at"]);
        assert_eq!(ok(&release)["replayed"], true);
        let replayed_note = ok(&note);
        assert_eq!(replayed_note["seq"], noted["seq"]);
        assert_eq!(replayed_note["replayed"], true);
        assert_eq!(std::fs::read(&cache).unwrap(), original_cache);
        assert_eq!(ok(&["show", id])["ticket"], current["ticket"]);
        let keep = ["beat", id, "--request-id", "keep-progress"];
        ok(&["beat", id, "--progress", "{\"step\":4}"]);
        assert_eq!(ok(&keep)["ticket"]["progress"]["step"], 4);
        let (code, error, _) = run(
            &project,
            &account,
            None,
            &[
                "beat",
                id,
                "--request-id",
                "keep-progress",
                "--progress",
                "null",
            ],
        );
        assert_eq!(code, EXIT_CONFLICT, "{error}");
        assert_eq!(error["error"]["code"], "request_conflict");
        assert!(ok(&["beat", id, "--progress", "null"])["ticket"]["progress"].is_null());
        assert_eq!(ok(&keep)["replayed"], true);
        assert!(ok(&["show", id])["ticket"]["progress"].is_null());
        assert_eq!(ok(&["verify"])["entries"], bound_events + 8);
        // A known response is not authority: each fresh connection still authenticates.
        signed_in(&account, "no-longer-valid");
        let (code, error, _) = run(&project, &account, None, &create);
        assert_ne!(code, EXIT_OK, "unauthenticated replay must fail: {error}");
        signed_in(&account, "tok-member");
        assert_eq!(ok(&create)["replayed"], true);
        assert_eq!(ok(&["verify"])["entries"], bound_events + 8);
    })
    .await
    .unwrap();
}
