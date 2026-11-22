//! A real WebSocket and native private IPC connection preserve the complete envelope.
use super::*;
use devagotchi_contracts::*;
use devagotchi_ipc as ipc;

#[tokio::test]
async fn remote_link_forwards_exact_authority_to_private_ipc_and_returns_broker_refusal() {
    let (listener, mut config) = fake_hub().await;
    let state = tempfile::tempdir().unwrap();
    let mut guard =
        ipc::StateGuard::acquire(&state.path().canonicalize().unwrap().join("broker")).unwrap();
    let mut local = guard.bind().await.unwrap();
    config.broker = guard.endpoint().display().to_string();
    let updater = Updater::new(&config, state.path(), "main-abc1234", false);
    let origin = super::super::remote::origin(&config, Some("enrolled".into())).unwrap();
    let wire = RemoteWireRequest {
        api_version: API_VERSION,
        remote: RemoteRequest::Interrupt {
            notice: GrantInterruption {
                origin,
                grant_id: "grant-123".into(),
                minimum_version: 7,
            },
        },
    };
    let expected = serde_json::to_value(&wire).unwrap();
    let broker = async {
        let mut stream = local.accept().await.unwrap();
        ipc::verify_peer(&stream).unwrap();
        let bytes = ipc::read_frame(&mut stream, MAX_FRAME_BYTES).await.unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
            expected
        );
        let response =
            RemoteWireResponse::Error(BridgeError::new("authority_revoked", "interrupted"));
        ipc::write_frame(
            &mut stream,
            &serde_json::to_vec(&response).unwrap(),
            ipc::MAX_RESPONSE_BYTES,
        )
        .await
        .unwrap();
    };
    let hub = async {
        let mut hub = accept(&listener).await;
        to_link(
            &mut hub,
            HubFrame::Challenge {
                nonce: "nonce".into(),
                hub: config.hub.clone(),
            },
        )
        .await;
        assert!(matches!(
            next_link_frame(&mut hub).await,
            LinkFrame::Hello {
                remote_access: Some(REMOTE_ACCESS_VERSION),
                ..
            }
        ));
        to_link(
            &mut hub,
            HubFrame::Welcome {
                device: config.device.clone(),
                generation: Some("enrolled".into()),
            },
        )
        .await;
        to_link(
            &mut hub,
            HubFrame::RemoteCall {
                id: 71,
                request: wire,
            },
        )
        .await;
        loop {
            match next_link_frame(&mut hub).await {
                LinkFrame::Heartbeat => continue,
                LinkFrame::RemoteResult {
                    id: 71,
                    response: RemoteWireResponse::Error(error),
                } => {
                    assert_eq!(error.code, "authority_revoked");
                    break;
                }
                other => panic!("unexpected response {other:?}"),
            }
        }
    };
    let key = key();
    let link = connect_with(
        &config,
        &key,
        Mode::Serve {
            updater: &updater,
            state_dir: state.path(),
        },
        PATIENT,
    );
    let (ended, (), ()) = tokio::time::timeout(BOUND, async { tokio::join!(link, hub, broker) })
        .await
        .unwrap();
    assert!(matches!(ended, Ended::Retry(_)));
}
