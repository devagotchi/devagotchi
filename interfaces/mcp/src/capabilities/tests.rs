use super::*;
use devagotchi_contracts::{BrokerCapabilities, Revision};

fn capabilities() -> ResponseBody {
    ResponseBody::Capabilities(BrokerCapabilities {
        api_version: API_VERSION,
        version: "test".into(),
        os: "windows".into(),
        arch: "x86_64".into(),
        exec_policy: "granted".into(),
        shell_commands: false,
        workspace_scope: "workspace".into(),
        supported_requests: vec!["broker_capabilities".into()],
    })
}

#[tokio::test]
async fn returns_only_the_brokers_answer_in_this_api_version() {
    assert!(matches!(
        response(async { Ok(capabilities()) }).await.unwrap(),
        ResponseBody::Capabilities(_)
    ));
    let mut body = capabilities();
    if let ResponseBody::Capabilities(ref mut info) = body {
        info.api_version += 1;
    }
    assert_eq!(
        response(async { Ok(body) }).await.unwrap_err().code,
        "api_version"
    );
}

#[tokio::test]
async fn passes_transport_and_broker_errors_through_unchanged() {
    for code in [
        "invalid_request",
        "ipc_io",
        "broker_offline",
        "device_revoked",
        "shutting_down",
    ] {
        let error = response(async { Err(BridgeError::new(code, "original detail")) })
            .await
            .unwrap_err();
        assert_eq!(error.code, code);
        assert_eq!(error.message, "original detail");
    }
}

#[tokio::test]
async fn refuses_another_kind_of_answer_instead_of_claiming_readiness() {
    let body = ResponseBody::Revision(Revision { revision: 0 });
    assert_eq!(
        response(async { Ok(body) }).await.unwrap_err().code,
        "unexpected_broker_response"
    );
}

#[tokio::test]
async fn a_broker_that_never_answers_is_unknown_after_three_seconds() {
    let start = tokio::time::Instant::now();
    let error = response(std::future::pending()).await.unwrap_err();
    assert_eq!(error.code, "inspection_timeout");
    assert!(start.elapsed() >= TIMEOUT);
    assert!(
        start.elapsed() < Duration::from_secs(6),
        "{:?}",
        start.elapsed()
    );
}
