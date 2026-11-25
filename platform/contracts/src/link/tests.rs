use super::*;

#[test]
fn hello_carries_its_build_and_an_old_hello_without_one_still_parses() {
    let hello = LinkFrame::Hello {
        version: LINK_PROTOCOL_VERSION,
        device: "dabo".into(),
        public_key: "ab".into(),
        signature: "cd".into(),
        enroll_code: None,
        build: Some("main-abc1234".into()),
        kind: None,
        remote_access: None,
    };
    let text = serde_json::to_string(&hello).unwrap();
    assert!(text.contains(r#""build":"main-abc1234""#), "{text}");
    // An ordinary machine says nothing about its kind, so an older hub still reads it.
    assert!(!text.contains("kind"), "{text}");
    let back: LinkFrame = serde_json::from_str(&text).unwrap();
    assert!(matches!(back, LinkFrame::Hello { build: Some(ref b), .. } if b == "main-abc1234"));
    let docked = text.replace(r#""build""#, r#""kind":"container","build""#);
    let back: LinkFrame = serde_json::from_str(&docked).unwrap();
    assert!(matches!(back, LinkFrame::Hello { kind: Some(ref k), .. } if k == "container"));
    // What a device from before builds had a name sends.
    let old = r#"{"type":"hello","version":1,"device":"dabo","public_key":"ab","signature":"cd"}"#;
    let parsed: LinkFrame = serde_json::from_str(old).unwrap();
    assert!(matches!(
        parsed,
        LinkFrame::Hello {
            build: None,
            enroll_code: None,
            kind: None,
            remote_access: None,
            ..
        }
    ));
}

#[test]
fn update_and_update_status_round_trip() {
    let update = HubFrame::Update {
        version: "main-abc1234".into(),
        force: true,
    };
    let text = serde_json::to_string(&update).unwrap();
    assert_eq!(
        text,
        r#"{"type":"update","version":"main-abc1234","force":true}"#
    );
    let back: HubFrame = serde_json::from_str(&text).unwrap();
    assert!(
        matches!(back, HubFrame::Update { ref version, force: true } if version == "main-abc1234")
    );
    for state in UPDATE_STATES {
        let detail = (state == UPDATE_FAILED).then(|| "busy".to_string());
        let status = LinkFrame::UpdateStatus {
            state: state.into(),
            detail: detail.clone(),
        };
        let text = serde_json::to_string(&status).unwrap();
        let back: LinkFrame = serde_json::from_str(&text).unwrap();
        assert!(
            matches!(back, LinkFrame::UpdateStatus { state: ref s, detail: ref d } if s == state && *d == detail),
            "{text}"
        );
    }
    let bare: LinkFrame =
        serde_json::from_str(r#"{"type":"update_status","state":"downloading"}"#).unwrap();
    assert!(matches!(bare, LinkFrame::UpdateStatus { detail: None, .. }));
}

/// Every frame shape a released peer sends, oldest first. APPEND ONLY: a peer that sends an
/// old line may still be out there (`.ai/patterns/link.md` rule 2). Never edit or delete a line.
const SEEN: &[&str] = &[
    r#"{"type":"challenge","nonce":"n","hub":"https://h"}"#,
    r#"{"type":"welcome","device":"d"}"#,
    r#"{"type":"refused","code":"refused","message":"m"}"#,
    r#"{"type":"refused","code":"unavailable","message":"m"}"#,
    r#"{"type":"call","id":1,"request":{"api_version":1,"request":{"method":"broker_capabilities","params":{}}}}"#,
    r#"{"type":"update","version":"main-abc1234","force":false}"#,
    // v1 links (before builds had names)
    r#"{"type":"hello","version":1,"device":"d","public_key":"ab","signature":"cd","enroll_code":null}"#,
    r#"{"type":"hello","version":1,"device":"d","public_key":"ab","signature":"cd","enroll_code":"dvg-x"}"#,
    // U2 links: enroll sends build null, serve sends a build
    r#"{"type":"hello","version":1,"device":"d","public_key":"ab","signature":"cd","enroll_code":"dvg-x","build":null}"#,
    r#"{"type":"hello","version":1,"device":"d","public_key":"ab","signature":"cd","enroll_code":null,"build":"main-abc1234"}"#,
    // increment 209: a container says its kind
    r#"{"type":"hello","version":1,"device":"d","public_key":"ab","signature":"cd","enroll_code":null,"build":"main-abc1234","kind":"container"}"#,
    // increment 271: an enroll hello leaves build out instead of sending null
    r#"{"type":"hello","version":1,"device":"d","public_key":"ab","signature":"cd","enroll_code":"dvg-x"}"#,
    r#"{"type":"result","id":1,"response":{"status":"ok","value":{"kind":"revision","value":{"revision":7}}}}"#,
    r#"{"type":"result","id":1,"response":{"status":"error","value":{"code":"link_busy","message":"m"}}}"#,
    r#"{"type":"heartbeat"}"#,
    r#"{"type":"update_status","state":"downloading","detail":null}"#,
    r#"{"type":"update_status","state":"failed","detail":"busy"}"#,
    r#"{"type":"remote_call","id":2,"request":{"api_version":1,"remote":{"action":"capabilities"}}}"#,
    r#"{"type":"remote_result","id":2,"response":{"status":"error","value":{"code":"authority_expired","message":"expired"}}}"#,
    r#"{"type":"welcome","device":"d","generation":"g-1"}"#,
    r#"{"type":"hello","version":1,"device":"d","public_key":"ab","signature":"cd","remote_access":1}"#,
];

// No wildcard arms: a new variant does not compile until it is named here, and the golden
// test then fails until SEEN carries a line for it.
fn hub_name(frame: &HubFrame) -> &'static str {
    match frame {
        HubFrame::Challenge { .. } => "challenge",
        HubFrame::Welcome { .. } => "welcome",
        HubFrame::Refused { .. } => "refused",
        HubFrame::Call { .. } => "call",
        HubFrame::Update { .. } => "update",
        HubFrame::RemoteCall { .. } => "remote_call",
    }
}

fn link_name(frame: &LinkFrame) -> &'static str {
    match frame {
        LinkFrame::Hello { .. } => "hello",
        LinkFrame::Result { .. } => "result",
        LinkFrame::Heartbeat => "heartbeat",
        LinkFrame::RemoteResult { .. } => "remote_result",
        LinkFrame::UpdateStatus { .. } => "update_status",
    }
}

const ALL: [&str; 11] = [
    "call",
    "challenge",
    "heartbeat",
    "hello",
    "refused",
    "result",
    "remote_call",
    "remote_result",
    "update",
    "update_status",
    "welcome",
];

#[test]
fn every_frame_a_released_peer_sends_still_parses_and_every_variant_has_a_line() {
    let mut seen = std::collections::BTreeSet::new();
    for line in SEEN {
        let name = serde_json::from_str::<HubFrame>(line)
            .map(|frame| hub_name(&frame))
            .or_else(|_| serde_json::from_str::<LinkFrame>(line).map(|frame| link_name(&frame)))
            .unwrap_or_else(|e| panic!("a released peer's frame no longer parses: {line}: {e}"));
        seen.insert(name);
    }
    assert_eq!(
        seen,
        ALL.into_iter().collect(),
        "add a SEEN line for every new variant"
    );
}

#[test]
fn the_signed_bytes_and_the_protocol_never_change() {
    assert_eq!(
        LINK_PROTOCOL_VERSION, 1,
        "bumping strands every machine: .ai/patterns/link.md rule 2"
    );
    assert_eq!(
        link_signing_message("https://h", "d", "n"),
        b"devagotchi-link/1\nhttps://h\nd\nn".to_vec()
    );
}

#[test]
fn fields_added_after_v1_stay_off_the_wire_when_none() {
    let hello = LinkFrame::Hello {
        version: LINK_PROTOCOL_VERSION,
        device: "d".into(),
        public_key: "ab".into(),
        signature: "cd".into(),
        enroll_code: Some("dvg-x".into()),
        build: None,
        kind: None,
        remote_access: None,
    };
    let text = serde_json::to_string(&hello).unwrap();
    assert!(
        !text.contains("\"build\"") && !text.contains("\"kind\""),
        "{text}"
    );
}

#[test]
fn the_update_states_are_the_constants_and_only_done_and_failed_end_one() {
    assert_eq!(
        UPDATE_STATES.iter().filter(|s| update_finished(s)).count(),
        2
    );
    assert!(update_finished(UPDATE_DONE) && update_finished(UPDATE_FAILED));
    assert!(!update_finished("requested"));
}

#[test]
fn build_names_are_plain() {
    for good in ["main-abc1234", "ci-abc1234", "0.4.2", "dev", "1.0.0-rc.1"] {
        assert!(valid_build_name(good), "{good}");
    }
    let long = "a".repeat(65);
    for bad in ["", "-x", ".x", "a b", "a/b", "a;b", "é", long.as_str()] {
        assert!(!valid_build_name(bad), "{bad:?}");
    }
}
