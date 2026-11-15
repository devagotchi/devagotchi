use super::*;
use crate::machines::devices::OWNED_LIMIT;
use ed25519_dalek::{Signer, SigningKey};

#[cfg(unix)]
const TOKENS: &str = "/tmp/tokens.json";
#[cfg(windows)]
const TOKENS: &str = r"C:\tmp\tokens.json";
const HUB: &str = "http://127.0.0.1:8787";

/// Links over a registry on a new database of the test Postgres, or a skip without one.
macro_rules! links {
    ($dir:expr) => {
        match test_links($dir).await {
            Some(found) => found,
            None => return,
        }
    };
}

async fn test_links(_dir: &std::path::Path) -> Option<Links> {
    let config = HubConfig::parse(
        &serde_json::json!({
            "listen": "127.0.0.1:8787",
            "public_url": HUB,
            "auth": { "mode": "dev", "tokens_file": TOKENS },
            "brokers": [ { "name": "gus", "link": true } ],
            "grants": [ { "subject": "jesse", "brokers": ["gus"] } ]
        })
        .to_string(),
    )
    .unwrap();
    let devices = crate::machines::devices::test_registry().await?;
    let links = Links::new(
        Arc::new(config),
        devices,
        crate::ops::audit::test_audit().await?,
    );
    Some(links)
}

fn hello(
    key: &SigningKey,
    hub: &str,
    device: &str,
    nonce: &str,
    code: Option<&str>,
) -> Option<LinkFrame> {
    Some(LinkFrame::Hello {
        version: LINK_PROTOCOL_VERSION,
        device: device.into(),
        public_key: hex::encode(key.verifying_key().to_bytes()),
        signature: hex::encode(
            key.sign(&link_signing_message(hub, device, nonce))
                .to_bytes(),
        ),
        enroll_code: code.map(str::to_string),
        build: None,
    })
}

/// A live session for `device`, as a link that connected leaves it.
fn connect(links: &Links, device: &str) {
    let (calls, _) = mpsc::channel(1);
    let (control, _) = mpsc::channel(1);
    links.sessions.lock().unwrap().insert(
        device.to_string(),
        Session {
            id: 99,
            calls,
            pending: Arc::default(),
            build: None,
            control,
        },
    );
}

#[tokio::test]
async fn a_machine_offline_ten_minutes_or_failing_an_update_reaches_its_peoples_phones_once() {
    let dir = tempfile::tempdir().unwrap();
    let mut links = links!(dir.path());
    links.offline_after = Duration::from_millis(40);
    let links = Arc::new(links);
    let wait = || tokio::time::sleep(Duration::from_millis(200));
    let pushed = |links: &Links| links.pushed.lock().unwrap().clone();

    // A configured machine goes to the accounts granted it; it stays away: one push,
    // whose tap opens its page in the app.
    let code = links.devices.issue_code("gus", [6; 16]).await.unwrap();
    links
        .devices
        .enroll("gus", &code, "gus-key", true)
        .await
        .unwrap();
    connect(&links, "gus");
    let episode = links
        .forget("gus", 99, &Pending::default())
        .expect("its live session");
    assert!(links.last_seen("gus").is_some());
    links.clone().watch_offline("gus".into(), episode);
    wait().await;
    let sent = pushed(&links);
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0].0, vec!["jesse".to_string()]);
    assert_eq!(sent[0].1["title"], "gus is offline");
    assert_eq!(sent[0].1["url"], "/app/machines/gus");

    // Back within the wait: nothing. Dropped again: that drop's own wait, one push.
    connect(&links, "gus");
    let episode = links.forget("gus", 99, &Pending::default()).unwrap();
    links.clone().watch_offline("gus".into(), episode);
    connect(&links, "gus");
    wait().await;
    assert_eq!(pushed(&links).len(), 1, "it came back: no push");
    let first = links.forget("gus", 99, &Pending::default()).unwrap();
    links.clone().watch_offline("gus".into(), first);
    connect(&links, "gus");
    let second = links.forget("gus", 99, &Pending::default()).unwrap();
    links.clone().watch_offline("gus".into(), second);
    wait().await;
    assert_eq!(pushed(&links).len(), 2, "one push for the drop that lasted");

    // A failed update: one push however often the link says so; the next update's
    // failure is another.
    links
        .update_reported("gus", "failed", Some("no space".into()))
        .await;
    links
        .update_reported("gus", "failed", Some("no space".into()))
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
    links.update_reported("gus", "failed", None).await;
    assert_eq!(pushed(&links).len(), 4);

    // A machine an account added goes to that account only; a revoked one is not pushed.
    let code = links.issue_owned_code("alice", "lab").await.unwrap();
    links
        .devices
        .enroll("lab", &code, "lab-key", false)
        .await
        .unwrap();
    links.update_reported("lab", "failed", None).await;
    assert_eq!(pushed(&links).last().unwrap().0, vec!["alice".to_string()]);
    connect(&links, "lab");
    let episode = links.forget("lab", 99, &Pending::default()).unwrap();
    links.devices.revoke("lab").await.unwrap();
    links.clone().watch_offline("lab".into(), episode);
    wait().await;
    assert_eq!(
        pushed(&links).len(),
        5,
        "a revoked machine going offline is not news"
    );
}

