//! What the link offers the rest of the hub about machines' records: a machine's record, an
//! account's machines and their count, joining with a code, the join script's machine and
//! revoking. Each change is audited (`Links::event`).

use devagotchi_contracts::{BridgeError, BridgeResult};

use super::Links;
use crate::machines::{
    devices::{Device, OWNED_LIMIT},
    random,
};

impl Links {
    /// The enrolled record for `device`, if any (revoked ones included).
    pub async fn device(&self, device: &str) -> BridgeResult<Option<Device>> {
        self.devices.get(device).await
    }

    /// The machines `owner` added, revoked ones included.
    pub async fn owned_by(&self, owner: &str) -> BridgeResult<Vec<Device>> {
        self.devices.owned_by(owner).await
    }

    /// How many machines `owner` holds against its quota: enrolled and not revoked, or
    /// with a live code.
    pub async fn held(&self, owner: &str) -> BridgeResult<usize> {
        self.devices.held(owner).await
    }

    /// Spend an account's one-time code and bind `public_key` to `name`, for tests that need
    /// a machine joined without its socket. A real machine binds its key only by signing the
    /// link's challenge (`enroll.rs` `admit`, `.ai/patterns/link.md` rule 8).
    #[cfg(test)]
    pub async fn enroll_owned(
        &self,
        name: &str,
        code: &str,
        public_key: &str,
    ) -> BridgeResult<Device> {
        let device = self.devices.enroll(name, code, public_key, false).await?;
        self.event(
            device.owner.as_deref().unwrap_or(""),
            "join",
            "link.enroll",
            name,
            "",
            "ok",
        )
        .await;
        self.joined(device.owner.as_deref());
        Ok(device)
    }

    /// A one-time code for `owner` to add a machine named `device` ("Add a machine"). The
    /// names in the hub config are the operator's. Audited without the code.
    #[cfg(test)]
    pub async fn issue_owned_code(&self, owner: &str, device: &str) -> BridgeResult<String> {
        self.issue_owned_code_within(owner, device, None).await
    }

    /// `issue_owned_code`, holding the owner to `limit` machines instead of the usual
    /// quota when one is given (an account nobody secured, `tenancy/machines.rs`).
    pub async fn issue_owned_code_within(
        &self,
        owner: &str,
        device: &str,
        limit: Option<usize>,
    ) -> BridgeResult<String> {
        if !crate::config::valid_name(device) {
            return Err(BridgeError::new(
                "invalid_name",
                "a machine name is 1 to 63 lowercase letters, digits and dashes, starting with a letter or digit",
            ));
        }
        if self.config.broker(device).is_some() {
            return Err(BridgeError::new(
                "name_taken",
                format!("{device:?} is already in use on this hub; choose another name"),
            ));
        }
        let limit = if self.config.is_owner(owner) {
            usize::MAX
        } else {
            limit.unwrap_or(OWNED_LIMIT)
        };
        let issued = self
            .devices
            .issue_owned_code(owner, device, random::<16>(), limit)
            .await;
        let outcome = match &issued {
            Ok(_) => "ok".to_string(),
            Err(error) => error.code.clone(),
        };
        self.event(owner, "web", "link.code_issued", device, "", &outcome)
            .await;
        issued
    }

    /// The machine a live code is for, so `/join` can write its script; audited with
    /// the machine's name and `way` (`sh`, `powershell`, `compose`), never the code. Looking
    /// does not spend the code.
    pub async fn join_script(&self, code: &str, way: &str) -> BridgeResult<Option<String>> {
        let device = self.devices.live_code(code).await?;
        if let Some(device) = &device {
            self.event(
                &format!("device:{device}"),
                "join",
                "link.join_script",
                device,
                way,
                "ok",
            )
            .await;
        }
        Ok(device)
    }

    /// Revoke one of `owner`'s machines; its open link closes within seconds.
    pub async fn revoke_owned(&self, owner: &str, device: &str) -> BridgeResult<(bool, usize)> {
        let result = self.devices.revoke_owned(owner, device).await;
        let (counts, outcome) = match &result {
            Ok((revoked, cancelled)) => (
                format!("revoked={revoked} codes_cancelled={cancelled}"),
                "ok",
            ),
            Err(error) => (String::new(), error.code.as_str()),
        };
        self.event(owner, "web", "link.revoke", device, &counts, outcome)
            .await;
        result
    }
}
