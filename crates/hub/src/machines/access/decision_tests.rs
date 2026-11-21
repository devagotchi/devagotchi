use super::*;
use super::{
    decisions::{Decided, Decision},
    request_tests::{Fixture, input},
};
use crate::{identity::accounts::agents::AgentEdit, store};
use devagotchi_contracts::GrantDuration;
use sqlx::Row;

impl Fixture {
    pub(super) async fn decide(
        &self,
        id: &str,
        decision: &Decision,
        key: &str,
    ) -> BridgeResult<Changed<Decided>> {
        self.access
            .decide(&self.owner.id, None, id, 1, decision, key)
            .await
    }
    async fn pending(&self) -> String {
        self.request(&input(), "request")
            .await
            .unwrap()
            .result
            .request_ids[0]
            .clone()
    }
    async fn grants_and_decisions(&self) -> (i64, i64) {
        transaction!(&self.access.accounts.store,|c| {
            self.access.accounts.store.scope(c,&self.owner.tenant).await?;
            let grants:i64=sqlx::query_scalar("SELECT count(*) FROM machine_grants WHERE tenant_id=$1")
                .bind(&self.owner.tenant).fetch_one(&mut *c).await?;
            let events:i64=sqlx::query_scalar("SELECT count(*) FROM tenancy_events WHERE tenant_id=$1 AND event IN ('access.approved','access.narrowed','access.denied')")
                .bind(&self.owner.tenant).fetch_one(c).await?;
            Ok((grants,events))
        }).unwrap()
    }
}
fn approve() -> Decision {
    Decision::Approve {
        narrow: Narrow::default(),
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_decision_concurrent_retry_creates_one_narrowed_grant_with_original_risks() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let id = f.pending().await;
    let decision = Decision::Approve {
        narrow: Narrow {
            duration: Some(GrantDuration::Once),
            note: "Run this inspection once".into(),
            ..Default::default()
        },
    };
    let (a, b) = tokio::join!(
        f.decide(&id, &decision, "approval"),
        f.decide(&id, &decision, "approval")
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.result, b.result);
    assert_ne!(a.replayed, b.replayed);
    assert_eq!(a.result.status, "narrowed");
    assert_eq!(a.result.version, 2);
    assert_eq!(a.result.granted.as_ref().unwrap().uses_max, Some(1));
    assert_eq!(f.grants_and_decisions().await, (1, 1));
    transaction!(&f.access.accounts.store,|c| {
        f.access.accounts.store.scope(c,&f.owner.tenant).await?;
        let row=sqlx::query("SELECT g.risk_snapshot::text, (r.prepared_item->'risks')::text AS original,
            g.approved_by,g.principal,g.expires_at,g.uses FROM machine_grants g
            JOIN machine_access_requests r ON r.tenant_id=g.tenant_id AND r.id=g.request_id WHERE g.tenant_id=$1 AND g.id=$2")
            .bind(&f.owner.tenant).bind(&a.result.grant_id).fetch_one(c).await?;
        assert_eq!(row.try_get::<String,_>("risk_snapshot")?,row.try_get::<String,_>("original")?);
        assert_eq!(row.try_get::<String,_>("approved_by")?,f.owner.id);
        assert_eq!(row.try_get::<String,_>("principal")?,f.principal);
        assert_eq!(row.try_get::<Option<i64>,_>("expires_at")?,None);
        assert_eq!(row.try_get::<i64,_>("uses")?,0);
        Ok(())
    }).unwrap();
    assert_eq!(
        f.decide(&id, &approve(), "approval")
            .await
            .unwrap_err()
            .code,
        "request_conflict"
    );
    assert_eq!(
        f.decide(&id, &decision, "other-key")
            .await
            .unwrap_err()
            .code,
        "version_conflict"
    );
    f.devices.revoke_owned(&f.owner.id, "lab").await.unwrap();
    assert_eq!(
        f.decide(&id, &decision, "approval").await.unwrap_err().code,
        "not_found"
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_decision_rejects_widening_stale_version_and_nonowner_without_partial_grant() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let id = f.pending().await;
    let wider = Decision::Approve {
        narrow: Narrow {
            duration: Some(GrantDuration::Hours(8)),
            ..Default::default()
        },
    };
    assert_eq!(
        f.decide(&id, &wider, "wide").await.unwrap_err().code,
        "scope_wider_than_request"
    );
    assert_eq!(
        f.access
            .decide(&f.owner.id, None, &id, 2, &approve(), "stale")
            .await
            .unwrap_err()
            .code,
        "version_conflict"
    );
    let other = f
        .access
        .accounts
        .create_local_user(
            &f.owner.id,
            "Reader",
            "reader@example.test",
            "a good password",
        )
        .await
        .unwrap()
        .account;
    assert_eq!(
        f.access
            .decide(&other.id, None, &id, 1, &approve(), "reader")
            .await
            .unwrap_err()
            .code,
        "not_granted"
    );
    assert_eq!(f.grants_and_decisions().await, (0, 0));
    // Expiry uses the database clock, not the browser's timestamp.
    transaction!(&f.access.accounts.store,|c| {
        f.access.accounts.store.scope(c,&f.owner.tenant).await?;
        sqlx::query("UPDATE machine_access_requests SET created_at=1,expires_at=2 WHERE tenant_id=$1 AND id=$2")
            .bind(&f.owner.tenant).bind(&id).execute(c).await?;
        Ok(())
    }).unwrap();
    assert_eq!(
        f.decide(&id, &approve(), "expired").await.unwrap_err().code,
        "not_pending"
    );
    assert_eq!(f.grants_and_decisions().await, (0, 0));
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_decision_denial_is_durable_and_cooldown_marks_the_repeated_request() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let id = f.pending().await;
    let decision = Decision::Deny {
        note: "Use the existing narrower workspace access".into(),
    };
    let denied = f.decide(&id, &decision, "denial").await.unwrap();
    assert_eq!(denied.result.status, "denied");
    assert!(denied.result.grant_id.is_none());
    assert!(f.decide(&id, &decision, "denial").await.unwrap().replayed);
    assert_eq!(f.grants_and_decisions().await, (0, 1));
    let again = f.request(&input(), "request-again").await.unwrap();
    transaction!(&f.access.accounts.store, |c| {
        f.access.accounts.store.scope(c, &f.owner.tenant).await?;
        let collapsed: bool = sqlx::query_scalar(
            "SELECT collapsed FROM machine_access_requests WHERE tenant_id=$1 AND id=$2",
        )
        .bind(&f.owner.tenant)
        .bind(&again.result.request_ids[0])
        .fetch_one(c)
        .await?;
        assert!(collapsed);
        Ok(())
    })
    .unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_decision_disabled_requester_is_refused_but_can_be_denied() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let id = f.pending().await;
    f.access
        .accounts
        .put_agent(
            &f.owner.id,
            &f.owner.id,
            &AgentEdit {
                id: "worker".into(),
                name: "Worker".into(),
                kind: "codex".into(),
                roles: vec!["worker".into()],
                enabled: false,
                wip_limit: None,
                version: 1,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        f.decide(&id, &approve(), "disabled")
            .await
            .unwrap_err()
            .code,
        "not_granted"
    );
    assert_eq!(f.grants_and_decisions().await, (0, 0));
    f.decide(
        &id,
        &Decision::Deny {
            note: "Requester disabled".into(),
        },
        "deny-disabled",
    )
    .await
    .unwrap();
    assert_eq!(f.grants_and_decisions().await, (0, 1));
}
