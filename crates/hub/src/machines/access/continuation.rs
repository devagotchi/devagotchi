//! Renew an existing admission without replaying a command or spending another use.
use super::*;
use super::{
    admissions::{AdmissionMachine, Admitted, Material},
    current::{Current, CurrentInput},
};
use crate::machines::devices::{Devices, MachineBinding};
use devagotchi_agenthub::{Actor, hosted::HostedBoard};
use sqlx::Row;

struct Stored {
    account: String,
    principal: String,
    machine: MachineBinding,
    grant: String,
    version: i64,
    key: String,
    digest: String,
    material: Material,
    state: String,
    operation: Option<String>,
    expires_at: i64,
}
impl Stored {
    async fn load(c: &mut AnyConnection, tenant: &str, id: &str) -> DbResult<Self> {
        let row = sqlx::query(
            "SELECT account,principal,machine,generation,grant_id,grant_version,
            operation_key,input_sha256,effective::text,state,operation_id,authority_expires_at
            FROM machine_admissions WHERE tenant_id=$1 AND id=$2",
        )
        .bind(tenant)
        .bind(id)
        .fetch_optional(c)
        .await?
        .ok_or_else(|| refused("not_found", "admission not found"))?;
        Ok(Self {
            account: row.try_get("account")?,
            principal: row.try_get("principal")?,
            machine: MachineBinding {
                name: row.try_get("machine")?,
                generation: row.try_get("generation")?,
                owner: Some(row.try_get("account")?),
            },
            grant: row.try_get("grant_id")?,
            version: row.try_get("grant_version")?,
            key: row.try_get("operation_key")?,
            digest: row.try_get("input_sha256")?,
            material: serde_json::from_str(&row.try_get::<String, _>("effective")?)
                .map_err(|_| refused("access_damaged", "invalid admission material"))?,
            state: row.try_get("state")?,
            operation: row.try_get("operation_id")?,
            expires_at: row.try_get("authority_expires_at")?,
        })
    }
    fn owner(&self, account: &str, actor: &Actor<'_>, machine: &MachineBinding) -> DbResult<()> {
        if self.account != account || self.principal != actor.id || self.machine != *machine {
            return Err(refused(
                "not_granted",
                "admission belongs to another principal or enrollment",
            ));
        }
        Ok(())
    }
}
pub(crate) struct Continuation {
    pub admitted: Admitted,
    pub operation_id: String,
}
pub(super) struct AdmissionSnapshot {
    pub binding: devagotchi_contracts::GrantBinding,
    pub machine: MachineBinding,
    pub project: Option<String>,
    pub operation_id: Option<String>,
}
impl Access {
    /// Control is authorized against the original live admission without renewal or use.
    pub(crate) async fn operation_control(
        &self,
        account: &str,
        actor: &Actor<'_>,
        board: Option<&HostedBoard>,
        target: &super::operation_read::AdmissionTarget,
        action: devagotchi_contracts::ProcessControl,
    ) -> BridgeResult<devagotchi_contracts::GrantControl> {
        use devagotchi_agenthub::policy::{Resource, Subject, catalog as policy};
        use devagotchi_contracts::{GrantCapability, GrantControl, ProcessControl};
        let snapshot = &target.snapshot;
        if snapshot.binding.account != account || snapshot.binding.principal != actor.id {
            return Err(BridgeError::new(
                "not_granted",
                "operation belongs to another principal",
            ));
        }
        if action != ProcessControl::Cancel
            && snapshot.binding.effective.capability != GrantCapability::Process
        {
            return Err(BridgeError::new(
                "not_granted",
                "input needs the original interactive process grant",
            ));
        }
        transaction!(&self.accounts.store, |c| {
            let found = self.accounts.catalog_actor_in(c, account, false).await?;
            let before = Stored::load(c, &found.tenant, &snapshot.binding.admission_id).await?;
            before.owner(account, actor, &snapshot.machine)?;
            let original = before
                .material
                .action
                .ok_or_else(|| refused("admission_unbound", "operation has no original action"))?;
            let current = self
                .current_in(
                    c,
                    account,
                    actor,
                    board,
                    &snapshot.machine,
                    &CurrentInput {
                        grant_id: &before.grant,
                        action: original,
                    },
                )
                .await?;
            let stored = Stored::load(c, &current.tenant, &snapshot.binding.admission_id).await?;
            stored.owner(account, actor, &snapshot.machine)?;
            if stored.state != "admitted" || stored.expires_at <= current.now {
                return Err(refused(
                    "authority_expired",
                    "only a live admission permits process control",
                ));
            }
            if stored.grant != snapshot.binding.grant_id
                || stored.version != snapshot.binding.grant_version
                || stored.version != current.grant.version
                || stored.key != snapshot.binding.operation_key
                || stored.digest != snapshot.binding.input_sha256
                || stored.material.item != snapshot.binding.effective
                || stored.material.action != Some(original)
                || stored.operation.as_deref() != Some(target.operation.as_str())
            {
                return Err(refused(
                    "authority_mismatch",
                    "control no longer matches the original admission",
                ));
            }
            let roles = if actor.human && found.owner {
                vec!["admin".into()]
            } else {
                vec![]
            };
            policy::authorize_in(
                c,
                &current.tenant,
                &Subject {
                    id: actor.id,
                    human: actor.human,
                    kind: actor.policy.as_ref().map_or("human", |p| p.kind.as_str()),
                    roles: actor
                        .policy
                        .as_ref()
                        .map_or(roles.as_slice(), |p| p.roles.as_slice()),
                },
                action.policy(),
                &Resource {
                    project: current.task.as_ref().map_or("", |t| t.project.as_str()),
                    labels: current.task.as_ref().map_or(&[], |t| t.labels.as_slice()),
                },
            )
            .await?;
            let control = GrantControl {
                binding: snapshot.binding.clone(),
                action,
                issued_at: current.now,
                expires_at: current.expires_at.min(stored.expires_at),
            };
            control.remaining_ms(current.now)?;
            Ok(control)
        })
    }
    /// Tenant/id comes from maintenance or an authenticated, owned-operation lookup.
    /// This identifies the broker record; it is deliberately not live authority.
    pub(super) async fn admission_snapshot(
        &self,
        tenant: &str,
        id: &str,
        links: &crate::machines::link::Links,
    ) -> BridgeResult<AdmissionSnapshot> {
        transaction!(&self.accounts.store, |c| {
            self.accounts.store.scope(c, tenant).await?;
            let stored = Stored::load(c, tenant, id).await?;
            let grant = super::grant_record::GrantRecord::load(c, tenant, &stored.grant).await?;
            if grant.account != stored.account
                || grant.principal != stored.principal
                || grant.machine != stored.machine
            {
                return Err(refused(
                    "access_damaged",
                    "admission no longer identifies its original grant",
                ));
            }
            let record = Admitted {
                admission_id: id.into(),
                grant_id: stored.grant,
                grant_version: stored.version,
                account: stored.account,
                principal: stored.principal,
                machine: stored.machine.name.clone(),
                generation: stored.machine.generation.clone(),
                operation_key: stored.key,
                input_sha256: stored.digest,
                effective: stored.material.item,
                issued_at: 0,
                expires_at: 0,
                task: grant.task,
                replayed: true,
            };
            Ok(AdmissionSnapshot {
                binding: record.binding(links.grant_origin(&stored.machine))?,
                machine: stored.machine,
                project: record.task.map(|t| t.project),
                operation_id: stored.operation,
            })
        })
    }
    /// Only an authenticated broker reply supplies this identity. Never accept an
    /// agent-selected operation ID as evidence that a command was actually admitted.
    pub(crate) async fn bind_operation(
        &self,
        account: &str,
        actor: &Actor<'_>,
        machine: &MachineBinding,
        admission_id: &str,
        operation_id: &str,
    ) -> BridgeResult<()> {
        if operation_id.is_empty()
            || operation_id.len() > 128
            || !operation_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        {
            return Err(invalid("invalid broker operation identity"));
        }
        transaction!(&self.accounts.store, |c| {
            let found = self.accounts.machine_actor_in(c, account, actor).await?;
            Devices::new(self.accounts.store.clone())
                .check_access_in(c, Some(account), machine)
                .await?;
            Self::lock_in(c, &found.tenant).await?;
            let stored = Stored::load(c, &found.tenant, admission_id).await?;
            stored.owner(account, actor, machine)?;
            if stored.operation.as_deref() == Some(operation_id) {
                return Ok(());
            }
            if stored.operation.is_some() || stored.state != "admitted" {
                return Err(refused(
                    "request_conflict",
                    "admission already identifies another broker result",
                ));
            }
            sqlx::query(
                "UPDATE machine_admissions SET operation_id=$3 WHERE tenant_id=$1 AND id=$2",
            )
            .bind(&found.tenant)
            .bind(admission_id)
            .bind(operation_id)
            .execute(c)
            .await?;
            Ok(())
        })
    }

