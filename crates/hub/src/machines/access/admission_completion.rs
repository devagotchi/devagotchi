//! Broker lifecycle receipts record completion/retirement without granting new authority.
use super::*;
use crate::machines::devices::{Devices, MachineBinding};
use devagotchi_agenthub::Actor;
use serde_json::json;
use sqlx::Row;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Completion {
    Finished {
        operation_id: String,
        exit_code: Option<i32>,
        error_code: Option<String>,
    },
    Retired {
        operation_id: String,
    },
}
impl Completion {
    fn operation(&self) -> &str {
        match self {
            Self::Finished { operation_id, .. } | Self::Retired { operation_id } => operation_id,
        }
    }
    fn state(&self) -> &'static str {
        match self {
            Self::Finished { .. } => "finished",
            Self::Retired { .. } => "retired",
        }
    }
}
impl Access {
    /// Supplied by the authenticated broker response path, never by an agent's tool.
    /// Grant expiry does not erase a result or prevent permanent retry-key retirement.
    pub(crate) async fn complete_admission(
        &self,
        account: &str,
        actor: &Actor<'_>,
        machine: &MachineBinding,
        admission_id: &str,
        completion: &Completion,
    ) -> BridgeResult<bool> {
        let id = completion.operation();
        if id.is_empty()
            || id.len() > 128
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        {
            return Err(invalid("invalid broker operation identity"));
        }
        if let Completion::Finished {
            error_code: Some(code),
            ..
        } = completion
            && (code.is_empty()
                || code.len() > 64
                || !code
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b)))
        {
            return Err(invalid(
                "completion accepts diagnostic codes, not transport error text",
            ));
        }
        transaction!(&self.accounts.store, |c| {
            let found = self.accounts.machine_actor_in(c, account, actor).await?;
            Devices::new(self.accounts.store.clone())
                .check_access_in(c, Some(account), machine)
                .await?;
            Self::lock_in(c, &found.tenant).await?;
            let row=sqlx::query("SELECT account,principal,machine,generation,state,operation_id,outcome::text,grant_id
                FROM machine_admissions WHERE tenant_id=$1 AND id=$2")
                .bind(&found.tenant).bind(admission_id).fetch_optional(&mut *c).await?
                .ok_or_else(||refused("not_found","admission not found"))?;
            if row.try_get::<String, _>("account")? != account
                || row.try_get::<String, _>("principal")? != actor.id
                || row.try_get::<String, _>("machine")? != machine.name
                || row.try_get::<String, _>("generation")? != machine.generation
            {
                return Err(refused(
                    "not_granted",
                    "completion belongs to another admission owner",
                ));
            }
            let state: String = row.try_get("state")?;
            let outcome = row
                .try_get::<Option<String>, _>("outcome")?
                .map(|s| serde_json::from_str::<Completion>(&s))
                .transpose()
                .map_err(|_| refused("access_damaged", "invalid completion receipt"))?;
            if outcome.as_ref() == Some(completion) {
                return Ok(false);
            }
            if row
                .try_get::<Option<String>, _>("operation_id")?
                .as_ref()
                .is_some_and(|old| old != id)
                || state == "retired"
                || state == "finished" && completion.state() != "retired"
            {
                return Err(refused(
                    "request_conflict",
                    "admission already has another terminal broker receipt",
                ));
            }
            if state != "admitted" && state != "finished" {
                return Err(refused("access_damaged", "invalid admission lifecycle"));
            }
            let now = Self::now_in(c).await?;
            sqlx::query("UPDATE machine_admissions SET state=$3,operation_id=$4,outcome=$5::jsonb,finished_at=$6,authority_expires_at=LEAST(authority_expires_at,$6)
                WHERE tenant_id=$1 AND id=$2")
                .bind(&found.tenant).bind(admission_id).bind(completion.state()).bind(id).bind(encoded(completion)?).bind(now).execute(&mut *c).await?;
            let grant: String = row.try_get("grant_id")?;
            Self::audit_in(c,&found.tenant,actor.id,if completion.state()=="finished" {"grant.used"}else{"grant.admission_retired"},
                &json!({"grant":grant,"admission":admission_id,"operation":id,"completion":completion}),now).await?;
            Ok(true)
        })
    }
}
