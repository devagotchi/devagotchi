use super::*;
use super::{admission_tests::call, broker::LinkExecution, request_tests::Fixture};
use crate::{
    machines::{devices::Devices, link::Links},
    store,
};
use devagotchi_contracts::*;
use tokio::sync::mpsc;

pub(super) async fn links(f: &Fixture) -> (Links, mpsc::Receiver<(u64, RemoteWireRequest)>) {
    #[cfg(windows)]
    let tokens = "D:/devagotchi-validation/tokens.json";
    #[cfg(unix)]
    let tokens = "/tmp/tokens.json";
    let config = crate::config::HubConfig::parse(
        &serde_json::json!({
            "listen":"127.0.0.1:8787","public_url":"http://127.0.0.1:8787",
            "auth":{"mode":"dev","tokens_file":tokens},"brokers":[],"grants":[]
        })
        .to_string(),
    )
    .unwrap();
    let links = Links::new(
        Arc::new(config),
        Arc::new(Devices::new(f.access.accounts.store.clone())),
        crate::ops::audit::test_audit().await.unwrap(),
    );
    let receiver = links.connect_remote_for_test(f.machine.clone(), "machine-key");
    (links, receiver)
}
fn start() -> StartInput {
    StartInput {
        idempotency_key: "operation-key".into(),
        executable: "git".into(),
        args: vec!["status".into()],
        command: None,
        cwd: Some("/work".into()),
        deadline_ms: 5000,
        interactive: None,
        combined_output: false,
        caller: None,
    }
}
async fn received(rx: &mut mpsc::Receiver<(u64, RemoteWireRequest)>) -> (u64, RemoteWireRequest) {
    tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn remote_access_bridge_prepares_then_consumes_one_admission_and_forwards_exact_binding() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let grant = f.once().await;
    let (links, mut rx) = links(&f).await;
    let actor = f.actor();
    let execution = LinkExecution {
        account: &f.owner.id,
        actor: &actor,
        board: None,
        links: &links,
        grant_id: &grant,
        client: "client",
    };
    let broker = async {
        let (id, wire) = received(&mut rx).await;
        assert!(matches!(wire.remote, RemoteRequest::Capabilities {}));
        links.answer_remote_for_test(
            "lab",
            id,
            RemoteWireResponse::Ok(RemoteReply::Capabilities {
                version: REMOTE_ACCESS_VERSION,
                os: "linux".into(),
                sandbox_levels: vec![GrantLevel::ReadOnly],
            }),
        );
        let (id, wire) = received(&mut rx).await;
        assert!(
            matches!(wire.remote,RemoteRequest::Prepare {actor,origin,..} if actor.principal==f.principal && origin.generation==f.machine.generation)
        );
        links.answer_remote_for_test(
            "lab",
            id,
            RemoteWireResponse::Ok(RemoteReply::Prepared {
                action: GrantAction::Exec,
                operation_key: start().idempotency_key,
                input_sha256: "a".repeat(64),
                call: call(),
            }),
        );
        let (id, wire) = received(&mut rx).await;
        let RemoteRequest::Execute {
            authority, request, ..
        } = wire.remote
        else {
            panic!("expected granted execution")
        };
        assert_eq!(authority.binding.principal, f.principal);
        assert_eq!(authority.binding.grant_id, grant);
        assert_eq!(authority.binding.origin, links.grant_origin(&f.machine));
        assert_eq!(authority.binding.input_sha256, "a".repeat(64));
        assert!(matches!(request, RequestBody::OperationStart(_)));
        authority.remaining_ms(authority.issued_at).unwrap();
        // A transport refusal preserves the admission identity; it never refunds/reuses a key.
        links.answer_remote_for_test(
            "lab",
            id,
            RemoteWireResponse::Error(BridgeError::new("observed_refusal", "test")),
        );
        authority
    };
    let (result, authority) = tokio::join!(f.access.execute_start(&execution, start()), broker);
    assert_eq!(result.unwrap_err().code, "observed_refusal");
    let retried = f.admit(&grant, "operation-key", &call()).await.unwrap();
    assert!(retried.replayed);
    assert_eq!(retried.admission_id, authority.binding.admission_id);
}

#[tokio::test]
async fn remote_access_bridge_refuses_unproved_level_before_consuming_a_use() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let grant = f.once().await;
    let (links, mut rx) = links(&f).await;
    let actor = f.actor();
    let execution = LinkExecution {
        account: &f.owner.id,
        actor: &actor,
        board: None,
        links: &links,
        grant_id: &grant,
        client: "client",
    };
    let broker = async {
        let (id, _) = received(&mut rx).await;
        links.answer_remote_for_test(
            "lab",
            id,
            RemoteWireResponse::Ok(RemoteReply::Capabilities {
                version: REMOTE_ACCESS_VERSION,
                os: "linux".into(),
                sandbox_levels: vec![GrantLevel::Unsandboxed],
            }),
        );
    };
    let (result, ()) = tokio::join!(f.access.execute_start(&execution, start()), broker);
    assert_eq!(result.unwrap_err().code, "sandbox_unavailable");
    assert!(rx.try_recv().is_err());
    assert!(
        !f.admit(&grant, "after-refusal", &call())
            .await
            .unwrap()
            .replayed
    );
}

#[tokio::test]
async fn remote_access_interruption_outbox_only_acknowledges_matching_broker_reply() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let grant = f.once().await;
    f.access
        .revoke(&f.owner.id, &grant, 1, "revoke")
        .await
        .unwrap();
    let (links, mut rx) = links(&f).await;
    let broker = async {
        let (id, wire) = received(&mut rx).await;
        assert!(
            matches!(wire.remote,RemoteRequest::Interrupt {notice} if notice.origin==links.grant_origin(&f.machine) && notice.grant_id==grant && notice.minimum_version==2)
        );
        links.answer_remote_for_test(
            "lab",
            id,
            RemoteWireResponse::Ok(RemoteReply::Interrupted { count: 1 }),
        );
    };
    let (result, ()) = tokio::join!(
        f.access.deliver_interruptions(&links, &f.owner.tenant, 100),
        broker
    );
    assert_eq!(result.unwrap(), 1);
    assert_eq!(
        f.access
            .deliver_interruptions(&links, &f.owner.tenant, 100)
            .await
            .unwrap(),
        0
    );
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn remote_access_task_identity_hash_ignores_renewal_and_labels_but_binds_lease() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let grant = f.once().await;
    let mut admitted = f.admit(&grant, "task-hash", &call()).await.unwrap();
    let origin = GrantOrigin {
        hub: "b".repeat(64),
        machine: f.machine.name,
        generation: f.machine.generation,
    };
    admitted.task = Some(devagotchi_agenthub::hosted::TaskLeaseBinding {
        tenant: f.owner.tenant,
        board: "physical-board".into(),
        project: "project".into(),
        ticket: "ticket".into(),
        principal: f.principal,
        fence: 7,
        lease_id: "lease".into(),
        expires_at: 1000,
        labels: vec![],
    });
    let first = admitted.authority(origin.clone()).unwrap().binding;
    let task = admitted.task.as_mut().unwrap();
    task.expires_at += 1000;
    task.labels.push("changed".into());
    assert_eq!(first, admitted.authority(origin.clone()).unwrap().binding);
    admitted.task.as_mut().unwrap().lease_id = "new-lease".into();
    assert_ne!(first, admitted.authority(origin).unwrap().binding);
}
