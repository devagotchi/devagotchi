use super::*;

fn walkthrough(steps: serde_json::Value) -> Walkthrough {
    serde_json::from_value(serde_json::json!({
        "label": "Step by step", "pause": "Pause", "play": "Play", "steps": steps,
    }))
    .unwrap()
}

fn step(screen: serde_json::Value) -> serde_json::Value {
    serde_json::json!({"tab": "One", "caption": "What happens", "screens": [screen]})
}

#[test]
fn a_walkthrough_says_what_is_wrong_with_it_and_where() {
    let chat =
        serde_json::json!({"kind": "chat", "lines": ["you: hi", "ai: hello", "tool: run ls"]});
    assert!(
        walkthrough(serde_json::json!([step(chat.clone())]))
            .check("w")
            .is_ok()
    );
    // By front, each list checked under its own name.
    let by_front = walkthrough(serde_json::json!({"hosted": [step(chat.clone())], "default": []}));
    let why = by_front.check("w").unwrap_err();
    assert!(
        why.contains("(default)") && why.contains("1 to 8 steps"),
        "{why}"
    );
    let too_many: Vec<_> = (0..=MAX_STEPS).map(|_| step(chat.clone())).collect();
    assert!(
        walkthrough(serde_json::json!(too_many))
            .check("w")
            .unwrap_err()
            .contains("1 to 8 steps")
    );
    let mut slow = step(chat.clone());
    slow["seconds"] = 30.into();
    assert!(
        walkthrough(serde_json::json!([slow]))
            .check("w")
            .unwrap_err()
            .contains("seconds is 2 to 10")
    );
    let stray = serde_json::json!({"kind": "chat", "lines": ["someone: hi"]});
    let why = walkthrough(serde_json::json!([step(stray)]))
        .check("w")
        .unwrap_err();
    assert!(
        why.contains("w.steps (default) 1") && why.contains("`you: `"),
        "{why}"
    );
    let notice = serde_json::json!({"kind": "phone", "title": "18:04", "notice": ["Only a title"]});
    assert!(
        walkthrough(serde_json::json!([step(notice)]))
            .check("w")
            .unwrap_err()
            .contains("notice")
    );
    // A field it does not know is refused when it is read.
    let typo = serde_json::json!([{"tab": "One", "caption": "x", "screens": [{"kind": "chat", "lnes": []}]}]);
    assert!(
        serde_json::from_value::<Walkthrough>(serde_json::json!({
            "label": "x", "pause": "p", "play": "p", "steps": typo,
        }))
        .is_err()
    );
}
