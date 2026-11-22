//! Bounded renewal and terminal receipts; native execution stays owned by the broker.
use super::*;
use super::{admission_completion::Completion, renewal::LinkContinuation};
use crate::{agenthub::mcp::Boards, machines::link::Links};
use devagotchi_agenthub::Actor;
use devagotchi_contracts::{OperationState, RemoteReply, RemoteRequest};
use std::time::Duration;

pub(super) fn start(
    accounts: Arc<Accounts>,
    links: Arc<Links>,
    boards: Arc<Boards>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let access = Access::new(accounts);
        let mut ticks = tokio::time::interval(Duration::from_millis(250));
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut last_error = None;
        loop {
            ticks.tick().await;
            if let Err(error) = access.maintain_admissions(&links, &boards).await {
                let now = tokio::time::Instant::now();
                if last_error.is_none_or(|at| now.duration_since(at) >= Duration::from_secs(5)) {
                    eprintln!("machine admission maintenance failed: {}", error.code);
                    last_error = Some(now);
                }
            }
        }
    })
}
impl Access {
    async fn due_admissions(&self, tenant: &str, after: Option<&str>) -> BridgeResult<Vec<String>> {
        transaction!(&self.accounts.store, |c| {
            self.accounts.store.scope(c, tenant).await?;
            Self::lock_in(c, tenant).await?;
            let settings = Self::settings_in(c, tenant).await?;
            let due = Self::now_in(c).await? + i64::from(settings.authority_ttl_ms) / 2;
            Ok(sqlx::query_scalar(
                "SELECT id FROM machine_admissions WHERE tenant_id=$1
                AND state='admitted' AND authority_expires_at<=$2 AND id>$3 ORDER BY id LIMIT 100",
            )
            .bind(tenant)
            .bind(due)
            .bind(after.unwrap_or(""))
            .fetch_all(c)
            .await?)
        })
    }
    pub(super) async fn maintain_admissions(
        &self,
        links: &Arc<Links>,
        boards: &Arc<Boards>,
    ) -> BridgeResult<()> {
        let mut environments_after = None;
        let mut first_error = None;
        loop {
            let environments = self
                .accounts
                .maintenance_environments(environments_after.as_deref())
                .await?;
            if environments.is_empty() {
                return first_error.map_or(Ok(()), Err);
            }
            for tenant in &environments {
                let mut after = None;
                loop {
                    let ids = self.due_admissions(tenant, after.as_deref()).await?;
                    if ids.is_empty() {
                        break;
                    }
                    let mut workers = tokio::task::JoinSet::new();
                    for id in &ids {
                        let (access, links, boards, tenant, id) = (
                            self.clone(),
                            links.clone(),
                            boards.clone(),
                            tenant.clone(),
                            id.clone(),
                        );
                        workers.spawn(async move {
                            tokio::time::timeout(
                                Duration::from_secs(3),
                                access.maintain_admission(&tenant, &id, &links, &boards),
                            )
                            .await
                            .map_err(|_| {
                                BridgeError::new(
                                    "admission_timeout",
                                    "broker maintenance timed out",
                                )
                            })?
                        });
                        if workers.len() == 16 {
                            if let Some(error) = worker_error(workers.join_next().await) {
                                first_error.get_or_insert(error);
                            }
                        }
                    }
                    while let Some(result) = workers.join_next().await {
                        if let Some(error) = worker_error(Some(result)) {
                            first_error.get_or_insert(error);
                        }
                    }
                    after = ids.last().cloned();
                }
            }
            environments_after = environments.last().cloned();
        }
    }
    pub(super) async fn maintain_admission(
        &self,
        tenant: &str,
        id: &str,
        links: &Links,
        boards: &Arc<Boards>,
    ) -> BridgeResult<()> {
        let snapshot = self.admission_snapshot(tenant, id, links).await?;
        let RemoteReply::Status {
            operation_id,
            state,
            exit_code,
        } = links
            .call_remote(
                &snapshot.machine.name,
                RemoteRequest::Status {
                    binding: snapshot.binding.clone(),
                },
            )
            .await?
        else {
            return Err(BridgeError::new(
                "invalid_response",
                "broker did not return admission lifecycle evidence",
            ));
        };
        if operation_id.is_empty()
            || operation_id.len() > 128
            || !operation_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
            || snapshot
                .operation_id
                .as_ref()
                .is_some_and(|old| old != &operation_id)
        {
            return Err(BridgeError::new(
                "invalid_response",
                "broker lifecycle evidence identifies another operation",
            ));
        }
        if state.terminal() {
            if matches!(state, OperationState::Succeeded) && exit_code != Some(0) {
                return Err(BridgeError::new(
                    "invalid_response",
                    "successful broker evidence needs exit code zero",
                ));
            }
            let error_code = match state {
                OperationState::Succeeded => None,
                OperationState::Failed => Some("failed"),
                OperationState::Cancelled => Some("cancelled"),
                OperationState::TimedOut => Some("timed_out"),
                OperationState::Interrupted => Some("interrupted"),
                OperationState::Unknown => Some("unknown"),
                _ => unreachable!("terminal checked"),
            }
            .map(str::to_owned);
            let completion = Completion::Finished {
                operation_id,
                exit_code,
                error_code,
            };
            transaction!(&self.accounts.store, |c| {
                self.accounts.store.scope(c, tenant).await?;
                Self::complete_admission_in(
                    c,
                    tenant,
                    &snapshot.binding.account,
                    &snapshot.binding.principal,
                    &snapshot.machine,
                    id,
                    &completion,
                )
                .await?;
                Ok(())
            })?;
            return Ok(());
        }
        let profile = transaction!(&self.accounts.store, |c| {
            self.accounts.store.scope(c, tenant).await?;
            self.accounts
                .machine_profile_in(c, &snapshot.binding.account, &snapshot.binding.principal)
                .await
        })?;
        let actor = Actor {
            id: &snapshot.binding.principal,
            human: profile.is_none(),
            wip_limit: None,
            policy: profile,
        };
        if snapshot.operation_id.is_none() {
            self.bind_operation(
                &snapshot.binding.account,
                &actor,
                &snapshot.machine,
                id,
                &operation_id,
            )
            .await?;
        }
        let board = match snapshot.project {
            None => None,
            Some(project) => {
                let (registry, account) = (boards.clone(), snapshot.binding.account.clone());
                tokio::task::spawn_blocking(move || registry.open_account(&account, &project))
                    .await
                    .map_err(|_| {
                        BridgeError::new("access_worker", "board resolution task failed")
                    })??
            }
        };
        self.refresh_execution(
            &LinkContinuation {
                account: &snapshot.binding.account,
                actor: &actor,
                board: board.as_ref().map(|b| b.hosted()),
                machine: &snapshot.machine,
                links,
            },
            id,
        )
        .await?;
        Ok(())
    }
}
fn worker_error(
    result: Option<Result<BridgeResult<()>, tokio::task::JoinError>>,
) -> Option<BridgeError> {
    match result {
        Some(Ok(Err(error))) => Some(error),
        Some(Err(_)) => Some(BridgeError::new(
            "access_worker",
            "admission maintenance task failed",
        )),
        _ => None,
    }
}
