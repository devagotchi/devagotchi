//! `broker_capabilities`: what a live broker says it is, from its policy alone, without
//! admitting work, touching its journal or revealing a path, a grant or its account.

use super::lifecycle::account_broker;
use super::*;

fn ask() -> RequestBody {
    RequestBody::BrokerCapabilities(CapabilitiesInput {})
}

#[tokio::test]
async fn capabilities_describe_both_policies_without_work_or_private_configuration() {
    for account in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let executable = test_executable();
        let broker = if account {
            account_broker(&directory)
        } else {
            open_broker(&directory, &[&executable])
        };
        let revision = *broker.shared.roster.borrow();
        let changes = *broker.shared.changes.borrow();
        let ResponseBody::Capabilities(caps) = broker.handle(ask()).await.unwrap() else {
            panic!("a capabilities request answers with capabilities");
        };
        let refused_when_granted = ["config_set", "process_list", "process_kill", "url_read"];
        let supported: Vec<&str> = REQUEST_METHODS
            .iter()
            .copied()
            .filter(|method| account || !refused_when_granted.contains(method))
            .collect();
        // The exact allow-list: nothing else (a path, a grant, an account, the environment).
        assert_eq!(
            serde_json::to_value(&caps).unwrap(),
            serde_json::json!({
                "api_version": API_VERSION,
                "version": env!("CARGO_PKG_VERSION"),
                "os": std::env::consts::OS,
                "arch": std::env::consts::ARCH,
                "exec_policy": if account { "account" } else { "granted" },
                "shell_commands": account,
                "workspace_scope": if account { "account" } else { "workspace" },
                "supported_requests": supported,
            })
        );
        let encoded = serde_json::to_string(&caps).unwrap();
        assert!(!encoded.contains(&directory.path().display().to_string()));
        assert!(!encoded.contains(&executable.display().to_string()));
        // Nothing admitted, changed or written.
        let listed = broker
            .list(ListInput {
                offset: 0,
                limit: 32,
                scope: None,
                caller: None,
            })
            .unwrap();
        assert!(listed.operations.is_empty());
        assert_eq!(*broker.shared.roster.borrow(), revision);
        assert_eq!(*broker.shared.changes.borrow(), changes);
        {
            let state = broker.shared.lock();
            assert!(state.operations.is_empty());
            assert!(state.keys.is_empty());
            assert_eq!(state.active_count, 0);
            assert!(state.journal.load().unwrap().is_empty());
        }
        // What a granted broker leaves out, it does refuse.
        if !account {
            let config = RequestBody::ConfigSet(ConfigSetInput {
                key: "max_concurrent".into(),
                value: 2,
            });
            let processes = RequestBody::ProcessList(ProcessListInput {
                filter: None,
                max_results: 10,
            });
            let kill = RequestBody::ProcessKill(ProcessKillInput {
                pid: std::process::id(),
                force: false,
            });
            let url = RequestBody::UrlRead(UrlReadInput {
                url: "http://127.0.0.1:9/".into(),
                offset: 0,
                max_bytes: None,
            });
            for request in [config, processes, kill, url] {
                let error = broker.handle(request).await.unwrap_err();
                assert!(
                    ["not_permitted", "setting_not_changeable"].contains(&error.code.as_str()),
                    "{error:?}"
                );
            }
        }
        broker.shutdown().await;
    }
}

#[tokio::test]
async fn capabilities_refuse_a_closing_or_faulted_broker() {
    let directory = tempfile::tempdir().unwrap();
    let broker = account_broker(&directory);
    broker.shutdown().await;
    assert_eq!(
        broker.handle(ask()).await.unwrap_err().code,
        "shutting_down"
    );
    broker
        .shared
        .lock()
        .fault(BridgeError::new("persistence_error", "injected failure"));
    let error = broker.handle(ask()).await.unwrap_err();
    assert_eq!(error.code, "persistence_error");
    assert_eq!(error.message, "injected failure");
}
