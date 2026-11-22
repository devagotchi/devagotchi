use super::*;

pub(super) struct TaskGrant {
    pub board: Arc<Board>,
    pub lease: Lease,
    pub grant: String,
    principal: String,
}
impl TaskGrant {
    pub fn actor(&self) -> Actor<'_> {
        actor(&self.principal)
    }
    pub async fn release(&self) {
        self.board
            .hosted()
            .release(
                &self.actor(),
                &self.lease.ticket.id,
                self.lease.fence,
                "selection race",
            )
            .await
            .unwrap();
    }
}
fn actor(principal: &str) -> Actor<'_> {
    Actor {
        id: principal,
        human: false,
        wip_limit: None,
        policy: Some(Identity {
            kind: "codex".into(),
            roles: vec!["worker".into()],
        }),
    }
}
pub(super) fn command(key: &str) -> Value {
    json!({"machine":"lab","idempotency_key":key,"command":"git status","executable":"","cwd":"/work","deadline_ms":5000})
}
pub(super) async fn rank(admin: &Admin, grant: &str, rank: i64) {
    let accounts = &admin.hub.accounts;
    let owner = accounts.get(&admin.owner).await.unwrap().unwrap();
    devagotchi_store::transaction!(accounts.store(), |c| {
        accounts.store().scope(c, &owner.tenant).await?;
        sqlx::query("UPDATE machine_grants SET approved_at=$3 WHERE tenant_id=$1 AND id=$2")
            .bind(&owner.tenant)
            .bind(grant)
            .bind(rank)
            .execute(c)
            .await?;
        Ok(())
    })
    .unwrap();
}
pub(super) async fn task_grant(admin: &Admin, project: &str, order: i64) -> TaskGrant {
    let boards = admin.hub.hub.boards.clone();
    let owner = admin.owner.clone();
    let name = project.to_owned();
    let board = tokio::task::spawn_blocking(move || {
        boards.bind(&owner, &name, &[]).unwrap();
        boards.open_account(&owner, &name).unwrap().unwrap()
    })
    .await
    .unwrap();
    let principal = format!("@agent:{}:worker", admin.owner);
    let ticket = board
        .hosted()
        .create(
            &Actor::human(&admin.owner),
            &NewTicket {
                title: format!("Inspect source {order}"),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let lease = board
        .hosted()
        .claim(&actor(&principal), &ticket.id, None)
        .await
        .unwrap();
    let access = Access::new(admin.hub.accounts.clone());
    let machine = access
        .machine_administration(&admin.owner, "lab", None)
        .await
        .unwrap()
        .binding;
    let request = serde_json::from_value(json!({
        "machine":"lab","selection":{"kind":"items","items":[{"capability":"exec","sandbox_level":"read-only","selector":{}}]},
        "duration":"task","reason":"Inspect this source repository for the requested change.",
        "task":{"project":project,"ticket":ticket.id,"fence":lease.fence}
    })).unwrap();
    let pending = access
        .request(
            &admin.owner,
            &actor(&principal),
            Some(board.hosted()),
            &machine,
            &MachineFacts {
                os: "linux",
                display_server: "x11",
                kind: "native",
            },
            &request,
            &format!("task-request-{order}"),
        )
        .await
        .unwrap();
    let grant = access
        .decide(
            &admin.owner,
            Some(board.hosted()),
            &pending.result.request_ids[0],
            1,
            &Decision::Approve {
                narrow: Narrow::default(),
            },
            &format!("task-approve-{order}"),
        )
        .await
        .unwrap()
        .result
        .grant_id
        .unwrap();
    rank(admin, &grant, order).await;
    TaskGrant {
        board,
        lease,
        grant,
        principal,
    }
}
