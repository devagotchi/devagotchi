//! Broker facts are obtained on the authenticated enrollment, never from request arguments.
use super::*;
use crate::machines::{
    devices::{Devices, MachineBinding},
    link::Links,
};
use devagotchi_agenthub::Actor;
use devagotchi_contracts::{GrantLevel, REMOTE_ACCESS_VERSION, RemoteReply, RemoteRequest};

#[derive(Debug, Serialize)]
pub(crate) struct BrokerFacts {
    pub os: String,
    /// None until a broker GUI backend proves its display. Unknown never suppresses risk.
    pub display_server: Option<String>,
    pub sandbox_levels: Vec<GrantLevel>,
}
impl BrokerFacts {
    pub(crate) fn catalog(&self) -> crate::catalog::access::MachineFacts<'_> {
        crate::catalog::access::MachineFacts {
            os: &self.os,
            display_server: self.display_server.as_deref().unwrap_or(""),
            kind: "unknown",
        }
    }
}
impl Access {
    pub(crate) async fn request_options(
        &self,
        account: &str,
        actor: &Actor<'_>,
    ) -> BridgeResult<serde_json::Value> {
        transaction!(&self.accounts.store, |c| {
            let found = self.accounts.machine_actor_in(c, account, actor).await?;
            let mut value = self.catalog.access_summary_in(c, account).await?;
            Self::lock_in(c, &found.tenant).await?;
            let settings = Self::settings_in(c, &found.tenant).await?;
            value["durations"] = serde_json::json!(settings.grant_duration_options);
            value["settings_version"] = settings.version.into();
            Ok(value)
        })
    }
    /// Resolve only this authenticated account's current enrolled machine. Configured
    /// and shared machines require their separate authority resolver, not owner guessing.
    pub(crate) async fn request_machine(
        &self,
        account: &str,
        actor: &Actor<'_>,
        name: &str,
    ) -> BridgeResult<MachineBinding> {
        transaction!(&self.accounts.store, |c| {
            self.accounts.machine_actor_in(c, account, actor).await?;
            Devices::new(self.accounts.store.clone())
                .bind_access_in(c, name, Some(account))
                .await
        })
    }
    pub(crate) async fn broker_facts(
        &self,
        account: &str,
        actor: &Actor<'_>,
        machine: &MachineBinding,
        links: &Links,
    ) -> BridgeResult<BrokerFacts> {
        let current = self.request_machine(account, actor, &machine.name).await?;
        if current != *machine {
            return Err(BridgeError::new(
                "machine_reenrolled",
                "machine enrollment changed",
            ));
        }
        let reply = links
            .call_remote(&machine.name, RemoteRequest::Capabilities {})
            .await?;
        let current = self.request_machine(account, actor, &machine.name).await?;
        if current != *machine {
            return Err(BridgeError::new(
                "machine_reenrolled",
                "machine enrollment changed",
            ));
        }
        match reply {
            RemoteReply::Capabilities {
                version: REMOTE_ACCESS_VERSION,
                os,
                sandbox_levels,
            } if matches!(os.as_str(), "linux" | "windows" | "macos")
                && !sandbox_levels.is_empty()
                && sandbox_levels.len() <= 4
                && sandbox_levels
                    .iter()
                    .enumerate()
                    .all(|(i, level)| !sandbox_levels[..i].contains(level)) =>
            {
                Ok(BrokerFacts {
                    os,
                    display_server: None,
                    sandbox_levels,
                })
            }
            _ => Err(BridgeError::new(
                "invalid_response",
                "broker did not supply valid access capabilities",
            )),
        }
    }
}
