//! What a tenant's plan lets it hold: setting a tenant's plan (the operator), the room left on
//! it, and the plan a tenant is on (`site/plans.rs`).

use std::{collections::BTreeSet, sync::Mutex};

use devagotchi_contracts::{BridgeError, BridgeResult};
use sqlx::{AnyConnection, Row};

use super::{Access, read_in};
use crate::identity::accounts::*;
use crate::machines::now_secs;
use crate::site::{Plan, Site};
use crate::store::{Failed, secs, transaction};

impl Accounts {
    /// An operator puts `tenant` on `plan` (a plan the site lists), recorded in its chain and
    /// the home tenant's.
    pub async fn set_plan(&self, tenant: &str, plan: &str, operator: &str) -> BridgeResult<()> {
        self.known_plan(plan)?;
        let now = now_secs();
        transaction!(&self.store, |connection| {
            self.store.scope(connection, tenant).await?;
            let changed = sqlx::query("UPDATE tenancy_access SET plan = $2 WHERE tenant_id = $1")
                .bind(tenant)
                .bind(plan)
                .execute(&mut *connection)
                .await?
                .rows_affected();
            if changed == 0 {
                return Err(
                    BridgeError::new("unknown_tenant", format!("no tenant {tenant} here")).into(),
                );
            }
            tenants::record_in(connection, tenant, operator, "tenant.plan", plan, now).await?;
            let home = tenants::home_in(connection).await?;
            self.store.scope(connection, &home).await?;
            tenants::record_in(
                connection,
                &home,
                operator,
                "tenant.plan",
                &format!("{tenant}: {plan}"),
                now,
            )
            .await
        })
    }

    /// Refuse a new AI connection for `tenant` once it has as many live ones (each refresh
    /// family is one) as its plan's `clients`. Inside a transaction scoped to it.
    pub(crate) async fn room_on_the_plan(
        &self,
        connection: &mut AnyConnection,
        tenant: &str,
        now: u64,
    ) -> Result<(), Failed> {
        let Some((access, _)) = read_in(connection, tenant).await? else {
            return Ok(());
        };
        let Some(plan) = self.plan_of(tenant, &access) else {
            return Ok(());
        };
        let crate::site::Cap::AtMost(most) = plan.cap("clients") else {
            return Ok(());
        };
        let live: i64 = sqlx::query(
            "SELECT COUNT(DISTINCT family) FROM oauth_tokens
             WHERE tenant_id = $1 AND kind = 'refresh' AND used_at IS NULL AND expires_at > $2",
        )
        .bind(tenant)
        .bind(secs(now))
        .fetch_one(&mut *connection)
        .await?
        .try_get(0)?;
        let live = usize::try_from(live).unwrap_or(0);
        if live >= most as usize {
            let site = self.site();
            return Err(BridgeError::new(
                "plan_limit",
                site.limit_message(&plan, "clients", most, live, &self.public_url),
            )
            .into());
        }
        Ok(())
    }

    pub(super) fn known_plan(&self, plan: &str) -> BridgeResult<()> {
        if self.site().plan(plan).is_none() {
            return Err(BridgeError::new(
                "unknown_plan",
                format!("site.json lists no plan {plan:?}"),
            ));
        }
        Ok(())
    }

    /// The plan `tenant`, kept on `access`, is on as the site is now: its plan, else the
    /// site's default plan (said in the log once per tenant and plan id).
    pub fn plan_of(&self, tenant: &str, access: &Access) -> Option<Plan> {
        plan_for(&self.site(), tenant, access)
    }
}

/// The plan a tenant kept on `access` is on in `site`; a plan id the site no longer lists
/// is its default plan, said in the log once per tenant and plan id.
pub fn plan_for(site: &Site, tenant: &str, access: &Access) -> Option<Plan> {
    static SAID: Mutex<BTreeSet<(String, String)>> = Mutex::new(BTreeSet::new());
    let (plan, fell_back) = site.plan_for(access.plan.as_deref());
    if fell_back {
        let kept = access.plan.clone().unwrap_or_default();
        let first = SAID
            .lock()
            .map(|mut said| said.insert((tenant.to_string(), kept.clone())))
            .unwrap_or(false);
        if first {
            eprintln!(
                "devagotchi hub: tenant {tenant} is on plan {kept:?}, which site.json no longer lists; holding it to the default plan {:?}",
                plan.map(|plan| plan.id.as_str()).unwrap_or("")
            );
        }
    }
    plan.cloned()
}