#[tokio::test]
async fn enrollment_then_signed_reconnects_are_admitted() {
    let dir = tempfile::tempdir().unwrap();
    let links = links!(dir.path());
    let key = SigningKey::from_bytes(&[9; 32]);
    let code = links.devices.issue_code("gus", [4; 16]).await.unwrap();
    let admitted = links
        .admit(hello(&key, HUB, "gus", "n1", Some(&code)), "n1")
        .await
        .unwrap();
    assert_eq!(
        (admitted.device.as_str(), admitted.joined_for),
        ("gus", None)
    );
    assert!(
        links
            .admit(hello(&key, HUB, "gus", "n2", None), "n2")
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn replays_other_hubs_other_keys_and_revoked_devices_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let links = links!(dir.path());
    let key = SigningKey::from_bytes(&[9; 32]);
    let other = SigningKey::from_bytes(&[8; 32]);
    let code = links.devices.issue_code("gus", [5; 16]).await.unwrap();
    links
        .admit(hello(&key, HUB, "gus", "n1", Some(&code)), "n1")
        .await
        .unwrap();
    let refused = async |frame, nonce: &str| links.admit(frame, nonce).await.unwrap_err().code;
    // A signature over an old challenge does not open a new connection.
    assert_eq!(
        refused(hello(&key, HUB, "gus", "n1", None), "n2").await,
        "bad_signature"
    );
    // Signed for another hub.
    assert_eq!(
        refused(hello(&key, "https://evil.example", "gus", "n3", None), "n3").await,
        "bad_signature"
    );
    // A different key claiming the enrolled name.
    assert_eq!(
        refused(hello(&other, HUB, "gus", "n4", None), "n4").await,
        "key_mismatch"
    );
    // A name nobody configured or added.
    assert_eq!(
        refused(hello(&key, HUB, "dabo", "n5", None), "n5").await,
        "not_enrolled"
    );
    links.devices.revoke("gus").await.unwrap();
    assert_eq!(
        refused(hello(&key, HUB, "gus", "n6", None), "n6").await,
        "device_revoked"
    );
}

#[tokio::test]
async fn a_machine_an_account_added_joins_for_that_account_without_config() {
    let dir = tempfile::tempdir().unwrap();
    let links = links!(dir.path());
    let key = SigningKey::from_bytes(&[11; 32]);
    // An operator's code cannot add a name the config does not have.
    let operator = links.devices.issue_code("unnamed", [1; 16]).await.unwrap();
    assert_eq!(
        links
            .admit(hello(&key, HUB, "unnamed", "n1", Some(&operator)), "n1")
            .await
            .unwrap_err()
            .code,
        "enroll_refused"
    );
    let code = links
        .devices
        .issue_owned_code("acct-a", "platinum", [2; 16], OWNED_LIMIT)
        .await
        .unwrap();
    let joined = links
        .admit(hello(&key, HUB, "platinum", "n2", Some(&code)), "n2")
        .await
        .unwrap();
    assert_eq!(joined.joined_for.as_deref(), Some("acct-a"));
    // Reconnecting later is an ordinary signed hello, and nobody is told it joined again.
    let again = links
        .admit(hello(&key, HUB, "platinum", "n3", None), "n3")
        .await
        .unwrap();
    assert_eq!(
        (again.device.as_str(), again.joined_for),
        ("platinum", None)
    );
    assert_eq!(links.owned_by("acct-a").await.unwrap().len(), 1);
    // A name that could never be a broker stops at the door.
    assert_eq!(
        links
            .admit(hello(&key, HUB, "Bad Name", "n4", None), "n4")
            .await
            .unwrap_err()
            .code,
        "unknown_broker"
    );
}

#[tokio::test]
async fn weak_keys_are_refused_even_with_a_valid_code() {
    let dir = tempfile::tempdir().unwrap();
    let links = links!(dir.path());
    let code = links.devices.issue_code("gus", [7; 16]).await.unwrap();
    // The identity point: a small-order key whose "signatures" anyone can forge.
    let mut identity = [0u8; 32];
    identity[0] = 1;
    let frame = Some(LinkFrame::Hello {
        version: LINK_PROTOCOL_VERSION,
        device: "gus".into(),
        public_key: hex::encode(identity),
        signature: hex::encode([0u8; 64]),
        enroll_code: Some(code),
        build: None,
    });
    assert_eq!(links.admit(frame, "n").await.unwrap_err().code, "bad_key");
    assert!(links.devices.get("gus").await.unwrap().is_none());
}

#[test]
fn the_wire_says_only_refused_unless_retrying_can_help() {
    for code in [
        "unknown_broker",
        "not_enrolled",
        "key_mismatch",
        "device_revoked",
        "enroll_refused",
        "bad_key",
    ] {
        let HubFrame::Refused { code: wire, .. } = wire_refusal(&BridgeError::new(code, "detail"))
        else {
            panic!("expected a refusal");
        };
        assert_eq!(wire, "refused", "{code} must not reach the peer");
    }
    let HubFrame::Refused { code, .. } = wire_refusal(&BridgeError::new("device_store", "busy"))
    else {
        panic!("expected a refusal");
    };
    assert_eq!(code, "unavailable");
}

#[tokio::test]
async fn handshakes_are_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let links = links!(dir.path());
    let held: Vec<_> = (0..MAX_HANDSHAKES)
        .map(|_| links.handshake_permit().unwrap())
        .collect();
    assert!(links.handshake_permit().is_none());
    drop(held);
    assert!(links.handshake_permit().is_some());
}

