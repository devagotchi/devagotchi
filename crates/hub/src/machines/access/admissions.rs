//! One durable admission per broker retry identity; continuation never consumes twice.
//! This domain API does not itself prove native confinement or forward a broker call.
use super::admission_action::AdmissionAction;
use super::*;
use crate::machines::devices::MachineBinding;
use devagotchi_agenthub::{
    Actor,
    hosted::{HostedBoard, TaskLeaseBinding},
};
use devagotchi_contracts::{GrantCall, GrantItem, GrantLevel};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::Row;

pub(crate) struct AdmissionInput<'a> {
    pub action: AdmissionAction,
    pub grant_id: &'a str,
    pub operation_key: &'a str,
    pub input_sha256: &'a str,
    pub call: &'a GrantCall,
}
/// The authenticated broker's current proved sandbox capabilities, not a tool argument.
pub(crate) struct AdmissionMachine<'a> {
    pub binding: &'a MachineBinding,
    pub sandbox_levels: &'a [GrantLevel],
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Material {
    pub item: GrantItem,
    pub call_sha256: String,
    // Earlier local admissions can be retried with their original projection to
    // acquire this field; no action is guessed for autonomous continuation.
    #[serde(default)]
    pub action: Option<AdmissionAction>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Admitted {
    pub admission_id: String,
    pub grant_id: String,
    pub grant_version: i64,
    pub account: String,
    pub principal: String,
    pub machine: String,
    pub generation: String,
    pub operation_key: String,
    pub input_sha256: String,
    pub effective: GrantItem,
    pub issued_at: i64,
    pub expires_at: i64,
    pub task: Option<TaskLeaseBinding>,
    pub replayed: bool,
}
impl Access {
    /// Call projection and digest come from the real broker operation at the trusted
    /// adapter. Grant selection may choose only this account/principal/machine's records.
    pub(crate) async fn admit(
        &self,
        account: &str,
        actor: &Actor<'_>,
        board: Option<&HostedBoard>,
        machine: &AdmissionMachine<'_>,
        input: &AdmissionInput<'_>,
    ) -> BridgeResult<Admitted> {
        if input.operation_key.is_empty()
            || input.operation_key.len() > devagotchi_contracts::MAX_OPERATION_KEY_BYTES
            || input.operation_key.chars().any(char::is_control)
            || input.input_sha256.len() != 64
            || !input
                .input_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(invalid(
                "admission needs a bounded retry key and canonical SHA-256 digest",
            ));
        }
        if input.action.capability() != input.call.capability {
            return Err(invalid(
                "operation policy action does not match its capability",
            ));
        }
        let call_bytes = serde_json::to_vec(&json!({"action":input.action,"call":input.call}))
            .map_err(|_| invalid("invalid operation projection"))?;
        if call_bytes.len() > 262_144 {
            return Err(invalid("operation projection is too large"));
        }
        let call_sha256 = hex::encode(Sha256::digest(&call_bytes));
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes).map_err(|e| BridgeError::new("random", e.to_string()))?;
        let fresh_id = hex::encode(bytes);
        transaction!(&self.accounts.store, |c| {
            let current = self
                .current_in(
                    c,
                    account,
                    actor,
                    board,
                    machine.binding,
                    &super::current::CurrentInput {
                        grant_id: input.grant_id,
                        action: input.action,
                    },
                )
                .await?;
            let super::current::Current {
                tenant,
                grant,
                task,
                now,
                expires_at,
            } = current;
            grant.terms.item.selector.permits(
                grant.terms.item.capability,
                grant.terms.item.sandbox_level,
                input.call,
            )?;
            if !machine.sandbox_levels.contains(&input.call.level) {
                return Err(refused(
                    "sandbox_unavailable",
                    "the authenticated broker has not proved the required sandbox",
                ));
            }
            let mut effective = grant.terms.item.clone();
            effective.sandbox_level = input.call.level;
            let existing=sqlx::query("SELECT id,account,principal,input_sha256,grant_id,grant_version,effective::text,state
                FROM machine_admissions WHERE tenant_id=$1 AND machine=$2 AND generation=$3 AND operation_key=$4")
                .bind(&tenant).bind(&machine.binding.name).bind(&machine.binding.generation).bind(input.operation_key).fetch_optional(&mut *c).await?;
            let (admission_id, replayed) = if let Some(row) = existing {
                let stored: Material =
                    serde_json::from_str(&row.try_get::<String, _>("effective")?)
                        .map_err(|_| refused("access_damaged", "invalid admission material"))?;
                if row.try_get::<String, _>("account")? != account
                    || row.try_get::<String, _>("principal")? != actor.id
                    || row.try_get::<String, _>("input_sha256")? != input.input_sha256
                    || row.try_get::<String, _>("grant_id")? != input.grant_id
                    || row.try_get::<i64, _>("grant_version")? != grant.version
                    || stored.item != effective
                    || stored.call_sha256 != call_sha256
                    || stored.action.is_some_and(|action| action != input.action)
                {
                    return Err(refused(
                        "request_conflict",
                        "retry key already identifies a different admission",
                    ));
                }
                match row.try_get::<String, _>("state")?.as_str() {
                    "admitted" => (),
                    "retired" => {
                        return Err(refused(
                            "request_retired",
                            "operation retry identity is permanently retired",
                        ));
                    }
                    "finished" if input.action == devagotchi_contracts::GrantAction::FileWrite => {
                        ()
                    }
                    "finished" => {
                        return Err(refused(
                            "operation_finished",
                            "operation already finished; read its broker result",
                        ));
                    }
                    _ => return Err(refused("access_damaged", "invalid admission state")),
                }
                let id: String = row.try_get("id")?;
                sqlx::query("UPDATE machine_admissions SET authority_expires_at=$3,effective=$4::jsonb WHERE tenant_id=$1 AND id=$2")
                    .bind(&tenant).bind(&id).bind(expires_at)
                    .bind(encoded(&Material { item: effective.clone(), call_sha256: call_sha256.clone(), action: Some(input.action) })?).execute(&mut *c).await?;
                (id, true)
            } else {
                if grant
                    .terms
                    .uses_max
                    .is_some_and(|max| grant.uses >= i64::from(max))
                {
                    return Err(refused(
                        "grant_exhausted",
                        "grant has no admissions remaining",
                    ));
                }
                sqlx::query("UPDATE machine_grants SET uses=uses+1 WHERE tenant_id=$1 AND id=$2")
                    .bind(&tenant)
                    .bind(input.grant_id)
                    .execute(&mut *c)
                    .await?;
                sqlx::query("INSERT INTO machine_admissions(tenant_id,id,account,principal,machine,generation,operation_key,input_sha256,
                    grant_id,grant_version,effective,state,admitted_at,authority_expires_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11::jsonb,'admitted',$12,$13)")
                    .bind(&tenant).bind(&fresh_id).bind(account).bind(actor.id).bind(&machine.binding.name).bind(&machine.binding.generation)
                    .bind(input.operation_key).bind(input.input_sha256).bind(input.grant_id).bind(grant.version)
                    .bind(encoded(&Material {item:effective.clone(),call_sha256:call_sha256.clone(),action:Some(input.action)})?).bind(now).bind(expires_at).execute(&mut *c).await?;
                Self::audit_in(
                    c,
                    &tenant,
                    actor.id,
                    "grant.admitted",
                    &json!({"grant":input.grant_id,"admission":fresh_id,
                    "machine":machine.binding.name,"capability":input.action.policy()}),
                    now,
                )
                .await?;
                (fresh_id.clone(), false)
            };
            Ok(Admitted {
                admission_id,
                grant_id: input.grant_id.into(),
                grant_version: grant.version,
                account: account.into(),
                principal: actor.id.into(),
                machine: machine.binding.name.clone(),
                generation: machine.binding.generation.clone(),
                operation_key: input.operation_key.into(),
                input_sha256: input.input_sha256.into(),
                effective,
                issued_at: now,
                expires_at,
                task,
                replayed,
            })
        })
    }
}
