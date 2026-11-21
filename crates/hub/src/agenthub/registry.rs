//! The hub's boards in its Postgres: which tenant an owner's boards live in, opening one, and
//! the lease reaper's view across tenants. What it asks of the board tables, it asks
//! AgentHub (`hosted::registry`); it runs no SQL on them itself (rule 1).

use crate::agenthub::{Board, fail, hosted::*};
use crate::store::{Lookup, Store, transaction};
use devagotchi_contracts::BridgeResult;
use std::sync::Arc;

pub(crate) struct Registry {
    store: Store,
    live: crate::config::Live,
    runtime: tokio::runtime::Handle,
}

impl Registry {
    pub(crate) fn new(store: Store) -> BridgeResult<Self> {
        Ok(Self {
            store,
            live: Default::default(),
            runtime: tokio::runtime::Handle::try_current()
                .map_err(|error| fail("runtime", error.to_string()))?,
        })
    }

    pub(crate) fn with_live(mut self, live: crate::config::Live) -> Self {
        self.live = live;
        self
    }

    pub(crate) fn blocking<T>(
        &self,
        future: impl std::future::Future<Output = BridgeResult<T>>,
    ) -> BridgeResult<T> {
        crate::agenthub::hosted::blocking(&self.runtime, future)
    }

    async fn tenant(&self, owner: &str) -> BridgeResult<String> {
        // An organisation's boards live in the organisation's own tenant (`crate::orgs`).
        if let Some(tenant) = crate::tenancy::orgs::tenant_of_owner(owner) {
            return Ok(tenant.to_string());
        }
        let mut connection = self.store.read().await?;
        if let Some(tenant) = self
            .store
            .tenant_of(&mut connection, Lookup::Account(owner))
            .await
            .map_err(crate::store::store_error)?
        {
            return Ok(tenant);
        }
        // Configured shared boards and externally authenticated account/org namespaces
        // belong to the operator's home tenant. Namespaces still gate every row access.
        let home: Option<String> =
            sqlx::query_scalar("SELECT id FROM tenancy_tenants WHERE home=1")
                .fetch_optional(&mut *connection)
                .await
                .map_err(crate::store::store_error)?;
        if let Some(home) = home {
            return Ok(home);
        }
        drop(connection);
        let home =
            crate::identity::accounts::tenants::new_tenant_id(crate::machines::random::<16>());
        let now = (crate::agenthub::system_clock()() / 1000) as u64;
        transaction!(&self.store, |connection| {
            crate::identity::accounts::tenants::settle_in(&self.store, connection, &home, now)
                .await?;
            Ok(
                sqlx::query_scalar("SELECT id FROM tenancy_tenants WHERE home=1")
                    .fetch_one(&mut *connection)
                    .await?,
            )
        })
    }

    pub(crate) async fn names(&self, owner: &str) -> BridgeResult<Vec<String>> {
        let tenant = self.tenant(owner).await?;
        registry::names(&self.store, &tenant, owner).await
    }

    async fn leases(&self) -> BridgeResult<Vec<(String, String, i64)>> {
        let tenants: Vec<String> = {
            let mut connection = self.store.read().await?;
            sqlx::query_scalar("SELECT id FROM tenancy_tenants ORDER BY id")
                .fetch_all(&mut *connection)
                .await
                .map_err(crate::store::store_error)?
        };
        let mut leases = Vec::new();
        for tenant in tenants {
            let rows = registry::lease_delays(&self.store, &tenant).await?;
            leases.extend(
                rows.into_iter()
                    .map(|(board, delay)| (tenant.clone(), board, delay)),
            );
        }
        Ok(leases)
    }

    pub(crate) async fn next_lease_delay(&self) -> BridgeResult<Option<i64>> {
        Ok(self
            .leases()
            .await?
            .into_iter()
            .map(|(_, _, delay)| delay)
            .min())
    }

    pub(crate) async fn reap_due(&self) -> BridgeResult<usize> {
        let mut expired = 0;
        for (tenant, board, delay) in self.leases().await? {
            if delay <= 0 {
                expired += HostedBoard::new(self.store.clone(), &tenant, &board)?
                    .reap()
                    .await?;
            }
        }
        Ok(expired)
    }

    pub(crate) async fn open(
        &self,
        owner: &str,
        project: &str,
        create: bool,
        limit: Option<usize>,
    ) -> BridgeResult<Option<(Arc<Board>, bool)>> {
        let tenant = self.tenant(owner).await?;
        let board = HostedBoard::for_owner(self.store.clone(), &tenant, owner, project)?
            .with_principals(Arc::new(crate::identity::accounts::agents::Directory(
                self.live.clone(),
            )));
        let created = if create {
            board.initialize_limited(limit).await?
        } else {
            if !registry::has_board(&self.store, &tenant, owner, project).await? {
                return Ok(None);
            }
            false
        };
        Ok(Some((Arc::new(Board::from_hosted(board).await?), created)))
    }
}
