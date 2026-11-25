//! Hub maintenance drives the existing access owners; execution remains at the broker.
use super::*;
use crate::{agenthub::mcp::Boards, machines::link::Links};
use std::time::Duration;

pub(crate) struct AccessRuntime([tokio::task::JoinHandle<()>; 2]);
impl Drop for AccessRuntime {
    fn drop(&mut self) {
        for task in &self.0 {
            task.abort();
        }
    }
}
impl AccessRuntime {
    pub(crate) fn start(accounts: Arc<Accounts>, links: Arc<Links>, boards: Arc<Boards>) -> Self {
        let admissions = super::maintenance::start(accounts.clone(), links.clone(), boards.clone());
        let grants = tokio::spawn(async move {
            let access = Access::new(accounts);
            let mut ticks = tokio::time::interval(Duration::from_secs(5));
            ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                ticks.tick().await;
                if let Err(error) = access.maintain_access(&links, &boards).await {
                    eprintln!("machine access maintenance failed: {}", error.code);
                }
            }
        });
        Self([grants, admissions])
    }
}
impl Access {
    pub(super) async fn maintain_access(
        &self,
        links: &Links,
        boards: &Arc<Boards>,
    ) -> BridgeResult<()> {
        let mut after = None;
        let mut first_error = None;
        loop {
            let environments = self
                .accounts
                .maintenance_environments(after.as_deref())
                .await?;
            if environments.is_empty() {
                return first_error.map_or(Ok(()), Err);
            }
            for tenant in &environments {
                if let Err(error) = self.maintain_environment(links, boards, tenant).await {
                    eprintln!(
                        "machine access environment maintenance failed: {}",
                        error.code
                    );
                    first_error.get_or_insert(error);
                }
            }
            after = environments.last().cloned();
        }
    }
    async fn maintain_environment(
        &self,
        links: &Links,
        boards: &Arc<Boards>,
        tenant: &str,
    ) -> BridgeResult<()> {
        while self.expire_requests(tenant, 100).await? == 100 {}
        let mut grant_after = None;
        let mut first_error = None;
        loop {
            self.deliver_interruptions(links, tenant, 100).await?;
            let page = self
                .reap_candidates(tenant, grant_after.as_deref(), 100)
                .await?;
            if page.is_empty() {
                return first_error.map_or(Ok(()), Err);
            }
            for candidate in &page {
                let result = async {
                    let board = match &candidate.project {
                        None => None,
                        Some(project) => {
                            let registry = boards.clone();
                            let account = candidate.account.clone();
                            let project = project.clone();
                            tokio::task::spawn_blocking(move || {
                                registry.open_account(&account, &project)
                            })
                            .await
                            .map_err(|_| {
                                BridgeError::new("access_worker", "board resolution task failed")
                            })??
                        }
                    };
                    self.reap_grant(tenant, &candidate.id, board.as_ref().map(|b| b.hosted()))
                        .await
                }
                .await;
                if let Err(error) = result {
                    eprintln!("machine grant maintenance failed: {}", error.code);
                    first_error.get_or_insert(error);
                }
            }
            grant_after = page.last().map(|candidate| candidate.id.clone());
        }
    }
}
