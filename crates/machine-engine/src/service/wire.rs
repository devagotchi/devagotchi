//! Authenticate the local OS peer before choosing the local or granted envelope.
use crate::broker::Broker;
use devagotchi_contracts::*;
use devagotchi_ipc as ipc;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(untagged)]
enum Request {
    Local(WireRequest),
    Remote(RemoteWireRequest),
}

pub(super) async fn exchange(broker: &Broker, stream: &mut ipc::Connection) -> BridgeResult<()> {
    let result = async {
        ipc::verify_peer(stream)?;
        let bytes = ipc::read_frame(stream, MAX_FRAME_BYTES).await?;
        dispatch(broker, &bytes).await
    }
    .await;
    let bytes = match result {
        Ok(bytes) => bytes,
        // Both response families deliberately use the same closed error shape.
        Err(error) => serde_json::to_vec(&WireResponse::Error(error)).map_err(serialization)?,
    };
    ipc::write_frame(stream, &bytes, ipc::MAX_RESPONSE_BYTES).await
}

async fn dispatch(broker: &Broker, bytes: &[u8]) -> BridgeResult<Vec<u8>> {
    let request: Request = serde_json::from_slice(bytes)
        .map_err(|e| BridgeError::new("invalid_request", e.to_string()))?;
    match request {
        Request::Local(request) => {
            if request.api_version != API_VERSION {
                return Err(BridgeError::new(
                    "api_version",
                    "unsupported broker API version",
                ));
            }
            let response = match broker.handle(request.request).await {
                Ok(body) => WireResponse::Ok(body),
                Err(error) => WireResponse::Error(error),
            };
            serde_json::to_vec(&response).map_err(serialization)
        }
        Request::Remote(request) => {
            let response = match broker.handle_remote(request).await {
                Ok(body) => RemoteWireResponse::Ok(body),
                Err(error) => RemoteWireResponse::Error(error),
            };
            serde_json::to_vec(&response).map_err(serialization)
        }
    }
}
fn serialization(error: serde_json::Error) -> BridgeError {
    BridgeError::new("serialize", error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::broker::{BrokerConfig, ExecPolicy};
    #[tokio::test]
    async fn remote_ipc_preserves_envelope_and_uses_private_native_peer_checks() {
        let dir = tempfile::tempdir().unwrap();
        // Let StateGuard create the leaf with native private permissions, as startup does.
        let state = dir.path().canonicalize().unwrap().join("state");
        let mut guard = ipc::StateGuard::acquire(&state).unwrap();
        let mut listener = guard.bind().await.unwrap();
        let broker = Broker::open(
            BrokerConfig {
                workspace: dir.path().display().to_string(),
                allowed_executables: vec![],
                output_limit_bytes: 8192,
                max_operations: 8,
                max_concurrent: 1,
                exec_policy: ExecPolicy::Granted,
            },
            &state,
        )
        .unwrap();
        let serving = broker.clone();
        let server = tokio::spawn(async move {
            for _ in 0..3 {
                let mut connection = listener.accept().await.unwrap();
                exchange(&serving, &mut connection).await.unwrap();
            }
        });
        let reply = ipc::remote_request(
            guard.endpoint(),
            RemoteWireRequest {
                api_version: API_VERSION,
                remote: RemoteRequest::Capabilities {},
            },
        )
        .await
        .unwrap();
        assert!(
            matches!(reply,RemoteReply::Capabilities {version:REMOTE_ACCESS_VERSION,sandbox_levels,..} if sandbox_levels==vec![GrantLevel::Unsandboxed])
        );
        assert_eq!(
            ipc::remote_request(
                guard.endpoint(),
                RemoteWireRequest {
                    api_version: 99,
                    remote: RemoteRequest::Capabilities {}
                }
            )
            .await
            .unwrap_err()
            .code,
            "api_version"
        );
        let notice = GrantInterruption {
            origin: GrantOrigin {
                hub: "a".repeat(64),
                machine: "machine".into(),
                generation: "generation".into(),
            },
            grant_id: "grant".into(),
            minimum_version: 2,
        };
        assert!(matches!(
            ipc::remote_request(
                guard.endpoint(),
                RemoteWireRequest {
                    api_version: API_VERSION,
                    remote: RemoteRequest::Interrupt { notice }
                }
            )
            .await
            .unwrap(),
            RemoteReply::Interrupted { count: 0 }
        ));
        server.await.unwrap();
        broker.shutdown().await;
    }

    #[test]
    fn remote_ipc_cannot_fall_back_to_a_local_request() {
        let bytes=br#"{"api_version":1,"remote":{"action":"capabilities"},"request":{"method":"operation_get","params":{"operation_id":"id"}}}"#;
        assert!(serde_json::from_slice::<Request>(bytes).is_err());
        assert!(
            serde_json::from_slice::<Request>(
                br#"{"api_version":1,"remote":{"action":"capabilities","grant_id":"operator"}}"#
            )
            .is_err()
        );
    }
}
