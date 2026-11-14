use crate::agenthub::{Board, fail, hosted::*};
use crate::store::{Backend, Lookup, Store, transaction};
use devagotchi_contracts::BridgeResult;
use std::sync::Arc;

pub(crate) struct Registry {
    store: Store,
    runtime: tokio::runtime::Handle,
}

impl Registry {
    pub(crate) fn new(store: Store) -> BridgeResult<Self> {
        if store.backend() != Backend::Postgres {
            return Err(fail("invalid_config", "hub boards require Postgres"));
        }
        Ok(Self {
            store,
            runtime: tokio::runtime::Handle::try_current()
                .map_err(|error| fail("runtime", error.to_string()))?,
        })
    }

    pub(crate) fn blocking<T>(
        &self,
        future: impl std::future::Future<Output = BridgeResult<T>>,
    ) -> BridgeResult<T> {
        crate::agenthub::hosted::blocking(&self.runtime, future)
    }

    async fn tenant(&self, owner: &str) -> BridgeResult<String> {
        // An organisation's boards live in the organisation's own tenant (`crate::orgs`).
        if let Some(tenant) = crate::orgs::tenant_of_owner(owner) {
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
        let home = crate::accounts::tenants::new_tenant_id(crate::link::random::<16>());
        let now = (crate::agenthub::system_clock()() / 1000) as u64;
        transaction!(&self.store, |connection| {
            crate::accounts::tenants::settle_in(&self.store, connection, &home, now).await?;
            Ok(
                sqlx::query_scalar("SELECT id FROM tenancy_tenants WHERE home=1")
                    .fetch_one(&mut *connection)
                    .await?,
            )
        })
    }

    pub(crate) async fn names(&self, owner: &str) -> BridgeResult<Vec<String>> {
        let tenant = self.tenant(owner).await?;
        transaction!(&self.store, |connection| {
            self.store.scope(connection, &tenant).await?;
            Ok(sqlx::query_scalar(
                "SELECT project_name FROM board_projects WHERE tenant_id=$1 AND owner_id=$2 ORDER BY project_name COLLATE \"C\"",
            ).bind(&tenant).bind(owner).fetch_all(&mut *connection).await?)
        })
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
            let rows: Vec<(String, i64)> = transaction!(&self.store, |connection| {
                self.store.scope(connection, &tenant).await?;
                Ok(sqlx::query_as(
                    "SELECT board_id, min(lease_expires_at)-(extract(epoch FROM clock_timestamp())*1000)::bigint
                     FROM board_tickets WHERE tenant_id=$1 AND holder IS NOT NULL GROUP BY board_id",
                ).bind(&tenant).fetch_all(&mut *connection).await?)
            })?;
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
        let board = HostedBoard::for_owner(self.store.clone(), &tenant, owner, project)?;
        let created = if create {
            board.initialize_limited(limit).await?
        } else {
            let exists = transaction!(&self.store, |connection| {
                self.store.scope(connection, &tenant).await?;
                let count:i64=sqlx::query_scalar(
                    "SELECT count(*) FROM board_projects WHERE tenant_id=$1 AND owner_id=$2 AND project_name=$3",
                ).bind(&tenant).bind(owner).bind(project).fetch_one(&mut *connection).await?;
                Ok(count == 1)
            })?;
            if !exists {
                return Ok(None);
            }
            false
        };
        Ok(Some((Arc::new(Board::from_hosted(board).await?), created)))
    }
}
