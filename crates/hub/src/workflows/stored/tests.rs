use super::*;

#[test]
fn ids_tokens_and_cookies_have_their_shapes() {
    let id = new_workflow_id([0xab; 12]);
    assert!(valid_workflow_id(&id), "{id}");
    for bad in [
        "wf_",
        "wf_ABABABABABABABABABABABAB",
        "acct_x",
        "wf_abababababababababababab0",
    ] {
        assert!(!valid_workflow_id(bad), "{bad}");
    }
    let token = new_link_token([7; 32]);
    assert!(valid_link_token(&token), "{token}");
    assert!(!valid_link_token("dgw_short"));
    assert!(!valid_link_token(&token.replace("dgw_", "dgs_")));
    assert_eq!(link_hash(&token).len(), 64);
    let secret = new_browser_secret([9; 32]);
    assert_eq!(browser_hash(&secret).map(|key| key.len()), Some(64));
    for bad in [
        "dgb_short",
        "dgs_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        "",
    ] {
        assert_eq!(browser_hash(bad), None, "{bad}");
    }
}

#[test]
fn browsers_are_remembered_once_newest_last_and_only_for_a_while() {
    let mut progress = Progress::default();
    assert!(!progress.any_done() && !progress.did_a_step("a", 60, 0));
    progress.mark_browser("a", 10);
    progress.mark_browser("b", 20);
    progress.mark_browser("a", 30);
    assert_eq!(
        progress
            .browsers
            .iter()
            .map(|m| (m.hash.as_str(), m.at))
            .collect::<Vec<_>>(),
        [("b", 20), ("a", 30)]
    );
    assert!(progress.did_a_step("a", 60, 89) && !progress.did_a_step("a", 60, 90));
    assert!(!progress.did_a_step("c", 60, 30));
    for n in 0..20 {
        progress.mark_browser(&format!("x{n}"), 100 + n);
    }
    assert_eq!(progress.browsers.len(), BROWSERS_MAX);
    assert_eq!(progress.browsers.last().unwrap().hash, "x19");
    progress.step_mut(2).done_at = Some(5);
    assert!(progress.any_done());
}
