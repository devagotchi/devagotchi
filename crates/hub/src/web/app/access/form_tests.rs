use super::*;
use crate::machines::access::GrantTerms;
use serde_json::json;

#[test]
fn access_browser_narrowing_keeps_other_selectors_and_empty_paths_are_deny_all() {
    let terms: GrantTerms=serde_json::from_value(json!({"item":{"capability":"exec","sandbox_level":"read-only",
        "selector":{"paths":["/work"],"argv_prefix":[["git","status"]],"env_allow":["LANG"],"hosts":["example.test"]}},
        "duration":"1h","uses_max":3})).unwrap();
    let (draft, error) = Draft::submitted(
        [
            ("action", "narrow"),
            ("level", "read-only"),
            ("paths_mode", "restrict"),
            ("paths", "\n  \n"),
            ("duration", "once"),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .collect(),
    );
    assert!(error.is_none());
    let Decision::Approve { narrow } = draft.decision(&terms).unwrap() else {
        panic!("expected approval")
    };
    let reduced = terms.narrowed(&narrow).unwrap();
    assert_eq!(reduced.item.selector.paths, Some(vec![]));
    assert_eq!(
        reduced.item.selector.argv_prefix,
        terms.item.selector.argv_prefix
    );
    assert_eq!(
        reduced.item.selector.env_allow,
        terms.item.selector.env_allow
    );
    assert_eq!(reduced.item.selector.hosts, terms.item.selector.hosts);
    assert_eq!(reduced.uses_max, Some(1));
}

#[test]
fn access_browser_invalid_fields_preserve_the_first_draft_and_retry_identity() {
    for extra in [("request_id", "replacement"), ("principal", "forged")] {
        let (draft, error) = Draft::submitted(
            [
                ("request_id", "original"),
                ("version", "7"),
                ("paths", "/work/a"),
                ("note", "My scope"),
                extra,
            ]
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect(),
        );
        assert!(error.is_some());
        assert_eq!(draft.request_id, "original");
        assert_eq!(draft.version().unwrap(), 7);
        assert_eq!(draft.paths, "/work/a");
        assert_eq!(draft.note, "My scope");
    }
}
