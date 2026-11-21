//! Machine pause invalidates active authority without silently reapproving old operations.
use super::*;
use crate::machines::devices::{Devices, MachineBinding, MachineControl};
use devagotchi_store::receipts::{self, Outcome, Request};
use serde_json::json;
use sqlx::Row;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ControlChange {
    Pause,
    Resume,
    StopGui,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Controlled {
    pub machine: String,
    pub generation: String,
    pub control: MachineControl,
    pub interrupted_grants: u64,
}
impl Access {
    pub(crate) async fn control_machine(
        &self,
        person: &str,
        machine: &MachineBinding,
        version: i64,
        change: ControlChange,
        key: &str,
    ) -> BridgeResult<Changed<Controlled>> {
        if version < 1 {
            return Err(invalid("machine control version must be positive"));
        }
        let original = json!({"machine":machine,"version":version,"change":change});
        transaction!(&self.accounts.store, |c| {
            let tenant = self.owner_in(c, person, "machine.pause").await?;
            let account = machine.owner.as_deref().ok_or_else(|| {
                refused(
                    "not_granted",
                    "configured machine control needs its configured authority resolver",
                )
            })?;
            let owner = self.accounts.catalog_actor_in(c, account, false).await?;
            if owner.tenant != tenant {
                return Err(refused(
                    "not_granted",
                    "machine belongs to another environment",
                ));
            }
            let devices = Devices::new(self.accounts.store.clone());
            let current = devices.lock_control_in(c, machine).await?;
            Self::lock_in(c, &tenant).await?;
            let request = Request::new(&tenant, person, key, "machine_control", &original)?;
            let reservation = match receipts::begin_in(c, &request).await? {
                Outcome::Replay(value) => {
                    return Ok(Changed {
                        result: serde_json::from_value(value).map_err(|_| {
                            refused("receipt_damaged", "invalid machine control receipt")
                        })?,
                        replayed: true,
                    });
                }
                Outcome::Fresh(r) => r,
            };
            if current.version != version {
                return Err(refused(
                    "version_conflict",
                    "machine control changed; reload before saving",
                ));
            }
            let result = self
                .apply_control_in(c, &tenant, person, machine, &current, change)
                .await?;
            reservation.complete_in(c, &result).await?;
            Ok(Changed {
                result,
                replayed: false,
            })
        })
    }
    /// Caller holds the machine control and access locks and has proved human authority.
    pub(super) async fn apply_control_in(
        &self,
        c: &mut AnyConnection,
        tenant: &str,
        person: &str,
        machine: &MachineBinding,
        current: &MachineControl,
        change: ControlChange,
    ) -> DbResult<Controlled> {
        let mut control = current.clone();
        control.version = current
            .version
            .checked_add(1)
            .ok_or_else(|| invalid("machine control version overflow"))?;
        match change {
            ControlChange::Pause => control.paused = true,
            ControlChange::Resume => {
                control.paused = false;
                control.gui_paused = false;
            }
            ControlChange::StopGui => control.gui_paused = true,
        }
        Devices::new(self.accounts.store.clone())
            .write_control_in(c, machine, &control)
            .await?;
        let now = Self::now_in(c).await?;
        let interrupted_grants = if !current.paused && control.paused {
            Self::interrupt_machine_in(c, tenant, machine, false, person, now).await?
        } else if !current.gui_paused && control.gui_paused {
            Self::interrupt_machine_in(c, tenant, machine, true, person, now).await?
        } else {
            0
        };
        let result = Controlled {
            machine: machine.name.clone(),
            generation: machine.generation.clone(),
            control,
            interrupted_grants,
        };
        Self::audit_in(c,tenant,person,match change {ControlChange::Pause=>"machine.paused",ControlChange::Resume=>"machine.resumed",ControlChange::StopGui=>"machine.gui_stopped"},
            &json!({"machine":machine.name,"version":result.control.version,"interrupted_grants":interrupted_grants}),now).await?;
        Ok(result)
    }
    async fn interrupt_machine_in(
        c: &mut AnyConnection,
        tenant: &str,
        machine: &MachineBinding,
        gui: bool,
        person: &str,
        now: i64,
    ) -> DbResult<u64> {
        let mut count = 0;
        let mut cursor = String::new();
        let reason = if gui { "gui_stopped" } else { "machine_paused" };
        loop {
            let rows=sqlx::query("SELECT id,version FROM machine_grants WHERE tenant_id=$1 AND machine=$2 AND machine_binding->>'generation'=$3
                AND ended_at IS NULL AND (NOT $4 OR capability LIKE 'gui.%') AND id>$5 ORDER BY id LIMIT 100")
                .bind(tenant).bind(&machine.name).bind(&machine.generation).bind(gui).bind(&cursor).fetch_all(&mut *c).await?;
            if rows.is_empty() {
                break;
            }
            for row in rows {
                let id: String = row.try_get("id")?;
                let version = row
                    .try_get::<i64, _>("version")?
                    .checked_add(1)
                    .ok_or_else(|| invalid("grant version overflow"))?;
                sqlx::query("UPDATE machine_grants SET version=$3 WHERE tenant_id=$1 AND id=$2")
                    .bind(tenant)
                    .bind(&id)
                    .bind(version)
                    .execute(&mut *c)
                    .await?;
                // Retiring a retry identity is permanent, even after the machine resumes.
                sqlx::query("UPDATE machine_admissions SET state='retired',finished_at=$3,authority_expires_at=LEAST(authority_expires_at,$3)
                    WHERE tenant_id=$1 AND grant_id=$2 AND state='admitted'").bind(tenant).bind(&id).bind(now).execute(&mut *c).await?;
                sqlx::query("INSERT INTO machine_access_notifications(tenant_id,grant_id,version,machine,generation,reason,created_at,next_attempt_at)
                    VALUES($1,$2,$3,$4,$5,$6,$7,$7)").bind(tenant).bind(&id).bind(version).bind(&machine.name).bind(&machine.generation).bind(reason).bind(now).execute(&mut *c).await?;
                Self::audit_in(
                    c,
                    tenant,
                    person,
                    "grant.interrupted",
                    &json!({"grant":id,"machine":machine.name,"version":version,"reason":reason}),
                    now,
                )
                .await?;
                count += 1;
                cursor = id;
            }
        }
        Ok(count)
    }
}
