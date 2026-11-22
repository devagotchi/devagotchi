//! Current permission to read an original admission, independent of execution renewal.
use super::*;
use super::{continuation::AdmissionSnapshot, grant_record::GrantRecord};
use crate::machines::{
    devices::{Devices, MachineBinding},
    link::Links,
};
use devagotchi_agenthub::{
    Actor,
    hosted::HostedBoard,
    policy::{Resource, Subject, catalog as policy},
};
use devagotchi_contracts::{GrantAction, GrantObservation};

pub(crate) struct AdmissionTarget {
    pub(super) snapshot: AdmissionSnapshot,
    pub(super) operation: String,
}
impl AdmissionTarget {
    pub(crate) fn project(&self) -> Option<&str> {
        self.snapshot.project.as_deref()
    }
}
impl Access {
    pub(crate) async fn operation_target(
        &self,
        account: &str,
        actor: &Actor<'_>,
        machine: &MachineBinding,
        operation: &str,
        links: &Links,
    ) -> BridgeResult<AdmissionTarget> {
        let (tenant, id) = transaction!(&self.accounts.store, |c| {
            let found = self.accounts.machine_actor_in(c, account, actor).await?;
            Devices::new(self.accounts.store.clone())
                .check_access_in(c, Some(account), machine)
                .await?;
            let id: String = sqlx::query_scalar("SELECT id FROM machine_admissions
                WHERE tenant_id=$1 AND account=$2 AND principal=$3 AND machine=$4 AND generation=$5 AND operation_id=$6")
                .bind(&found.tenant).bind(account).bind(actor.id).bind(&machine.name)
                .bind(&machine.generation).bind(operation).fetch_optional(c).await?
                .ok_or_else(|| refused("not_found", "no admitted operation belongs to this principal"))?;
            Ok((found.tenant, id))
        })?;
        let snapshot = self.admission_snapshot(&tenant, &id, links).await?;
        if snapshot.binding.account != account
            || snapshot.binding.principal != actor.id
            || snapshot.machine != *machine
            || snapshot.operation_id.as_deref() != Some(operation)
        {
            return Err(BridgeError::new(
                "authority_mismatch",
                "operation admission changed",
            ));
        }
        Ok(AdmissionTarget {
            snapshot,
            operation: operation.into(),
        })
    }

    pub(crate) async fn operation_observation(
        &self,
        account: &str,
        actor: &Actor<'_>,
        board: Option<&HostedBoard>,
        target: &AdmissionTarget,
    ) -> BridgeResult<GrantObservation> {
        let snapshot = &target.snapshot;
        if snapshot.binding.account != account || snapshot.binding.principal != actor.id {
            return Err(BridgeError::new(
                "not_granted",
                "operation belongs to another principal",
            ));
        }
        transaction!(&self.accounts.store, |c| {
            let initial = self.accounts.catalog_actor_in(c, account, false).await?;
            let grant = GrantRecord::load(c, &initial.tenant, &snapshot.binding.grant_id).await?;
            if grant.account != account
                || grant.principal != actor.id
                || grant.machine != snapshot.machine
            {
                return Err(refused(
                    "authority_mismatch",
                    "original operation grant changed identity",
                ));
            }
            // Board first, then current identity/policy, machine and access locks.
            let labels = match (&grant.task, board) {
                (None, None) => vec![],
                (Some(task), Some(board)) if board.tenant() == initial.tenant => {
                    board.access_read_labels_in(c, actor, task).await?
                }
                _ => {
                    return Err(refused(
                        "not_granted",
                        "operation needs its original task board",
                    ));
                }
            };
            let found = self.accounts.machine_actor_in(c, account, actor).await?;
            policy::initialize_in(c, &found.tenant).await?;
            let roles = if actor.human && found.owner {
                vec!["admin".into()]
            } else {
                vec![]
            };
            let subject = Subject {
                id: actor.id,
                human: actor.human,
                kind: actor.policy.as_ref().map_or("human", |p| p.kind.as_str()),
                roles: actor
                    .policy
                    .as_ref()
                    .map_or(roles.as_slice(), |p| p.roles.as_slice()),
            };
            policy::authorize_in(
                c,
                &found.tenant,
                &subject,
                GrantAction::ProcessRead.policy(),
                &Resource {
                    project: grant.task.as_ref().map_or("", |t| t.project.as_str()),
                    labels: &labels,
                },
            )
            .await?;
            Devices::new(self.accounts.store.clone())
                .check_access_in(c, Some(account), &snapshot.machine)
                .await?;
            Self::lock_in(c, &found.tenant).await?;
            let same: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM machine_admissions
                WHERE tenant_id=$1 AND id=$2 AND account=$3 AND principal=$4 AND operation_id=$5)",
            )
            .bind(&found.tenant)
            .bind(&snapshot.binding.admission_id)
            .bind(account)
            .bind(actor.id)
            .bind(&target.operation)
            .fetch_one(&mut *c)
            .await?;
            if !same {
                return Err(refused("not_found", "operation admission is unavailable"));
            }
            let now = Self::now_in(c).await?;
            let settings = Self::settings_in(c, &found.tenant).await?;
            let observation = GrantObservation {
                binding: snapshot.binding.clone(),
                issued_at: now,
                expires_at: now
                    .checked_add(i64::from(settings.authority_ttl_ms))
                    .ok_or_else(|| invalid("observation clock overflow"))?,
            };
            observation.remaining_ms(now)?;
            Ok(observation)
        })
    }
}
