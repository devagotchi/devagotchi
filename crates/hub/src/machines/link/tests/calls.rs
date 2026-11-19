//! Routing a call to a machine: why one cannot be, a full queue, and an abandoned wait.

use devagotchi_contracts::{
    API_VERSION, CapabilitiesInput, ListInput, RequestBody, ResponseBody, Revision,
};

use super::*;

fn list() -> RequestBody {
    RequestBody::OperationList(ListInput {
        offset: 0,
        limit: 1,
        caller: None,
        scope: None,
    })
}

#[tokio::test]
async fn calls_to_a_device_without_a_link_say_why() {
    let links = or_skip!(test_links());
    assert_eq!(
        links.call("gus", list()).await.unwrap_err().code,
        "not_enrolled"
    );
    let key = SigningKey::from_bytes(&[9; 32]);
    let code = links.devices.issue_code("gus", [6; 16]).await.unwrap();
    links
        .admit(hello(&key, HUB, "gus", "n", Some(&code)), "n")
        .await
        .unwrap();
    assert_eq!(
        links.call("gus", list()).await.unwrap_err().code,
        "broker_offline"
    );
    links.devices.revoke("gus").await.unwrap();
    assert_eq!(
        links.call("gus", list()).await.unwrap_err().code,
        "device_revoked"
    );
}

#[tokio::test]
async fn a_full_queue_answers_busy_and_leaves_no_waiters() {
    let links = or_skip!(test_links());
    let key = SigningKey::from_bytes(&[9; 32]);
    let code = links.devices.issue_code("gus", [3; 16]).await.unwrap();
    links
        .admit(hello(&key, HUB, "gus", "n", Some(&code)), "n")
        .await
        .unwrap();
    // A session whose loop never drains: the device stopped reading.
    let (pending, _receivers) = connect(&links, "gus", 1, None, false);
    let calls = links.sessions.lock().unwrap()["gus"].calls.clone();
    for i in 0..CALL_QUEUE as u64 {
        let request = WireRequest {
            api_version: API_VERSION,
            request: list(),
        };
        calls.try_send((u64::MAX - i, request)).unwrap();
    }
    assert_eq!(
        links.call("gus", list()).await.unwrap_err().code,
        "link_busy"
    );
    assert!(
        pending.lock().unwrap().is_empty(),
        "a refused call must not leave a waiter"
    );
}

/// An inspection gives up after its own 3 seconds, long before the link's call timeout: its
/// waiter goes, the link stays connected, and the next call on it is answered.
#[tokio::test]
async fn an_unanswered_inspection_leaves_no_waiter_and_keeps_the_link() {
    let links = or_skip!(test_links());
    let key = SigningKey::from_bytes(&[9; 32]);
    let code = links.devices.issue_code("gus", [5; 16]).await.unwrap();
    links
        .admit(hello(&key, HUB, "gus", "n", Some(&code)), "n")
        .await
        .unwrap();
    let (pending, (mut received, _frames)) = connect(&links, "gus", 1, None, false);
    let inspect = RequestBody::BrokerCapabilities(CapabilitiesInput {});
    let error = devagotchi_mcp::capabilities::response(links.call("gus", inspect))
        .await
        .unwrap_err();
    assert_eq!(error.code, "inspection_timeout");
    let (_, asked) = received.recv().await.unwrap();
    assert!(matches!(asked.request, RequestBody::BrokerCapabilities(_)));
    assert!(
        pending.lock().unwrap().is_empty(),
        "an abandoned inspection must not leave a waiter"
    );
    assert!(links.connected("gus"));
    let answering = pending.clone();
    let device = tokio::spawn(async move {
        let (id, _) = received.recv().await.unwrap();
        let waiter = answering.lock().unwrap().remove(&id).unwrap();
        let answer = ResponseBody::Revision(Revision { revision: 7 });
        waiter.send(WireResponse::Ok(answer)).unwrap();
    });
    let answer = links.call("gus", list()).await.unwrap();
    assert!(matches!(
        answer,
        ResponseBody::Revision(Revision { revision: 7 })
    ));
    device.await.unwrap();
}
