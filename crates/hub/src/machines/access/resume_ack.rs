//! Acknowledgment never grants new access. Pause/revoke effects and the receipt are atomic.
use super::control::ControlChange;
use super::*;
use crate::machines::devices::{Devices, MachineBinding};
use devagotchi_store::receipts::{self, Outcome, Request};
use serde_json::json;
use sqlx::Row;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ResumeAction {
    Continue,
    Revoke,
    Pause,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Acknowledged {
    pub alert_id: String,
    pub grant_id: String,
    pub version: i64,
    pub action: ResumeAction,
}
impl Access {
    pub(crate) async fn acknowledge_resume(
        &self,
        person: &str,
        id: &str,
        version: i64,
        action: ResumeAction,
        key: &str,
    ) -> BridgeResult<Changed<Acknowledged>> {
        if version < 1 || id.is_empty() || id.len() > 128 {
            return Err(invalid("invalid resume acknowledgment"));
        }
        let original = json!({"id":id,"version":version,"action":action});
        transaction!(&self.accounts.store, |c| {
            let found = self
                .resume_person_in(c, person, "resume.acknowledge")
                .await?;
            match action {
                ResumeAction::Continue => (),
                ResumeAction::Revoke => {
                    self.owner_in(c, person, "grant.revoke").await?;
                }
                ResumeAction::Pause => {
                    self.owner_in(c, person, "machine.pause").await?;
                }
            }
            let row=sqlx::query("SELECT a.grant_id,g.machine_binding::text FROM machine_resume_alerts a
                JOIN machine_grants g ON g.tenant_id=a.tenant_id AND g.id=a.grant_id WHERE a.tenant_id=$1 AND a.id=$2 AND a.person=$3")
                .bind(&found.tenant).bind(id).bind(person).fetch_optional(&mut *c).await?.ok_or_else(||refused("not_found","resume alert not found for this person"))?;
            let grant: String = row.try_get("grant_id")?;
            let machine: MachineBinding =
                serde_json::from_str(&row.try_get::<String, _>("machine_binding")?)
                    .map_err(|_| refused("access_damaged", "invalid resume machine binding"))?;
            let control = if action == ResumeAction::Pause {
                Some(
                    Devices::new(self.accounts.store.clone())
                        .lock_control_in(c, &machine)
                        .await?,
                )
            } else {
                None
            };
            Self::lock_in(c, &found.tenant).await?;
            let request = Request::new(&found.tenant, person, key, "resume_ack", &original)?;
            let reservation = match receipts::begin_in(c, &request).await? {
                Outcome::Replay(value) => {
                    return Ok(Changed {
                        result: serde_json::from_value(value).map_err(|_| {
                            refused("receipt_damaged", "invalid resume acknowledgment receipt")
                        })?,
                        replayed: true,
                    });
                }
                Outcome::Fresh(r) => r,
            };
            let now = Self::now_in(c).await?;
            let action_name = serde_json::to_value(action).expect("closed resume action");
            let changed=sqlx::query("UPDATE machine_resume_alerts SET acknowledged_at=$4,action=$5,version=version+1
                WHERE tenant_id=$1 AND id=$2 AND person=$3 AND version=$6 AND acknowledged_at IS NULL")
                .bind(&found.tenant).bind(id).bind(person).bind(now).bind(action_name.as_str().expect("resume action")).bind(version).execute(&mut *c).await?.rows_affected();
            if changed != 1 {
                return Err(refused(
                    "not_open",
                    "resume alert changed or is already acknowledged",
                ));
            }
            match action {
                ResumeAction::Continue => (),
                ResumeAction::Revoke => {
                    Self::end_grant_in(c, &found.tenant, &grant, "revoked", person, now).await?;
                }
                ResumeAction::Pause => {
                    self.apply_control_in(
                        c,
                        &found.tenant,
                        person,
                        &machine,
                        control.as_ref().expect("pause control locked"),
                        ControlChange::Pause,
                    )
                    .await?;
                }
            }
            let result = Acknowledged {
                alert_id: id.into(),
                grant_id: grant,
                version: version
                    .checked_add(1)
                    .ok_or_else(|| invalid("alert version overflow"))?,
                action,
            };
            Self::audit_in(
                c,
                &found.tenant,
                person,
                "resume.acknowledged",
                &json!({"alert":id,"grant":result.grant_id,"action":action_name}),
                now,
            )
            .await?;
            reservation.complete_in(c, &result).await?;
            Ok(Changed {
                result,
                replayed: false,
            })
        })
    }
}
