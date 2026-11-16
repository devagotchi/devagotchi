use super::*;

#[test]
fn a_machine_older_than_its_hub_is_told_to_update_when_a_call_fails() {
    let offline = || {
        BridgeError::new(
            "broker_offline",
            "box is enrolled but its link is not connected",
        )
    };
    let hinted = outdated(Some("main-aaaaaaa"), Some("main-bbbbbbb"), offline());
    assert_eq!(
        hinted.code, "broker_offline",
        "the code stays what happened"
    );
    assert!(
        hinted.message.contains("runs main-aaaaaaa"),
        "{}",
        hinted.message
    );
    assert!(
        hinted.message.contains("machine_update"),
        "{}",
        hinted.message
    );
    // The same build, an unknown one, or a refusal that is not about skew: nothing added.
    for (build, offered) in [
        (Some("main-b"), Some("main-b")),
        (None, Some("main-b")),
        (Some("main-a"), None),
    ] {
        assert_eq!(
            outdated(build, offered, offline()).message,
            offline().message
        );
    }
    let refused = BridgeError::new("not_found", "No operation has this ID here.");
    assert_eq!(
        outdated(Some("main-a"), Some("main-b"), refused).message,
        "No operation has this ID here."
    );
}
