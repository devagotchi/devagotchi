//! The hello: what binds a key, and every way one is refused, by its exact code.

use super::*;
use crate::machines::devices::OWNED_LIMIT;

#[tokio::test]
async fn enrollment_then_signed_reconnects_are_admitted() {
    let links = or_skip!(test_links());
    let key = SigningKey::from_bytes(&[9; 32]);
    let code = links.devices.issue_code("gus", [4; 16]).await.unwrap();
    let admitted = links
        .admit(hello(&key, HUB, "gus", "n1", Some(&code)), "n1")
        .await
        .unwrap();
    assert_eq!(
        (admitted.device.as_str(), admitted.joined_for),
        ("gus", None)
    );
    assert!(
        links
            .admit(hello(&key, HUB, "gus", "n2", None), "n2")
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn replays_other_hubs_other_keys_and_revoked_devices_are_refused() {
    let links = or_skip!(test_links());
    let key = SigningKey::from_bytes(&[9; 32]);
    let other = SigningKey::from_bytes(&[8; 32]);
    let code = links.devices.issue_code("gus", [5; 16]).await.unwrap();
    links
        .admit(hello(&key, HUB, "gus", "n1", Some(&code)), "n1")
        .await
        .unwrap();
    let refused = async |frame, nonce: &str| links.admit(frame, nonce).await.unwrap_err().code;
    // A signature over an old challenge does not open a new connection.
    assert_eq!(
        refused(hello(&key, HUB, "gus", "n1", None), "n2").await,
        "bad_signature"
    );
    // Signed for another hub.
    assert_eq!(
        refused(hello(&key, "https://evil.example", "gus", "n3", None), "n3").await,
        "bad_signature"
    );
    // A different key claiming the enrolled name.
    assert_eq!(
        refused(hello(&other, HUB, "gus", "n4", None), "n4").await,
        "key_mismatch"
    );
    // A name nobody configured or added.
    assert_eq!(
        refused(hello(&key, HUB, "dabo", "n5", None), "n5").await,
        "not_enrolled"
    );
    links.devices.revoke("gus").await.unwrap();
    assert_eq!(
        refused(hello(&key, HUB, "gus", "n6", None), "n6").await,
        "device_revoked"
    );
}

/// A link speaking another protocol is told so by name (`protocol_version`, which reaches
/// the wire as is), before its key or code is even looked at.
#[tokio::test]
async fn a_hello_in_another_protocol_is_refused_as_protocol_version() {
    let links = or_skip!(test_links());
    let key = SigningKey::from_bytes(&[9; 32]);
    let code = links.devices.issue_code("gus", [8; 16]).await.unwrap();
    let mut frame = hello(&key, HUB, "gus", "n", Some(&code));
    if let Some(LinkFrame::Hello { version, .. }) = &mut frame {
        *version = LINK_PROTOCOL_VERSION + 1;
    }
    let refused = links.admit(frame, "n").await.unwrap_err();
    assert_eq!(refused.code, "protocol_version");
    assert!(
        links.devices.get("gus").await.unwrap().is_none(),
        "the code was not spent"
    );
}

#[tokio::test]
async fn a_machine_an_account_added_joins_for_that_account_without_config() {
    let links = or_skip!(test_links());
    let key = SigningKey::from_bytes(&[11; 32]);
    // An operator's code cannot add a name the config does not have.
    let operator = links.devices.issue_code("unnamed", [1; 16]).await.unwrap();
    assert_eq!(
        links
            .admit(hello(&key, HUB, "unnamed", "n1", Some(&operator)), "n1")
            .await
            .unwrap_err()
            .code,
        "enroll_refused"
    );
    let code = links
        .devices
        .issue_owned_code("acct-a", "lab", [2; 16], OWNED_LIMIT)
        .await
        .unwrap();
    let joined = links
        .admit(hello(&key, HUB, "lab", "n2", Some(&code)), "n2")
        .await
        .unwrap();
    assert_eq!(joined.joined_for.as_deref(), Some("acct-a"));
    // Reconnecting later is an ordinary signed hello, and nobody is told it joined again.
    let again = links
        .admit(hello(&key, HUB, "lab", "n3", None), "n3")
        .await
        .unwrap();
    assert_eq!((again.device.as_str(), again.joined_for), ("lab", None));
    assert_eq!(links.owned_by("acct-a").await.unwrap().len(), 1);
    // A name that could never be a broker stops at the door.
    assert_eq!(
        links
            .admit(hello(&key, HUB, "Bad Name", "n4", None), "n4")
            .await
            .unwrap_err()
            .code,
        "unknown_broker"
    );
}

#[tokio::test]
async fn weak_keys_are_refused_even_with_a_valid_code() {
    let links = or_skip!(test_links());
    let code = links.devices.issue_code("gus", [7; 16]).await.unwrap();
    // The identity point: a small-order key whose "signatures" anyone can forge.
    let mut identity = [0u8; 32];
    identity[0] = 1;
    let frame = Some(LinkFrame::Hello {
        version: LINK_PROTOCOL_VERSION,
        device: "gus".into(),
        public_key: hex::encode(identity),
        signature: hex::encode([0u8; 64]),
        enroll_code: Some(code),
        build: None,
        kind: None,
    });
    assert_eq!(links.admit(frame, "n").await.unwrap_err().code, "bad_key");
    assert!(links.devices.get("gus").await.unwrap().is_none());
}
