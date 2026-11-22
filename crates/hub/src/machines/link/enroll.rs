//! Who may join: a one-time code that is still live, the gate a hosted hub puts on enrolling, and the hello a machine opens with checked against its enrolled key or a code.

use devagotchi_contracts::{
    BridgeError, BridgeResult, LINK_PROTOCOL_VERSION, LinkFrame, MACHINE_KIND_CONTAINER,
    link_signing_message,
};
use ed25519_dalek::{Signature, VerifyingKey};

use super::{Admitted, Links, fingerprint};

pub(super) fn decode<const N: usize>(text: &str) -> Option<[u8; N]> {
    hex::decode(text).ok()?.try_into().ok()
}

impl Links {
    /// The machine a live code (unused, unexpired) is for, if any, without spending it or
    /// auditing: a workflow's page shows its person their own line again while it works.
    pub async fn live_code(&self, code: &str) -> BridgeResult<Option<String>> {
        self.devices.live_code(code).await
    }

    /// Whether `code` may enroll a machine now: refused while the account that issued it
    /// waits to be let in. Looking does not spend the code.
    pub async fn may_enroll(&self, code: &str) -> BridgeResult<()> {
        let Some(gate) = &self.gate else {
            return Ok(());
        };
        let Some(owner) = self.devices.code_owner(code).await? else {
            return Ok(());
        };
        if gate(owner).await {
            Ok(())
        } else {
            Err(BridgeError::new(
                "early_access",
                "the account that issued this code is not let in yet; nothing joins for it until it is",
            ))
        }
    }

    /// A machine enrolled with `owner`'s code: its tenant's machine milestone.
    pub(super) fn joined(&self, owner: Option<&str>) {
        if let (Some(analytics), Some(owner)) = (&self.analytics, owner) {
            analytics.account_event(owner, crate::ops::analytics::funnel::MACHINE);
        }
    }

    /// Check the device's hello: a configured link broker or a machine an account added,
    /// a strong key, a strict signature over this connection's challenge, and an enrolled,
    /// unrevoked key (or a valid one-time code that binds it now). Returns the device name
    /// and its key.
    pub(super) async fn admit(
        &self,
        hello: Option<LinkFrame>,
        nonce: &str,
    ) -> BridgeResult<Admitted> {
        let refuse = |code: &str, message: String| Err(BridgeError::new(code, message));
        let Some(LinkFrame::Hello {
            version,
            device,
            public_key,
            signature,
            enroll_code,
            build,
            kind,
            remote_access,
        }) = hello
        else {
            return refuse("protocol_error", "expected a hello frame".into());
        };
        let container = kind.as_deref() == Some(MACHINE_KIND_CONTAINER);
        if version != LINK_PROTOCOL_VERSION {
            return refuse(
                "protocol_version",
                format!("hub speaks link protocol {LINK_PROTOCOL_VERSION}, device sent {version}"),
            );
        }
        // A configured broker on this host is never a link; a name the config does not
        // know is a machine an account added (checked against its code or record below).
        let configured = match self.config.broker(&device) {
            Some(entry) if entry.link => true,
            Some(_) => {
                return refuse(
                    "unknown_broker",
                    format!("{device:?} is configured as a broker on this hub's host, not a link"),
                );
            }
            None if crate::config::valid_name(&device) => false,
            None => {
                return refuse(
                    "unknown_broker",
                    format!("{device:?} is not a valid machine name"),
                );
            }
        };
        let key = decode::<32>(&public_key)
            .and_then(|bytes| VerifyingKey::from_bytes(&bytes).ok())
            .filter(|key| !key.is_weak())
            .ok_or_else(|| BridgeError::new("bad_key", "public_key is not a strong Ed25519 key"))?;
        let signature = decode::<64>(&signature)
            .map(|bytes| Signature::from_bytes(&bytes))
            .ok_or_else(|| BridgeError::new("bad_signature", "signature is not 64 hex bytes"))?;
        let message = link_signing_message(&self.config.public_url, &device, nonce);
        if key.verify_strict(&message, &signature).is_err() {
            return refuse(
                "bad_signature",
                "signature does not match this challenge".into(),
            );
        }
        if let Some(code) = enroll_code {
            let bound = self
                .devices
                .enroll(&device, &code, &public_key, configured)
                .await?;
            self.event(
                &format!("device:{device}"),
                "link",
                "link.enroll",
                &device,
                &match &bound.owner {
                    Some(owner) => format!("{} owner={owner}", fingerprint(&public_key)),
                    None => fingerprint(&public_key),
                },
                "ok",
            )
            .await;
            self.joined(bound.owner.as_deref());
            return Ok(Admitted {
                device,
                key: public_key,
                joined_for: bound.owner,
                build,
                container,
                remote_access,
            });
        }
        match self.devices.get(&device).await? {
            None => refuse("not_enrolled", format!("{device} has no enrolled key")),
            // A machine the config stopped naming, that no account added, is no longer served.
            Some(record) if !configured && record.owner.is_none() => refuse(
                "unknown_broker",
                format!("{device:?} is no longer configured on this hub"),
            ),
            Some(record) if record.revoked_at.is_some() => refuse(
                "device_revoked",
                format!("{device} was revoked on this hub"),
            ),
            Some(record) if record.public_key != public_key => refuse(
                "key_mismatch",
                format!("{device} is enrolled with a different key"),
            ),
            Some(_) => Ok(Admitted {
                device,
                key: public_key,
                joined_for: None,
                build,
                container,
                remote_access,
            }),
        }
    }
}
