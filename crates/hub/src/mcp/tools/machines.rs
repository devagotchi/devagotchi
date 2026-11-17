//! Who is calling and what they reach: `whoami`, `machines`, and adding or renaming a machine.

use devagotchi_contracts::BridgeError;
use rmcp::{
    Json, RoleServer, handler::server::wrapper::Parameters, service::RequestContext, tool,
    tool_router,
};

use crate::mcp::{
    hub::Hub,
    machines::join_line,
    schema::{MachineAdd, MachineAdded, MachineUpdate, MachineUpdated, Machines, OrgRole, WhoAmI},
};

#[tool_router(router = machines_router, vis = "pub(super)")]
impl Hub {
    #[tool(
        name = "whoami",
        description = "The subject this hub authenticated for the current request and the brokers it may route to. Identity comes from the bearer token, never from arguments.",
        title = "Who am I",
        annotations(title = "Who am I", read_only_hint = true, open_world_hint = false)
    )]
    async fn whoami(
        &self,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<WhoAmI>, Json<BridgeError>> {
        let subject = self.subject(&context)?;
        let access_note = subject.waiting().then(|| self.waiting_text(&subject));
        let plan = self.held_plan(&subject).map(|plan| plan.id);
        Ok(Json(WhoAmI {
            access: subject.access.as_ref().map(|access| {
                if access.waiting {
                    crate::tenancy::access::WAITING
                } else {
                    crate::tenancy::access::ADMITTED
                }
                .to_string()
            }),
            waiting_since: subject
                .access
                .as_ref()
                .filter(|access| access.waiting)
                .and_then(|access| access.waiting_since),
            access_note,
            plan,
            brokers: self.machine_names(&subject).await,
            boards: {
                let hub = self.clone();
                let who = subject.clone();
                tokio::task::spawn_blocking(move || hub.board_names(&who))
                    .await
                    .unwrap_or_else(|join| {
                        Err(BridgeError::new(
                            "board_store",
                            format!("board call did not complete: {join}"),
                        ))
                    })
                    .map_err(Json)?
            },
            orgs: match self.boards.orgs() {
                Some(orgs) => orgs
                    .of_subject(&subject.id)
                    .await
                    .map_err(Json)?
                    .into_iter()
                    .map(|(org, role)| OrgRole {
                        org: org.slug,
                        name: org.name,
                        role,
                    })
                    .collect(),
                None => Vec::new(),
            },
            owner: self.config.is_owner(&subject.id),
            // An AI connected through the hub's own tokens never has a person's standing.
            human: subject.via != "hub" && self.config.is_human(&subject.id),
            subject: subject.id,
            via: subject.via.to_string(),
            name: subject.name,
            org: subject.org,
            provisional_until: subject.provisional_until,
            agent: subject.agent.clone(),
            claim_code: subject.claim_code.clone(),
            provisional_warning: subject.provisional_warning,
            secure_url: (subject.via == "hub" || subject.via == "passkey")
                .then_some(subject.provisional_until)
                .flatten()
                .map(|_| {
                    format!(
                        "{}/account#secure",
                        self.config.public_url.trim_end_matches('/')
                    )
                }),
            tenant: subject.tenant.clone(),
            claim_url: subject.claim_code.as_ref().map(|code| {
                format!(
                    "{}/claim?code={code}",
                    self.config.public_url.trim_end_matches('/')
                )
            }),
        }))
    }

    #[tool(
        name = "machine_add",
        description = "Add one of the user's own machines to this account. Returns a one-time code and every install command: Linux (systemd), Windows (no administrator), Docker Compose, a Proxmox or Incus host, and a manual tarball. The code works once. The machine that enrolls belongs to this account. Do not use a name the hub operator already configured. If you can run commands on this computer, run the command for its OS yourself; give the user a command only for another machine.",
        title = "Add a machine",
        annotations(
            title = "Add a machine",
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn machine_add(
        &self,
        Parameters(input): Parameters<MachineAdd>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<MachineAdded>, Json<BridgeError>> {
        let subject = self.subject(&context)?;
        let name = input.name.trim().to_ascii_lowercase();
        let (device, code) = self.add_own_machine(&subject, &name).await.map_err(Json)?;
        let hub = self.config.public_url.trim_end_matches('/');
        Ok(Json(MachineAdded {
            name,
            code: code.clone(),
            expires_at: crate::machines::now_secs() + crate::machines::devices::CODE_TTL_SECS,
            linux: join_line(hub, "linux", &code),
            windows: join_line(hub, "windows", &code),
            docker: join_line(hub, "docker", &code),
            host: join_line(hub, "host", &code),
            manual: format!(
                "Download the archive and SHA256SUMS from {hub}/dist/ for this machine, check the sum, unpack, then: printf '%s\\n' '{code}' | devagotchi link enroll --state-dir ~/.local/state/devagotchi-link --hub {hub} --device {device} --broker <broker-endpoint>"
            ),
        }))
    }

    #[tool(
        name = "machine_update",
        description = "Update one of the user's machines to the hub's current build now. Waits for its running jobs to finish unless force is true; the machine is offline for about a minute.",
        title = "Update a machine",
        annotations(
            title = "Update a machine",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn machine_update(
        &self,
        Parameters(input): Parameters<MachineUpdate>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<MachineUpdated>, Json<BridgeError>> {
        let subject = self.subject(&context)?;
        self.update_machine(&subject, input).await.map(Json)
    }

    #[tool(
        name = "machines",
        description = "Start here for anything on the user's own machines (servers, PCs, devspaces): every machine you can work on through this hub and whether it can be used right now. status is online, offline (the machine is off, asleep or its link is not running: tell the user rather than retrying), not_enrolled or revoked. Pass a machine's name as `broker` to operation_start, operation_get, operation_read and operation_cancel.",
        title = "List my machines",
        annotations(
            title = "List my machines",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn machines(
        &self,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<Machines>, Json<BridgeError>> {
        let subject = self.subject(&context)?;
        Ok(Json(Machines {
            machines: self.machine_status(&subject).await,
        }))
    }
}
