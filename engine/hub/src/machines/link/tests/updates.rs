//! The update gate (`updates.rs` `ask_update`): the hub sends `update` only to a machine
//! whose hello said its build, that is not on the hub's build already, from a hub that hands
//! one out. Each refusal is asserted by its code, with nothing sent.

use super::*;

#[tokio::test]
async fn a_link_too_old_to_say_its_build_is_never_sent_an_update() {
    let links = or_skip!(test_links()).offering(Some("main-bbbbbbb".into()));
    let (_, (_, mut frames)) = connect(&links, "gus", 1, None, false);
    let refused = links
        .request_update("gus", true, "jesse", "hub")
        .await
        .unwrap_err();
    assert_eq!(refused.code, "update_unsupported");
    assert!(
        refused.message.contains("install it once more by hand"),
        "{}",
        refused.message
    );
    assert!(frames.try_recv().is_err(), "nothing was sent");
    assert!(links.update_of("gus").is_none());
}

#[tokio::test]
async fn a_machine_on_the_hubs_build_is_already_on_it() {
    let links = or_skip!(test_links()).offering(Some("main-bbbbbbb".into()));
    let (_, (_, mut frames)) = connect(&links, "gus", 1, Some("main-bbbbbbb"), false);
    let asked = links
        .request_update("gus", false, "jesse", "hub")
        .await
        .unwrap();
    assert_eq!(asked, UpdateAsked::AlreadyOn("main-bbbbbbb".into()));
    assert!(frames.try_recv().is_err(), "nothing was sent");
    assert!(links.update_of("gus").is_none());
}

#[tokio::test]
async fn a_hub_with_no_build_to_hand_out_asks_nothing() {
    let links = or_skip!(test_links());
    let (_, (_, mut frames)) = connect(&links, "gus", 1, Some("main-aaaaaaa"), false);
    let refused = links
        .request_update("gus", false, "jesse", "hub")
        .await
        .unwrap_err();
    assert_eq!(refused.code, "no_build");
    assert!(frames.try_recv().is_err(), "nothing was sent");
}

#[tokio::test]
async fn a_machine_that_said_its_build_gets_the_update_and_its_reports_are_recorded() {
    let links = or_skip!(test_links()).offering(Some("main-bbbbbbb".into()));
    let (_, (_, mut frames)) = connect(&links, "gus", 1, Some("main-aaaaaaa"), false);
    let asked = links
        .request_update("gus", true, "jesse", "hub")
        .await
        .unwrap();
    assert_eq!(asked, UpdateAsked::Sent);
    assert!(matches!(
        frames.try_recv(),
        Ok(HubFrame::Update { ref version, force: true }) if version == "main-bbbbbbb"
    ));
    assert_eq!(links.update_of("gus").unwrap().state, update::REQUESTED);
    // A state the contracts do not name is not taken.
    links.update_reported("gus", "exploding", None).await;
    assert_eq!(links.update_of("gus").unwrap().state, update::REQUESTED);
    links
        .update_reported("gus", UPDATE_FAILED, Some("no space".into()))
        .await;
    let failed = links.update_of("gus").unwrap();
    assert_eq!(
        (failed.state.as_str(), failed.detail.as_deref()),
        (UPDATE_FAILED, Some("no space"))
    );
}