#[tokio::test]
async fn calls_to_a_device_without_a_link_say_why() {
    let dir = tempfile::tempdir().unwrap();
    let links = links!(dir.path());
    let list = || {
        RequestBody::OperationList(devagotchi_contracts::ListInput {
            offset: 0,
            limit: 1,
        })
    };
    assert_eq!(
        links.call("gus", list()).await.unwrap_err().code,
        "not_enrolled"
    );
    let key = SigningKey::from_bytes(&[9; 32]);
    let code = links.devices.issue_code("gus", [6; 16]).await.unwrap();
    links
        .admit(hello(&key, HUB, "gus", "n", Some(&code)), "n")
        .await
        .unwrap();
    assert_eq!(
        links.call("gus", list()).await.unwrap_err().code,
        "broker_offline"
    );
    links.devices.revoke("gus").await.unwrap();
    assert_eq!(
        links.call("gus", list()).await.unwrap_err().code,
        "device_revoked"
    );
}

#[tokio::test]
async fn a_full_queue_answers_busy_and_leaves_no_waiters() {
    let dir = tempfile::tempdir().unwrap();
    let links = links!(dir.path());
    let key = SigningKey::from_bytes(&[9; 32]);
    let code = links.devices.issue_code("gus", [3; 16]).await.unwrap();
    links
        .admit(hello(&key, HUB, "gus", "n", Some(&code)), "n")
        .await
        .unwrap();
    // A session whose loop never drains: the device stopped reading.
    let (calls, _keep) = mpsc::channel(CALL_QUEUE);
    let pending: Pending = Arc::default();
    for i in 0..CALL_QUEUE as u64 {
        let request = WireRequest {
            api_version: API_VERSION,
            request: RequestBody::OperationList(devagotchi_contracts::ListInput {
                offset: 0,
                limit: 1,
            }),
        };
        calls.try_send((u64::MAX - i, request)).unwrap();
    }
    links.sessions.lock().unwrap().insert(
        "gus".into(),
        Session {
            id: 1,
            calls,
            pending: pending.clone(),
            build: None,
            control: mpsc::channel(1).0,
        },
    );
    let body = RequestBody::OperationList(devagotchi_contracts::ListInput {
        offset: 0,
        limit: 1,
    });
    assert_eq!(links.call("gus", body).await.unwrap_err().code, "link_busy");
    assert!(
        pending.lock().unwrap().is_empty(),
        "a refused call must not leave a waiter"
    );
}
