//! Owner read/selection helpers keep registry and policy SQL out of transport adapters.
use super::*;
use crate::machines::devices::{Devices, MachineBinding, MachineControl};
use serde_json::Value;

#[derive(Debug, Serialize)]
pub(crate) struct MachineAdministration {
    pub binding: MachineBinding,
    pub control: MachineControl,
}
#[derive(Debug, Serialize)]
pub(crate) struct PolicyPage {
    pub rows: Vec<Value>,
    pub next: Option<String>,
}
impl Access {
    /// A submitted generation is an optimistic precondition, never a credential.
    pub(crate) async fn machine_administration(
        &self,
        person: &str,
        name: &str,
        generation: Option<&str>,
    ) -> BridgeResult<MachineAdministration> {
        if name.is_empty()
            || name.len() > 128
            || generation.is_some_and(|g| g.is_empty() || g.len() > 128)
        {
            return Err(invalid("invalid machine name or generation"));
        }
        let devices = Devices::new(self.accounts.store.clone());
        transaction!(&self.accounts.store, |c| {
            let tenant = self
                .owner_in(c, person, "admin.access.machines.read")
                .await?;
            let account = devices.owned_account_in(c, name).await?;
            let owner = self.accounts.catalog_actor_in(c, &account, false).await?;
            if owner.tenant != tenant {
                return Err(refused("not_found", "machine unavailable"));
            }
            let binding = devices.bind_access_in(c, name, Some(&account)).await?;
            if generation.is_some_and(|expected| expected != binding.generation) {
                return Err(refused(
                    "machine_reenrolled",
                    "machine enrollment changed; reload before changing access",
                ));
            }
            let control = devices.read_control_in(c, &binding).await?;
            Ok(MachineAdministration { binding, control })
        })
    }
    pub(crate) async fn sandbox_policies(
        &self,
        person: &str,
        after: Option<&str>,
        limit: u32,
    ) -> BridgeResult<PolicyPage> {
        if !(1..=100).contains(&limit) || after.is_some_and(|s| s.len() > 256 || s.contains('\0')) {
            return Err(invalid("invalid sandbox policy page"));
        }
        transaction!(&self.accounts.store, |c| {
            let tenant = self
                .owner_in(c, person, "admin.access.policies.read")
                .await?;
            let now = Self::now_in(c).await?;
            let raw: Vec<String> = sqlx::query_scalar("SELECT jsonb_build_object('scope',scope,'machine',machine,
                'generation',generation,'allowed',allowed,'acknowledged',acknowledged,'enabled_by',enabled_by,
                'enabled_at',enabled_at,'expires_at',expires_at,'version',version,
                'live',allowed AND (expires_at IS NULL OR expires_at>$3) AND (scope='environment' OR EXISTS(
                    SELECT 1 FROM machine_devices d WHERE d.name=machine_sandbox_policies.machine
                    AND d.generation=machine_sandbox_policies.generation AND d.revoked_at IS NULL)))::text FROM machine_sandbox_policies
                WHERE tenant_id=$1 AND scope>$2 ORDER BY scope LIMIT $4")
                .bind(&tenant).bind(after.unwrap_or("")).bind(now).bind(i64::from(limit)+1).fetch_all(c).await?;
            let mut rows: Vec<Value> = raw
                .into_iter()
                .map(|s| {
                    serde_json::from_str(&s)
                        .map_err(|_| refused("access_damaged", "invalid sandbox policy record"))
                })
                .collect::<DbResult<_>>()?;
            let next = if rows.len() > limit as usize {
                rows.truncate(limit as usize);
                Some(
                    rows.last().expect("positive policy page")["scope"]
                        .as_str()
                        .ok_or_else(|| refused("access_damaged", "invalid policy scope"))?
                        .to_owned(),
                )
            } else {
                None
            };
            Ok(PolicyPage { rows, next })
        })
    }
}
