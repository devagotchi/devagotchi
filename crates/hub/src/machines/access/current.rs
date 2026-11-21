//! Admission and continuation share the same live identity, policy and lease checks.
use super::*;
use super::{admission_action::AdmissionAction, grant_record::GrantRecord};
use crate::machines::devices::{Devices, MachineBinding};
use devagotchi_agenthub::{
    Actor,
    hosted::{HostedBoard, TaskLeaseBinding},
    policy::{Resource, Subject, catalog as policy},
};
use devagotchi_contracts::GrantDuration;

pub(super) struct CurrentInput<'a> {
    pub grant_id: &'a str,
    pub action: AdmissionAction,
}
pub(super) struct Current {
    pub tenant: String,
    pub grant: GrantRecord,
    pub task: Option<TaskLeaseBinding>,
    pub now: i64,
    pub expires_at: i64,
}
impl Access {
    /// Board, principal policy, catalog, machine, then access: the same order as
    /// admission, decisions and reaping. The caller retains these transaction locks.
    pub(super) async fn current_in(
        &self,
        c: &mut AnyConnection,
        account: &str,
        actor: &Actor<'_>,
        board: Option<&HostedBoard>,
        machine: &MachineBinding,
        input: &CurrentInput<'_>,
    ) -> DbResult<Current> {
        let found = self.accounts.catalog_actor_in(c, account, false).await?;
        let grant = GrantRecord::load(c, &found.tenant, input.grant_id).await?;
        if grant.account != account || grant.principal != actor.id || grant.machine != *machine {
            return Err(refused(
                "not_granted",
                "grant belongs to another account, principal or machine enrollment",
            ));
        }
        let task = match (&grant.task, board) {
            (None, None) => None,
            (Some(binding), Some(board)) if board.tenant() == found.tenant => Some(
                board
                    .check_access_task_in(c, actor.id, actor, binding)
                    .await?,
            ),
            _ => {
                return Err(refused(
                    "not_granted",
                    "grant needs its original task and live lease",
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
        let resource = Resource {
            project: task.as_ref().map_or("", |t| t.project.as_str()),
            labels: task.as_ref().map_or(&[], |t| t.labels.as_slice()),
        };
        let action = input.action.policy();
        policy::authorize_in(c, &found.tenant, &subject, action, &resource).await?;
        if grant.terms.duration == GrantDuration::UntilRevoked {
            policy::authorize_in(c, &found.tenant, &subject, "grant.indefinite", &resource).await?;
        }
        self.catalog.access_scope_in(c, account).await?;
        let gui = matches!(
            input.action.capability(),
            devagotchi_contracts::GrantCapability::Observe
                | devagotchi_contracts::GrantCapability::Input
                | devagotchi_contracts::GrantCapability::Window
                | devagotchi_contracts::GrantCapability::App
        );
        Devices::new(self.accounts.store.clone())
            .check_execution_in(c, account, machine, gui)
            .await?;
        Self::lock_in(c, &found.tenant).await?;
        // State can change while acquiring the board/policy/machine/access locks.
        let grant = GrantRecord::load(c, &found.tenant, input.grant_id).await?;
        let now = Self::now_in(c).await?;
        grant.require_live(now)?;
        if input.action.capability() == devagotchi_contracts::GrantCapability::Input {
            let indefinite =
                Self::indefinite_allowed_in(c, &found.tenant, &subject, &resource).await?;
            if !Self::observation_in(
                c,
                &found.tenant,
                account,
                actor.id,
                machine,
                task.as_ref(),
                now,
                indefinite,
            )
            .await?
            {
                return Err(refused(
                    "observe_required",
                    "GUI input needs current observation authority",
                ));
            }
        }
        if grant.policy_required && !Self::policy_live_in(c, &found.tenant, machine, now).await? {
            return Err(refused(
                "policy_denied",
                "grant's unsandboxed policy is no longer live",
            ));
        }
        if input.action.capability() != grant.terms.item.capability {
            return Err(refused(
                "access_damaged",
                "admission action differs from its grant capability",
            ));
        }
        let settings = Self::settings_in(c, &found.tenant).await?;
        let mut expires_at = now
            .checked_add(i64::from(settings.authority_ttl_ms))
            .ok_or_else(|| invalid("authority clock overflow"))?;
        if let Some(at) = grant.expires_at {
            expires_at = expires_at.min(at);
        }
        if let Some(task) = &task {
            expires_at = expires_at.min(task.expires_at);
        }
        Ok(Current {
            tenant: found.tenant,
            grant,
            task,
            now,
            expires_at,
        })
    }
}
