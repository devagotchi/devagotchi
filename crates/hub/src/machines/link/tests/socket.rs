//! The session loop over a real socket: the whole router on a port (the `hosted_with`
//! harness), and this test playing the machine with the WebSocket client the link uses. What
//! the unit tests cannot reach: a result routed back to its call, a hello mid-session ending
//! the link as `protocol_error`, and a revoke closing a live link.

use std::time::Duration;

use devagotchi_contracts::{
    HubFrame, LinkFrame, ListInput, RequestBody, ResponseBody, Revision, WireResponse,
    link_signing_message,
};
use ed25519_dalek::{Signer, SigningKey};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, tungstenite::Message};

use super::super::serve::REVOCATION_CHECK;
use crate::{ops::audit::Recorded, tenancy::tests::tenants::hosted_with};

type Device = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

/// The link broker the harness's config names.
const MACHINE: &str = "wi1";
/// Longest any step may take before the test fails instead of hanging.
const STEP: Duration = Duration::from_secs(10);

async fn next(device: &mut Device) -> Option<HubFrame> {
    loop {
        match tokio::time::timeout(STEP, device.next()).await.unwrap() {
            Some(Ok(Message::Text(text))) => return Some(serde_json::from_str(&text).unwrap()),
            Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return None,
            Some(Ok(_)) => continue,
        }
    }
}

async fn say(device: &mut Device, frame: &LinkFrame) {
    let text = serde_json::to_string(frame).unwrap();
    device.send(Message::text(text)).await.unwrap();
}

/// Dial `/link`, prove `key` over the challenge (with `code` the first time) and be welcomed.
async fn join(url: &str, key: &SigningKey, code: Option<&str>) -> Device {
    let socket = format!("{}/link", url.replacen("http://", "ws://", 1));
    let (mut device, _) = tokio_tungstenite::connect_async(socket).await.unwrap();
    let Some(HubFrame::Challenge { nonce, hub }) = next(&mut device).await else {
        panic!("the hub opens with a challenge");
    };
    assert_eq!(hub, url);
    let hello = LinkFrame::Hello {
        version: devagotchi_contracts::LINK_PROTOCOL_VERSION,
        device: MACHINE.into(),
        public_key: hex::encode(key.verifying_key().to_bytes()),
        signature: hex::encode(
            key.sign(&link_signing_message(url, MACHINE, &nonce))
                .to_bytes(),
        ),
        enroll_code: code.map(str::to_string),
        build: Some("main-aaaaaaa".into()),
        kind: None,
    };
    say(&mut device, &hello).await;
    match next(&mut device).await {
        Some(HubFrame::Welcome { device: name }) => assert_eq!(name, MACHINE),
        other => panic!("expected a welcome, got {other:?}"),
    }
    device
}

/// The audit row `tool` left for this machine with `outcome`, waited for (the loop writes
/// it on its way out).
async fn row(audit: &crate::ops::audit::Audit, tool: &str, outcome: &str) -> Recorded {
    let deadline = tokio::time::Instant::now() + STEP;
    loop {
        let rows = audit.after(0, 1000).await.unwrap();
        if let Some(found) = rows
            .into_iter()
            .rev()
            .find(|r| r.tool == tool && r.broker == MACHINE && r.outcome == outcome)
        {
            return found;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "no {tool} row for {MACHINE} with {outcome}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn a_live_link_answers_calls_and_a_hello_mid_session_ends_it_as_protocol_error() {
    let hosted = or_skip!(hosted_with(None, |_, _| serde_json::json!({})));
    let code = hosted.devices.issue_code(MACHINE, [1; 16]).await.unwrap();
    let key = SigningKey::from_bytes(&[3; 32]);
    let mut device = join(&hosted.url, &key, Some(&code)).await;
    row(&hosted.hub.audit, "link.connect", "ok").await;
    assert!(hosted.hub.links.connected(MACHINE));
    assert_eq!(
        hosted.hub.links.build(MACHINE).as_deref(),
        Some("main-aaaaaaa")
    );

    // A call goes out as a `call` frame; the `result` with its id answers it.
    let links = hosted.hub.links.clone();
    let asked = tokio::spawn(async move {
        let list = RequestBody::OperationList(ListInput {
            offset: 0,
            limit: 1,
            caller: None,
            scope: None,
        });
        links.call(MACHINE, list).await
    });
    let Some(HubFrame::Call { id, request }) = next(&mut device).await else {
        panic!("the call reaches the machine");
    };
    assert!(matches!(request.request, RequestBody::OperationList(_)));
    let answer = WireResponse::Ok(ResponseBody::Revision(Revision { revision: 7 }));
    say(&mut device, &LinkFrame::Heartbeat).await;
    say(
        &mut device,
        &LinkFrame::Result {
            id,
            response: answer,
        },
    )
    .await;
    let answered = tokio::time::timeout(STEP, asked).await.unwrap().unwrap();
    assert!(matches!(
        answered,
        Ok(ResponseBody::Revision(Revision { revision: 7 }))
    ));

    // A second hello on an open link is not something a link sends: the hub ends it.
    let again = LinkFrame::Hello {
        version: devagotchi_contracts::LINK_PROTOCOL_VERSION,
        device: MACHINE.into(),
        public_key: "ab".into(),
        signature: "cd".into(),
        enroll_code: None,
        build: None,
        kind: None,
    };
    say(&mut device, &again).await;
    assert!(next(&mut device).await.is_none(), "the hub closed the link");
    row(&hosted.hub.audit, "link.disconnect", "protocol_error").await;
    assert!(!hosted.hub.links.connected(MACHINE));
}

#[tokio::test]
async fn revoking_a_machine_closes_its_live_link_and_says_why() {
    let hosted = or_skip!(hosted_with(None, |_, _| serde_json::json!({})));
    let code = hosted.devices.issue_code(MACHINE, [2; 16]).await.unwrap();
    let key = SigningKey::from_bytes(&[4; 32]);
    // Enrolled once, then connected as a link that already holds its key.
    drop(join(&hosted.url, &key, Some(&code)).await);
    let mut device = join(&hosted.url, &key, None).await;
    let revoked_at = tokio::time::Instant::now();
    hosted.devices.revoke(MACHINE).await.unwrap();
    match next(&mut device).await {
        Some(HubFrame::Refused { code, .. }) => assert_eq!(code, "device_revoked"),
        other => panic!("expected device_revoked, got {other:?}"),
    }
    assert!(next(&mut device).await.is_none(), "and the link is closed");
    assert!(
        revoked_at.elapsed() <= REVOCATION_CHECK * 2 + Duration::from_secs(1),
        "closed within two revocation checks: {:?}",
        revoked_at.elapsed()
    );
    row(&hosted.hub.audit, "link.disconnect", "revoked").await;
}
