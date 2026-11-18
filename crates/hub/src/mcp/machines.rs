//! A subject's machines as the hub reports them (status, update shown, owned devices), renaming one, and the one line that joins a new machine.

use devagotchi_contracts::BridgeError;
use rmcp::Json;

use crate::{
    identity::Subject,
    mcp::{
        hub::{Hub, Route},
        schema::{Machine, MachineUpdate, MachineUpdated},
    },
};

impl Hub {
    /// The machines `subject` reaches, each with its status (a linked machine is `online` while
    /// its link is connected, which does not show its broker answers: `machine_inspect` asks
    /// it), named as it knows them (on a hosted hub, its tenant's names: `tenancy/machines.rs`).
    pub async fn machine_status(&self, subject: &Subject) -> Vec<Machine> {
        let configured = self.config.brokers_for(&subject.id);
        let owned: Vec<Machine> = self
            .owned(&subject.id)
            .await
            .into_iter()
            .filter(|device| configured.iter().all(|b| b.name != device.name))
            .map(|device| Machine {
                status: if device.revoked_at.is_some() {
                    "revoked"
                } else if self.links.connected(&device.name) {
                    "online"
                } else {
                    "offline"
                },
                name: self.shown_name(subject, &device.name),
                kind: "linked",
                enrolled_at: Some(device.enrolled_at),
                owned: true,
                build: self.links.build(&device.name),
                update: self.update_shown(&device.name),
            })
            .collect();
        let checks = configured.into_iter().map(|broker| async move {
            match &broker.socket {
                Some(socket) => Machine {
                    name: broker.name.clone(),
                    kind: "local",
                    status: if crate::ops::health::reachable(socket).await {
                        "online"
                    } else {
                        "offline"
                    },
                    enrolled_at: None,
                    owned: false,
                    build: None,
                    update: None,
                },
                None => {
                    let (status, enrolled_at) = match self.links.device(&broker.name).await {
                        Ok(Some(device)) if device.revoked_at.is_some() => {
                            ("revoked", Some(device.enrolled_at))
                        }
                        Ok(Some(device)) if self.links.connected(&broker.name) => {
                            ("online", Some(device.enrolled_at))
                        }
                        Ok(Some(device)) => ("offline", Some(device.enrolled_at)),
                        Ok(None) => ("not_enrolled", None),
                        Err(_) => ("offline", None),
                    };
                    Machine {
                        name: broker.name.clone(),
                        kind: "linked",
                        status,
                        enrolled_at,
                        owned: false,
                        build: self.links.build(&broker.name),
                        update: self.update_shown(&broker.name),
                    }
                }
            }
        });
        let mut machines = futures_util::future::join_all(checks).await;
        machines.extend(owned);
        machines
    }

    /// Update one of `subject`'s machines to the hub's build now: what `machine_update`
    /// does, and the phone app's Update now (`web/app/machine.rs`).
    pub async fn update_machine(
        &self,
        subject: &Subject,
        input: MachineUpdate,
    ) -> Result<MachineUpdated, Json<BridgeError>> {
        let device = match self.route(subject, &input.machine).await? {
            Route::Link(device) => device,
            Route::Local(_) => {
                return Err(Json(BridgeError::new(
                    "update_unsupported",
                    format!(
                        "{} is a broker on the hub's own host; it updates with the hub",
                        input.machine
                    ),
                )));
            }
        };
        let asked = self
            .links
            .request_update(&device, input.force, &subject.id, subject.via)
            .await
            .map_err(Json)?;
        let (state, detail) = match asked {
            crate::machines::link::UpdateAsked::AlreadyOn(build) => {
                ("done".to_string(), Some(format!("already on {build}")))
            }
            crate::machines::link::UpdateAsked::Sent => {
                let within =
                    std::time::Duration::from_secs(input.wait_seconds.unwrap_or(50).min(300));
                match self.links.wait_update(&device, within).await {
                    Some(update) => (update.state, update.detail),
                    None => ("requested".to_string(), None),
                }
            }
        };
        Ok(MachineUpdated {
            machine: input.machine,
            state,
            detail,
            build: self.links.build(&device),
        })
    }

    /// The link device behind `machine`, a name as `subject` knows it; None for a broker
    /// on the hub's own host or a machine that is not theirs.
    pub async fn link_device(&self, subject: &Subject, machine: &str) -> Option<String> {
        match self.route(subject, machine).await {
            Ok(Route::Link(device)) => Some(device),
            _ => None,
        }
    }

    /// What `machines` says about `device`'s latest update (`machine_update.rs`).
    pub(super) fn update_shown(&self, device: &str) -> Option<String> {
        let build = self.links.build(device);
        self.links
            .update_of(device)
            .and_then(|update| update.shown(build.as_deref(), crate::machines::now_secs()))
    }

    /// The machines `subject` added with codes of its own (increment 29a). A store that
    /// cannot be read lists none rather than failing the caller's whole request.
    pub(crate) async fn owned(&self, subject: &str) -> Vec<crate::machines::devices::Device> {
        self.links.owned_by(subject).await.unwrap_or_default()
    }
}

/// The one line that joins a machine with `code`, for `os` (`linux`, `windows`, `docker`
/// or `host`): what `machine_add` hands an AI and a workflow's page hands its person.
pub(crate) fn join_line(hub: &str, os: &str, code: &str) -> String {
    let hub = hub.trim_end_matches('/');
    match os {
        "windows" => format!("irm {hub}/join/{code}.ps1 | iex"),
        "docker" => format!(
            "curl -fsSL {hub}/join/{code}.compose.yml -o compose.yml && docker compose -f compose.yml up -d"
        ),
        "host" => format!("curl -fsSL {hub}/join/{code}.host.sh | sudo sh"),
        _ => format!("curl -fsSL {hub}/join/{code} | sudo sh"),
    }
}
