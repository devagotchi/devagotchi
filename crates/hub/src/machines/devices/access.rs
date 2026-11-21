//! Access transactions share the enrolled device's current authority and revocation lock.
use super::*;
use devagotchi_store::Failed;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MachineBinding {
    pub name: String,
    pub generation: String,
    pub owner: Option<String>,
}
impl Devices {
    /// Resolve metadata for an already authenticated environment administrator.
    /// The caller must prove this account's environment, then bind_access_in rechecks
    /// ownership/enrollment under the machine lock before using it.
    pub(crate) async fn owned_account_in(
        &self,
        c: &mut sqlx::AnyConnection,
        device: &str,
    ) -> Result<String, Failed> {
        let owner: Option<Option<String>> = sqlx::query_scalar(
            "SELECT owner FROM machine_devices WHERE name=$1 AND revoked_at IS NULL",
        )
        .bind(device)
        .fetch_optional(c)
        .await?;
        owner
            .ok_or_else(|| BridgeError::new("not_found", "machine unavailable"))?
            .ok_or_else(|| {
                BridgeError::new(
                    "not_granted",
                    "configured machines need their configured authority resolver",
                )
                .into()
            })
    }
    /// `owner` is resolved by the authenticated host, not tool input. Some(account) only
    /// reaches that account's machines; None only reaches a configured machine after
    /// the host has checked its configured subject-to-machine access. This is composed
    /// after board/policy/catalog locks and before access/admission locks.
    pub(crate) async fn bind_access_in(
        &self,
        c: &mut sqlx::AnyConnection,
        device: &str,
        owner: Option<&str>,
    ) -> Result<MachineBinding, Failed> {
        let row = sqlx::query(
            "SELECT name,generation,owner FROM machine_devices
            WHERE name=$1 AND owner IS NOT DISTINCT FROM $2::text AND revoked_at IS NULL FOR SHARE",
        )
        .bind(device)
        .bind(owner)
        .fetch_optional(c)
        .await?
        .ok_or_else(|| BridgeError::new("not_found", "machine is unavailable to this account"))?;
        Ok(MachineBinding {
            name: row.try_get("name")?,
            generation: row.try_get("generation")?,
            owner: row.try_get("owner")?,
        })
    }
    /// A revoked or re-enrolled machine cannot inherit a previous enrollment's grants.
    /// Bindings are loaded from the host's stored access record, never used as credentials.
    pub(crate) async fn check_access_in(
        &self,
        c: &mut sqlx::AnyConnection,
        owner: Option<&str>,
        binding: &MachineBinding,
    ) -> Result<(), Failed> {
        if binding.owner.as_deref() != owner {
            return Err(BridgeError::new("not_granted", "machine access owner changed").into());
        }
        let current = self.bind_access_in(c, &binding.name, owner).await?;
        if current != *binding {
            return Err(BridgeError::new(
                "machine_reenrolled",
                "machine enrollment changed; request access again",
            )
            .into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store;
    async fn bind(devices: &Devices, owner: Option<&str>) -> BridgeResult<MachineBinding> {
        transaction!(&devices.store, |c| {
            devices.bind_access_in(c, "lab", owner).await
        })
    }
    async fn check(
        devices: &Devices,
        owner: Option<&str>,
        binding: &MachineBinding,
    ) -> BridgeResult<()> {
        transaction!(&devices.store, |c| {
            devices.check_access_in(c, owner, binding).await
        })
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn machine_access_binding_never_revives_after_reenrollment_with_the_same_key() {
        let test = or_skip!(store::testing::postgres());
        let devices = Devices::new(test.store.clone());
        let code = devices
            .issue_owned_code("acct_a", "lab", [90; 16], 16)
            .await
            .unwrap();
        devices
            .enroll("lab", &code, "same-key", false)
            .await
            .unwrap();
        let first = bind(&devices, Some("acct_a")).await.unwrap();
        check(&devices, Some("acct_a"), &first).await.unwrap();
        assert_eq!(
            bind(&devices, Some("acct_b")).await.unwrap_err().code,
            "not_found"
        );
        assert_eq!(bind(&devices, None).await.unwrap_err().code, "not_found");
        devices.revoke_owned("acct_a", "lab").await.unwrap();
        assert_eq!(
            check(&devices, Some("acct_a"), &first)
                .await
                .unwrap_err()
                .code,
            "not_found"
        );
        let code = devices
            .issue_owned_code("acct_a", "lab", [91; 16], 16)
            .await
            .unwrap();
        devices
            .enroll("lab", &code, "same-key", false)
            .await
            .unwrap();
        assert_eq!(
            check(&devices, Some("acct_a"), &first)
                .await
                .unwrap_err()
                .code,
            "machine_reenrolled"
        );
        let second = bind(&devices, Some("acct_a")).await.unwrap();
        assert_ne!(first.generation, second.generation);
        check(&devices, Some("acct_a"), &second).await.unwrap();
        assert_eq!(
            check(&devices, Some("acct_b"), &second)
                .await
                .unwrap_err()
                .code,
            "not_granted"
        );
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn machine_access_binding_keeps_configured_and_owned_names_distinct() {
        let test = or_skip!(store::testing::postgres());
        let devices = Devices::new(test.store.clone());
        let code = devices.issue_code("lab", [92; 16]).await.unwrap();
        devices
            .enroll("lab", &code, "configured-key", true)
            .await
            .unwrap();
        let binding = bind(&devices, None).await.unwrap();
        check(&devices, None, &binding).await.unwrap();
        assert!(bind(&devices, Some("acct_a")).await.is_err());
        devices.revoke("lab").await.unwrap();
        assert!(check(&devices, None, &binding).await.is_err());
    }
}
