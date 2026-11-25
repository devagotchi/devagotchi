use super::*;
use devagotchi_contracts::*;

async fn enrolled(links: &Links) -> crate::machines::devices::MachineBinding {
    let code = links.devices.issue_code("gus", [71; 16]).await.unwrap();
    links
        .devices
        .enroll("gus", &code, "key", true)
        .await
        .unwrap();
    links.devices.link_binding("gus", "key").await.unwrap()
}
fn remote_session(
    links: &Links,
    binding: crate::machines::devices::MachineBinding,
) -> (RemotePending, mpsc::Receiver<(u64, RemoteWireRequest)>) {
    let (mut session, _) = Session::fake(71, None, false);
    let (calls, rx) = mpsc::channel(CALL_QUEUE);
    let pending = RemotePending::default();
    session.remote = RemoteSession {
        version: Some(REMOTE_ACCESS_VERSION),
        binding,
        key: "key".into(),
        calls,
        pending: pending.clone(),
    };
    links.sessions.lock().unwrap().insert("gus".into(), session);
    (pending, rx)
}
fn interruption(origin: GrantOrigin) -> RemoteRequest {
    RemoteRequest::Interrupt {
        notice: GrantInterruption {
            origin,
            grant_id: "grant".into(),
            minimum_version: 2,
        },
    }
}

#[tokio::test]
async fn remote_link_requires_negotiation_and_exact_enrollment_before_queueing() {
    let links = or_skip!(test_links());
    let binding = enrolled(&links).await;
    connect(&links, "gus", 70, None, false);
    assert_eq!(
        links
            .call_remote("gus", RemoteRequest::Capabilities {})
            .await
            .unwrap_err()
            .code,
        "remote_access_unavailable"
    );
    let (_, mut rx) = remote_session(&links, binding.clone());
    for field in 0..3 {
        let mut origin = links.grant_origin(&binding);
        match field {
            0 => origin.hub = "b".repeat(64),
            1 => origin.machine = "other".into(),
            _ => origin.generation = "older".into(),
        }
        assert_eq!(
            links
                .call_remote("gus", interruption(origin))
                .await
                .unwrap_err()
                .code,
            "authority_mismatch"
        );
        assert!(rx.try_recv().is_err());
    }
    links.devices.revoke("gus").await.unwrap();
    let code = links.devices.issue_code("gus", [72; 16]).await.unwrap();
    links
        .devices
        .enroll("gus", &code, "key", true)
        .await
        .unwrap();
    assert_eq!(
        links
            .call_remote("gus", RemoteRequest::Capabilities {})
            .await
            .unwrap_err()
            .code,
        "machine_reenrolled"
    );
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn remote_link_cancellation_releases_pending_and_shared_capacity() {
    let links = Arc::new(or_skip!(test_links()));
    let binding = enrolled(&links).await;
    let (pending, mut rx) = remote_session(&links, binding.clone());
    let worker = links.clone();
    let origin = links.grant_origin(&binding);
    let task = tokio::spawn(async move { worker.call_remote("gus", interruption(origin)).await });
    let (id, wire) = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(
        matches!(wire.remote,RemoteRequest::Interrupt {notice} if notice.origin==links.grant_origin(&binding))
    );
    assert!(pending.lock().unwrap().contains_key(&id));
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(pending.lock().unwrap().is_empty());
    assert_eq!(
        links.sessions.lock().unwrap()["gus"]
            .capacity
            .available_permits(),
        CALL_QUEUE
    );
}

#[tokio::test]
async fn remote_link_does_not_disclose_response_after_reenrollment() {
    let links = Arc::new(or_skip!(test_links()));
    let binding = enrolled(&links).await;
    let (pending, mut rx) = remote_session(&links, binding);
    let worker = links.clone();
    let task = tokio::spawn(async move {
        worker
            .call_remote("gus", RemoteRequest::Capabilities {})
            .await
    });
    let (id, _) = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .unwrap()
        .unwrap();
    links.devices.revoke("gus").await.unwrap();
    let code = links.devices.issue_code("gus", [73; 16]).await.unwrap();
    links
        .devices
        .enroll("gus", &code, "key", true)
        .await
        .unwrap();
    pending
        .lock()
        .unwrap()
        .remove(&id)
        .unwrap()
        .send(RemoteWireResponse::Ok(RemoteReply::Interrupted {
            count: 0,
        }))
        .unwrap();
    assert_eq!(task.await.unwrap().unwrap_err().code, "machine_reenrolled");
}
