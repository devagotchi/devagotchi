//! Independent human approval with immutable request evidence and atomic grant creation.
use super::records::Pending;
use super::*;
use crate::machines::devices::Devices;
use devagotchi_agenthub::{
    Actor,
    hosted::HostedBoard,
    policy::{Resource, Subject, catalog as policy},
};
use devagotchi_contracts::GrantDuration;
use devagotchi_store::receipts::{self, Outcome, Request};
use serde_json::json;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Decision {
    Approve { narrow: Narrow },
    Deny { note: String },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Decided {
    pub request_id: String,
    pub status: String,
    pub version: i64,
    pub grant_id: Option<String>,
    pub granted: Option<GrantTerms>,
}

impl Access {
    /// `person` comes only from an authenticated human adapter. The optional board is
    /// resolved using the stored requester account and project, never the approver's seat.
    pub(crate) async fn decide(
        &self,
        person: &str,
        board: Option<&HostedBoard>,
        id: &str,
        version: i64,
        decision: &Decision,
        key: &str,
    ) -> BridgeResult<Changed<Decided>> {
        let note = match decision {
            Decision::Approve { narrow } => &narrow.note,
            Decision::Deny { note } => note,
        };
        if note.len() > 8192 || note.contains('\0') || version < 1 {
            return Err(invalid("invalid access decision"));
        }
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes).map_err(|e| BridgeError::new("random", e.to_string()))?;
        let grant_id = hex::encode(bytes);
        let original = json!({"id":id,"version":version,"decision":decision});
        transaction!(&self.accounts.store, |c| {
            let owner = self.accounts.catalog_actor_in(c, person, true).await?;
            // Do not lock access rows before a task's board. Material fields never mutate.
            let pending = Pending::load(c, &owner.tenant, id).await?;
            let approving = matches!(decision, Decision::Approve { .. });
            let task = if approving {
                let profile = self
                    .accounts
                    .machine_profile_in(c, &pending.account, &pending.principal)
                    .await?;
                let actor = Actor {
                    id: &pending.principal,
                    human: profile.is_none(),
                    wip_limit: None,
                    policy: profile,
                };
                let task = match (&pending.task, board) {
                    (None, None) => None,
                    (Some(binding), Some(board)) if board.tenant() == owner.tenant => Some(
                        board
                            .check_access_task_in(c, &pending.principal, &actor, binding)
                            .await?,
                    ),
                    _ => {
                        return Err(refused(
                            "not_granted",
                            "approval needs the original task's current lease",
                        ));
                    }
                };
                let requester = self
                    .accounts
                    .machine_actor_in(c, &pending.account, &actor)
                    .await?;
                if requester.tenant != owner.tenant {
                    return Err(refused(
                        "not_granted",
                        "request account changed environment",
                    ));
                }
                policy::initialize_in(c, &owner.tenant).await?;
                let roles = if requester.owner && actor.human {
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
                policy::authorize_in(c, &owner.tenant, &subject, "access.request", &resource)
                    .await?;
                if pending.terms.duration == GrantDuration::UntilRevoked {
                    policy::authorize_in(c, &owner.tenant, &subject, "grant.indefinite", &resource)
                        .await?;
                }
                task
            } else {
                None
            };
            let resource = Resource {
                project: task
                    .as_ref()
                    .or(pending.task.as_ref())
                    .map_or("", |t| t.project.as_str()),
                labels: task
                    .as_ref()
                    .or(pending.task.as_ref())
                    .map_or(&[], |t| t.labels.as_slice()),
            };
            self.owner_resource_in(c, person, "grant.approve", &resource)
                .await?;
            self.catalog.access_scope_in(c, person).await?;
            if approving {
                Devices::new(self.accounts.store.clone())
                    .check_access_in(c, Some(&pending.account), &pending.machine)
                    .await?;
            }
            Self::lock_in(c, &owner.tenant).await?;
            let now = Self::now_in(c).await?;
            let selected = match decision {
                Decision::Approve { narrow } => Some(pending.terms.narrowed(narrow)?),
                _ => None,
            };
            let preset_policy = pending.preset.as_ref().is_some_and(|p| {
                p.data.get("full_access").and_then(|v| v.as_bool()) == Some(true)
                    || p.data.get("requires_policy").is_some_and(|v| !v.is_null())
            });
            if selected
                .as_ref()
                .is_some_and(|t| t.item.needs_unsandboxed_policy() || preset_policy)
            {
                let live = Self::policy_live_in(c, &owner.tenant, &pending.machine, now).await?;
                if !live {
                    return Err(refused(
                        "policy_denied",
                        "unsandboxed policy is not live for this machine",
                    ));
                }
            }
            let request = Request::new(&owner.tenant, person, key, "access_decide", &original)?;
            let reservation = match receipts::begin_in(c, &request).await? {
                Outcome::Replay(value) => {
                    return Ok(Changed {
                        result: serde_json::from_value(value)
                            .map_err(|_| refused("receipt_damaged", "invalid decision receipt"))?,
                        replayed: true,
                    });
                }
                Outcome::Fresh(reservation) => reservation,
            };
            let pending = Pending::load(c, &owner.tenant, id).await?;
            pending.require_pending(version, now, person)?;
            let granted = match decision {
                Decision::Deny { .. } => None,
                Decision::Approve { .. } => {
                    let terms =
                        selected.expect("approval terms validated before receipt reservation");
                    let full = pending.preset.as_ref().is_some_and(|p| {
                        p.data.get("full_access").and_then(|v| v.as_bool()) == Some(true)
                    });
                    sqlx::query("INSERT INTO machine_grants(tenant_id,id,account,principal,machine,machine_binding,task_binding,
                        request_id,capability,item,duration,uses_max,preset_id,full_access,risk_snapshot,approved_by,approved_at,expires_at,version)
                        VALUES($1,$2,$3,$4,$5,$6::jsonb,$7::jsonb,$8,$9,$10::jsonb,$11,$12,$13,$14,$15::jsonb,$16,$17,$18,1)")
                        .bind(&owner.tenant).bind(&grant_id).bind(&pending.account).bind(&pending.principal).bind(&pending.machine.name)
                        .bind(encoded(&pending.machine)?).bind(task.as_ref().map(encoded).transpose()?).bind(id)
                        .bind(&pending.prepared.capability.id).bind(encoded(&terms.item)?).bind(String::from(terms.duration))
                        .bind(terms.uses_max.map(i64::from)).bind(pending.preset.as_ref().map(|p|p.id.as_str())).bind(full)
                        .bind(encoded(&pending.prepared.risks)?).bind(person).bind(now).bind(terms.expires_at(now)?).execute(&mut *c).await?;
                    Some(terms)
                }
            };
            let status = match &granted {
                None => "denied",
                Some(g) if g != &pending.terms => "narrowed",
                _ => "approved",
            };
            let result = Decided {
                request_id: pending.id,
                status: status.into(),
                version: version + 1,
                grant_id: granted.as_ref().map(|_| grant_id.clone()),
                granted,
            };
            sqlx::query("UPDATE machine_access_requests SET status=$3,version=version+1,decided_by=$4,decided_at=$5,decision_note=$6,grant_id=$7
                WHERE tenant_id=$1 AND id=$2")
                .bind(&owner.tenant).bind(id).bind(status).bind(person).bind(now).bind(note).bind(&result.grant_id).execute(&mut *c).await?;
            Self::audit_in(c,&owner.tenant,person,&format!("access.{status}"),
                &json!({"request":id,"grant":result.grant_id,"machine":pending.machine.name,"principal":pending.principal}),now).await?;
            reservation.complete_in(c, &result).await?;
            Ok(Changed {
                result,
                replayed: false,
            })
        })
    }
}
