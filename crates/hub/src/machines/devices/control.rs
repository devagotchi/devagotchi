//! Registry-owned operator controls. Mutations lock the machine before access records.
use super::*;
use devagotchi_store::Failed;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct MachineControl {
    pub version: i64,
    pub paused: bool,
    pub gui_paused: bool,
}
impl Devices {
    pub(crate) async fn lock_control_in(
        &self,
        c: &mut sqlx::AnyConnection,
        binding: &MachineBinding,
    ) -> Result<MachineControl, Failed> {
        let row=sqlx::query("SELECT generation,access_version,access_paused,gui_paused FROM machine_devices
            WHERE name=$1 AND owner IS NOT DISTINCT FROM $2::text AND revoked_at IS NULL FOR UPDATE")
            .bind(&binding.name).bind(binding.owner.as_deref()).fetch_optional(c).await?
            .ok_or_else(||BridgeError::new("not_found","machine unavailable to this account"))?;
        if row.try_get::<String, _>("generation")? != binding.generation {
            return Err(
                BridgeError::new("machine_reenrolled", "machine enrollment changed").into(),
            );
        }
        Ok(MachineControl {
            version: row.try_get("access_version")?,
            paused: row.try_get("access_paused")?,
            gui_paused: row.try_get("gui_paused")?,
        })
    }
    /// The caller already holds lock_control_in in this transaction and has checked its
    /// authenticated owner/policy, canonical receipt and expected control version.
    pub(crate) async fn write_control_in(
        &self,
        c: &mut sqlx::AnyConnection,
        binding: &MachineBinding,
        control: &MachineControl,
    ) -> Result<(), Failed> {
        let changed=sqlx::query("UPDATE machine_devices SET access_version=$4,access_paused=$5,gui_paused=$6
            WHERE name=$1 AND owner IS NOT DISTINCT FROM $2::text AND generation=$3 AND revoked_at IS NULL AND access_version=$7")
            .bind(&binding.name).bind(binding.owner.as_deref()).bind(&binding.generation).bind(control.version)
            .bind(control.paused).bind(control.gui_paused).bind(control.version-1).execute(c).await?.rows_affected();
        if changed != 1 {
            return Err(BridgeError::new(
                "version_conflict",
                "machine control changed; reload before saving",
            )
            .into());
        }
        Ok(())
    }
    pub(crate) async fn check_execution_in(
        &self,
        c: &mut sqlx::AnyConnection,
        account: &str,
        binding: &MachineBinding,
        gui: bool,
    ) -> Result<(), Failed> {
        self.check_access_in(c, Some(account), binding).await?;
        let row = sqlx::query(
            "SELECT access_paused,gui_paused FROM machine_devices WHERE name=$1 AND generation=$2",
        )
        .bind(&binding.name)
        .bind(&binding.generation)
        .fetch_one(c)
        .await?;
        if row.try_get::<bool, _>("access_paused")? {
            return Err(
                BridgeError::new("machine_paused", "the operator paused this machine").into(),
            );
        }
        if gui && row.try_get::<bool, _>("gui_paused")? {
            return Err(BridgeError::new(
                "gui_stopped_by_operator",
                "the operator stopped GUI control on this machine",
            )
            .into());
        }
        Ok(())
    }
}
