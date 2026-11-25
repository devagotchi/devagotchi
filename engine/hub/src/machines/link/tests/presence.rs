//! Who hears that a machine went away or failed an update: its people's phones, once.

use super::*;
use crate::machines::now_secs;

#[tokio::test]
async fn a_machine_offline_ten_minutes_or_failing_an_update_reaches_its_peoples_phones_once() {
    let mut links = or_skip!(test_links());
    links.offline_after = Duration::from_millis(40);
    let links = Arc::new(links);
    // Five absence windows: long enough that a push due is made, short enough for a test.
    let window = links.offline_after * 5;
    let wait = || tokio::time::sleep(window);
    let pushed = |links: &Links| links.pushed.lock().unwrap().clone();
    let gone = |links: &Links, device: &str| links.forget(device, 99, &Pending::default());

    // A configured machine goes to the accounts granted it; it stays away: one push,
    // whose tap opens its page in the app.
    let code = links.devices.issue_code("gus", [6; 16]).await.unwrap();
    links
        .devices
        .enroll("gus", &code, "gus-key", true)
        .await
        .unwrap();
    let _live = connect(&links, "gus", 99, None, false);
    let episode = gone(&links, "gus").expect("its live session");
    assert!(links.last_seen("gus").is_some());
    links.clone().watch_offline("gus".into(), episode);
    wait().await;
    let sent = pushed(&links);
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0].0, vec!["jesse".to_string()]);
    assert_eq!(sent[0].1["title"], "gus is offline");
    assert_eq!(sent[0].1["url"], "/app/machines/gus");

    // Back within the wait: nothing. Dropped again: that drop's own wait, one push.
    let _live = connect(&links, "gus", 99, None, false);
    let episode = gone(&links, "gus").unwrap();
    links.clone().watch_offline("gus".into(), episode);
    let _live = connect(&links, "gus", 99, None, false);
    wait().await;
    assert_eq!(pushed(&links).len(), 1, "it came back: no push");
    let first = gone(&links, "gus").unwrap();
    links.clone().watch_offline("gus".into(), first);
    let _live = connect(&links, "gus", 99, None, false);
    let second = gone(&links, "gus").unwrap();
    links.clone().watch_offline("gus".into(), second);
    wait().await;
    assert_eq!(pushed(&links).len(), 2, "one push for the drop that lasted");

    // A failed update: one push however often the link says so; the next update's
    // failure is another.
    links
        .update_reported("gus", UPDATE_FAILED, Some("no space".into()))
        .await;
    links
        .update_reported("gus", UPDATE_FAILED, Some("no space".into()))
        .await;
    let sent = pushed(&links);
    assert_eq!(sent.len(), 3, "{sent:?}");
    assert_eq!(sent[2].1["title"], "gus update failed");
    assert_eq!(sent[2].1["url"], "/app/machines/gus");
    links
        .updates
        .lock()
        .unwrap()
        .insert("gus".into(), Update::new("v2", now_secs()));
    links.update_reported("gus", UPDATE_FAILED, None).await;
    assert_eq!(pushed(&links).len(), 4);

    // A machine an account added goes to that account only; a revoked one is not pushed.
    let code = links.issue_owned_code("alice", "lab").await.unwrap();
    links
        .devices
        .enroll("lab", &code, "lab-key", false)
        .await
        .unwrap();
    links.update_reported("lab", UPDATE_FAILED, None).await;
    assert_eq!(pushed(&links).last().unwrap().0, vec!["alice".to_string()]);
    let _live = connect(&links, "lab", 99, None, false);
    let episode = gone(&links, "lab").unwrap();
    links.devices.revoke("lab").await.unwrap();
    links.clone().watch_offline("lab".into(), episode);
    wait().await;
    assert_eq!(
        pushed(&links).len(),
        5,
        "a revoked machine going offline is not news"
    );
}
