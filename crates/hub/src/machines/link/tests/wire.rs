//! What a refused peer is told (little), and how many may knock at once.

use super::serve::wire_refusal;
use super::*;
use devagotchi_contracts::REFUSAL_UNAVAILABLE;

/// Every refusal `admit` and `may_enroll` can give, and the code that reaches the wire: only
/// `protocol_version` passes as is, a store or audit hiccup asks for a retry, and the rest
/// say only `refused`.
#[test]
fn the_wire_says_only_refused_unless_retrying_can_help() {
    for (code, wire) in [
        ("protocol_version", "protocol_version"),
        ("device_store", REFUSAL_UNAVAILABLE),
        ("audit_write", REFUSAL_UNAVAILABLE),
        ("protocol_error", "refused"),
        ("unknown_broker", "refused"),
        ("not_enrolled", "refused"),
        ("key_mismatch", "refused"),
        ("device_revoked", "refused"),
        ("enroll_refused", "refused"),
        ("early_access", "refused"),
        ("bad_key", "refused"),
        ("bad_signature", "refused"),
    ] {
        let HubFrame::Refused {
            code: said,
            message,
        } = wire_refusal(&BridgeError::new(code, "detail only the audit sees"))
        else {
            panic!("expected a refusal");
        };
        assert_eq!(said, wire, "{code}");
        if code != "protocol_version" {
            assert!(!message.contains("detail"), "{code} leaked: {message}");
        }
    }
}

#[tokio::test]
async fn handshakes_are_bounded() {
    let links = or_skip!(test_links());
    let held: Vec<_> = (0..MAX_HANDSHAKES)
        .map(|_| links.handshake_permit().unwrap())
        .collect();
    assert!(links.handshake_permit().is_none());
    drop(held);
    assert!(links.handshake_permit().is_some());
}
