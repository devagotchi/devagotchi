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
        let detail = (state == "failed").then(|| "busy".to_string());
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
