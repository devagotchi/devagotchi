//! Machine enrollment, updates and listing through the hub.
use devagotchi_contracts::BridgeError;
use rmcp::{
    Json, RoleServer, handler::server::wrapper::Parameters, service::RequestContext, tool,
    tool_router,
};

use crate::mcp::{
    Hub, join_line,
    schema::{MachineAdd, MachineAdded, MachineUpdate, MachineUpdated, Machines},
};

#[tool_router(router = manage_router, vis = "pub(crate)")]
impl Hub {
    #[tool(
        name = "machine_add",
        description = "Add one of the user's own machines to this account. Returns a one-time code and every install command: Linux (systemd), Windows (no administrator), Docker Compose, a Proxmox or Incus host, and a manual tarball. The code works once. The machine that enrolls belongs to this account. Do not use a name the hub operator already configured. If you can run commands on this computer, run the command for its OS yourself; give the user a command only for another machine.",
        title = "Add a machine",
        annotations(
            title = "Add a machine",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
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
        let result = async {
        let (device, code) = self.add_own_machine(&subject, &name).await?;
        let hub = self.config.public_url.trim_end_matches('/');
        Ok(MachineAdded {
            name: name.clone(),
            code: code.clone(),
            expires_at: crate::machines::now_secs() + crate::machines::devices::CODE_TTL_SECS,
            linux: join_line(hub, "linux", &code),
            windows: join_line(hub, "windows", &code),
            docker: join_line(hub, "docker", &code),
            host: join_line(hub, "host", &code),
            manual: format!(
                "Download the archive and SHA256SUMS from {hub}/dist/ for this machine, check the sum, unpack, then: printf '%s\\n' '{code}' | devagotchi link enroll --state-dir ~/.local/state/devagotchi-link --hub {hub} --device {device} --broker <broker-endpoint>"
            ),
        }) }.await;
        self.recorded(&subject, "machine_add", "", &name, result)
            .await
    }

    #[tool(
        name = "machine_update",
        description = "Update one of the user's machines to the hub's current build now. Waits for its running jobs to finish unless force is true; the machine is offline for about a minute.",
        title = "Update a machine",
        annotations(
            title = "Update a machine",
            read_only_hint = false,
            destructive_hint = true,
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
        let machine = input.machine.clone();
        let result = self
            .update_machine(&subject, input)
            .await
            .map_err(|Json(e)| e);
        self.recorded(&subject, "machine_update", &machine, "", result)
            .await
    }

    #[tool(
        name = "machines",
        description = "Start here for anything on the user's own machines (servers, PCs, devspaces): every machine you can work on through this hub and its status. status is online (its link is connected; that its broker answers too, machine_inspect asks within 3 seconds), offline (the machine is off, asleep or its link is not running: tell the user rather than retrying), not_enrolled or revoked. Pass a machine's name as `machine` to operation_start, operation_get, operation_read and operation_cancel.",
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
        let result = Ok(Machines {
            machines: self.machine_status(&subject).await,
        });
        self.recorded(&subject, "machines", "", "", result).await
    }
}