    /// No new admission, no use consumption and no raw command persisted. Rechecks
    /// the exact original action under the same locks as a new admission. An expired
    /// authority cannot be revived, even if the grant itself is still live.
    pub(crate) async fn renew_admission(
        &self,
        account: &str,
        actor: &Actor<'_>,
        board: Option<&HostedBoard>,
        machine: &AdmissionMachine<'_>,
        admission_id: &str,
    ) -> BridgeResult<Continuation> {
        transaction!(&self.accounts.store, |c| {
            let found = self.accounts.catalog_actor_in(c, account, false).await?;
            let before = Stored::load(c, &found.tenant, admission_id).await?;
            before.owner(account, actor, machine.binding)?;
            let action = before.material.action.ok_or_else(|| {
                refused(
                    "admission_unbound",
                    "retry the original operation before continuing this admission",
                )
            })?;
            let Current {
                tenant,
                grant,
                task,
                now,
                expires_at,
            } = self
                .current_in(
                    c,
                    account,
                    actor,
                    board,
                    machine.binding,
                    &CurrentInput {
                        grant_id: &before.grant,
                        action,
                    },
                )
                .await?;
            // Completion or interruption may have committed while acquiring locks.
            let stored = Stored::load(c, &tenant, admission_id).await?;
            stored.owner(account, actor, machine.binding)?;
            if stored.state != "admitted" {
                return Err(refused(
                    "operation_finished",
                    "only an active admission can be renewed",
                ));
            }
            if stored.expires_at <= now {
                return Err(refused(
                    "authority_expired",
                    "expired admission authority cannot be renewed",
                ));
            }
            let mut effective = grant.terms.item.clone();
            effective.sandbox_level = stored.material.item.sandbox_level;
            if stored.grant != before.grant
                || stored.version != grant.version
                || stored.material.action != Some(action)
                || stored.material.item != effective
            {
                return Err(refused(
                    "authority_mismatch",
                    "admission no longer matches its original grant",
                ));
            }
            if !machine.sandbox_levels.contains(&effective.sandbox_level) {
                return Err(refused(
                    "sandbox_unavailable",
                    "broker no longer proves this admission's confinement",
                ));
            }
            let operation_id = stored.operation.ok_or_else(|| {
                refused(
                    "admission_unbound",
                    "admission has no authenticated broker operation receipt",
                )
            })?;
            sqlx::query("UPDATE machine_admissions SET authority_expires_at=$3 WHERE tenant_id=$1 AND id=$2")
                .bind(&tenant).bind(admission_id).bind(expires_at).execute(c).await?;
            Ok(Continuation {
                operation_id,
                admitted: Admitted {
                    admission_id: admission_id.into(),
                    grant_id: stored.grant,
                    grant_version: stored.version,
                    account: stored.account,
                    principal: stored.principal,
                    machine: stored.machine.name,
                    generation: stored.machine.generation,
                    operation_key: stored.key,
                    input_sha256: stored.digest,
                    effective,
                    issued_at: now,
                    expires_at,
                    task,
                    replayed: true,
                },
            })
        })
    }
}
