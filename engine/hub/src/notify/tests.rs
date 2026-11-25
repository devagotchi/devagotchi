use crate::machines::devices::test_registry;

#[tokio::test]
async fn push_subscriptions_are_kept_per_person_and_replaced_by_endpoint() {
    let devices = or_skip!(test_registry());
    devices
        .add_push("acct-a", "https://push.example/1", "k1", "a1")
        .await
        .unwrap();
    devices
        .add_push("acct-a", "https://push.example/1", "k2", "a2")
        .await
        .unwrap();
    devices
        .add_push("acct-b", "https://push.example/2", "k3", "a3")
        .await
        .unwrap();
    let mine = devices.push_for("acct-a").await.unwrap();
    assert_eq!(mine.len(), 1);
    assert_eq!(
        (mine[0].p256dh.as_str(), mine[0].auth.as_str()),
        ("k2", "a2")
    );
    devices.drop_push("https://push.example/1").await.unwrap();
    assert!(devices.push_for("acct-a").await.unwrap().is_empty());
    assert_eq!(devices.push_for("acct-b").await.unwrap().len(), 1);
}
