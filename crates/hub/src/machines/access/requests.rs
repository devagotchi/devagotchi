//! Atomic request groups. Resolved catalog data never replaces original receipt input.
use super::*;
use crate::{
    catalog::access::{MachineFacts, Prepared, Selection},
    machines::devices::{Devices, MachineBinding},
};
use devagotchi_agenthub::{
    Actor,
    hosted::{HostedBoard, TaskLeaseBinding},
    policy::{Resource, Subject, catalog as policy},
};
use devagotchi_contracts::{GrantCapability, GrantDuration};
use devagotchi_store::receipts::{self, Outcome, Request};
use serde_json::json;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TaskRequest {
    pub project: String,
    pub ticket: String,
    pub fence: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RequestInput {
    pub machine: String,
    pub selection: Selection,
    pub duration: Option<GrantDuration>,
    pub reason: String,
    pub task: Option<TaskRequest>,
    pub uses_max: Option<u32>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Requested {
    pub group_id: String,
    pub request_ids: Vec<String>,
    pub status: String,
}
fn id() -> BridgeResult<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| BridgeError::new("random", error.to_string()))?;
    Ok(hex::encode(bytes))
}
impl Access {
    /// The adapter supplies authenticated actor, account-authorized board and machine
    /// binding/facts from the authenticated broker. None of those are tool authority.
    /// This records a request; it cannot authorize execution or approve itself.
    pub(crate) async fn request(
        &self,
        account: &str,
        actor: &Actor<'_>,
        board: Option<&HostedBoard>,
        machine: &MachineBinding,
        facts: &MachineFacts<'_>,
        input: &RequestInput,
        key: &str,
    ) -> BridgeResult<Changed<Requested>> {
        if input.reason.trim().chars().count() < 20 {
            return Err(BridgeError::new(
                "reason_too_short",
                "explain the access request in at least 20 characters",
            ));
        }
        if input.reason.len() > 8192
            || input.reason.contains('\0')
            || input.uses_max == Some(0)
            || input.machine != machine.name
            || input.machine.len() > 256
        {
            return Err(invalid(
                "invalid access reason, use limit or machine binding",
            ));
        }
        // Allocate outside the retryable transaction, including preset-expanded item IDs.
        let group = id()?;
        let ids = (0..64).map(|_| id()).collect::<BridgeResult<Vec<_>>>()?;
        let original =
            serde_json::to_value(input).map_err(|_| invalid("invalid access request"))?;
        transaction!(&self.accounts.store, |c| {
            let found = self.accounts.catalog_actor_in(c, account, false).await?;
            let task = task_in(c, &found.tenant, actor, board, input.task.as_ref()).await?;
            let found = self.accounts.machine_actor_in(c, account, actor).await?;
            policy::initialize_in(c, &found.tenant).await?;
            let roles = if found.owner && actor.human {
                vec!["admin".into()]
            } else {
                Vec::new()
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
            policy::authorize_in(c, &found.tenant, &subject, "access.request", &resource).await?;
            self.catalog.access_scope_in(c, account).await?;
            // Owned-machine authority is preserved. Configured/shared machine adapters
            // need their existing configured-grant resolver before this can be extended.
            Devices::new(self.accounts.store.clone())
                .check_access_in(c, Some(account), machine)
                .await?;
            Self::lock_in(c, &found.tenant).await?;
            let request = Request::new(&found.tenant, actor.id, key, "access_request", &original)?;
            let reservation = match receipts::begin_in(c, &request).await? {
                Outcome::Replay(value) => {
                    return Ok(Changed {
                        result: serde_json::from_value(value).map_err(|_| {
                            refused("receipt_damaged", "invalid access request receipt")
                        })?,
                        replayed: true,
                    });
                }
                Outcome::Fresh(reservation) => reservation,
            };
            let prepared = self
                .catalog
                .prepare_access_in(c, account, &input.selection, input.duration, facts)
                .await?;
            let settings = Self::settings_in(c, &found.tenant).await?;
            if !settings.grant_duration_options.contains(&prepared.duration) {
                return Err(invalid("duration is not offered by current access settings").into());
            }
            if prepared.duration == GrantDuration::Task && task.is_none() {
                return Err(invalid("task duration requires a live task lease").into());
            }
            if prepared.duration == GrantDuration::UntilRevoked {
                policy::authorize_in(c, &found.tenant, &subject, "grant.indefinite", &resource)
                    .await?;
            }
            let now = Self::now_in(c).await?;
            if prepared.needs_unsandboxed_policy {
                let live = Self::policy_live_in(c, &found.tenant, machine, now).await?;
                if !live {
                    return Err(refused(
                        "policy_denied",
                        "enable an acknowledged unsandboxed policy for this machine first",
                    ));
                }
            }
            if prepared
                .items
                .iter()
                .any(|i| i.item.capability == GrantCapability::Input)
                && !prepared
                    .items
                    .iter()
                    .any(|i| i.item.capability == GrantCapability::Observe)
            {
                return Err(refused(
                    "observe_required",
                    "request GUI observation alongside GUI input",
                ));
            }
            let requested = insert_in(
                c,
                &found.tenant,
                account,
                actor.id,
                machine,
                task.as_ref(),
                input,
                &prepared,
                &settings,
                now,
                &group,
                &ids,
            )
            .await?;
            Self::audit_in(
                c,
                &found.tenant,
                actor.id,
                "access.requested",
                &json!({"group":group,"machine":machine.name,"items":requested.request_ids.len()}),
                now,
            )
            .await?;
            reservation.complete_in(c, &requested).await?;
            Ok(Changed {
                result: requested,
                replayed: false,
            })
        })
    }
}
async fn task_in(
    c: &mut AnyConnection,
    tenant: &str,
    actor: &Actor<'_>,
    board: Option<&HostedBoard>,
    task: Option<&TaskRequest>,
) -> DbResult<Option<TaskLeaseBinding>> {
    match (board, task) {
        (None, None) => Ok(None),
        (Some(board), Some(task)) if board.tenant() == tenant && board.name == task.project => {
            Ok(Some(
                board
                    .bind_access_task_in(c, actor.id, actor, &task.ticket, task.fence)
                    .await?,
            ))
        }
        _ => Err(refused(
            "not_granted",
            "task access needs its authenticated board and current lease",
        )),
    }
}

async fn insert_in(
    c: &mut AnyConnection,
    tenant: &str,
    account: &str,
    principal: &str,
    machine: &MachineBinding,
    task: Option<&TaskLeaseBinding>,
    input: &RequestInput,
    prepared: &Prepared,
    settings: &AccessSettings,
    now: i64,
    group: &str,
    ids: &[String],
) -> DbResult<Requested> {
    let expires = now
        .checked_add(i64::from(settings.request_expiry_minutes) * 60_000)
        .ok_or_else(|| invalid("pending request expiry exceeds the clock"))?;
    let cooldown = now.saturating_sub(i64::from(settings.deny_cooldown_minutes) * 60_000);
    let uses = if prepared.duration == GrantDuration::Once {
        Some(1)
    } else {
        input.uses_max
    };
    let task = task.map(encoded).transpose()?;
    let preset = prepared.preset.as_ref().map(encoded).transpose()?;
    for (id, item) in ids.iter().zip(&prepared.items) {
        let capability = serde_json::to_value(item.item.capability).expect("closed capability");
        let capability = capability.as_str().expect("capability string");
        let selector = encoded(&item.item.selector)?;
        let collapsed: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM machine_access_requests WHERE tenant_id=$1
            AND principal=$2 AND machine=$3 AND capability=$4 AND prepared_item->'item'->'selector'=$5::jsonb
            AND status='denied' AND decided_at>$6)")
            .bind(tenant).bind(principal).bind(&machine.name).bind(capability).bind(selector).bind(cooldown)
            .fetch_one(&mut *c).await?;
        sqlx::query("INSERT INTO machine_access_requests(tenant_id,id,group_id,account,principal,machine,machine_binding,
            task_binding,capability,prepared_item,preset_id,preset_snapshot,duration,uses_max,reason,collapsed,status,created_at,expires_at,version)
            VALUES($1,$2,$3,$4,$5,$6,$7::jsonb,$8::jsonb,$9,$10::jsonb,$11,$12::jsonb,$13,$14,$15,$16,'pending',$17,$18,1)")
            .bind(tenant).bind(id).bind(group).bind(account).bind(principal).bind(&machine.name).bind(encoded(machine)?)
            .bind(task.as_deref()).bind(capability).bind(encoded(item)?).bind(prepared.preset.as_ref().map(|p|p.id.as_str()))
            .bind(preset.as_deref()).bind(String::from(prepared.duration)).bind(uses.map(i64::from)).bind(input.reason.trim())
            .bind(collapsed).bind(now).bind(expires).execute(&mut *c).await?;
    }
    Ok(Requested {
        group_id: group.into(),
        request_ids: ids[..prepared.items.len()].to_vec(),
        status: "pending".into(),
    })
}
