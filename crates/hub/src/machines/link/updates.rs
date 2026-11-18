//! Updating a machine's build: what it runs, asking it to update, waiting on the report, and the hourly auto-update.

use std::{sync::Arc, time::Duration};

use devagotchi_contracts::{BridgeError, BridgeResult, HubFrame};

use crate::machines::{now_secs, update::Update};

use super::{Links, UpdateAsked, offline};

impl Links {
    /// The build a connected machine's hello said it runs.
    pub fn build(&self, device: &str) -> Option<String> {
        let sessions = self.sessions.lock().expect("links");
        sessions
            .get(device)
            .and_then(|session| session.build.clone())
    }

    /// A machine's latest update, if it had one since this hub started.
    pub fn update_of(&self, device: &str) -> Option<Update> {
        self.updates.lock().expect("updates").get(device).cloned()
    }

    /// Ask a connected machine to update to the build this hub hands out, now; with
    /// `force`, without waiting for its running jobs. `subject` and `via` are who asked
    /// (`hub`, `auto` for the hourly round), for the audit.
    pub async fn request_update(
        &self,
        device: &str,
        force: bool,
        subject: &str,
        via: &str,
    ) -> BridgeResult<UpdateAsked> {
        let asked = self.ask_update(device, force);
        let (version, outcome) = match &asked {
            Ok(UpdateAsked::Sent) => (self.offered.clone(), format!("sent force={force}")),
            Ok(UpdateAsked::AlreadyOn(build)) => (Some(build.clone()), "already_on".into()),
            Err(error) => (self.offered.clone(), error.code.clone()),
        };
        let version = version.unwrap_or_default();
        self.event(subject, via, "machine.update", device, &version, &outcome)
            .await;
        asked
    }

    pub(super) fn ask_update(&self, device: &str, force: bool) -> BridgeResult<UpdateAsked> {
        let version = self.offered.clone().ok_or_else(|| {
            BridgeError::new(
                "no_build",
                "this hub hands out no builds (no downloads), so it has none to update to",
            )
        })?;
        let session = self
            .sessions
            .lock()
            .expect("links")
            .get(device)
            .cloned()
            .ok_or_else(|| offline(device))?;
        let Some(build) = session.build else {
            return Err(BridgeError::new(
                "update_unsupported",
                format!(
                    "{device} runs a link from before machines could update themselves; install it once more by hand"
                ),
            ));
        };
        if build == version {
            return Ok(UpdateAsked::AlreadyOn(build));
        }
        session
            .control
            .try_send(HubFrame::Update {
                version: version.clone(),
                force,
            })
            .map_err(|_| offline(device))?;
        let mut updates = self.updates.lock().expect("updates");
        let now = now_secs();
        if !updates
            .get(device)
            .is_some_and(|u| u.running(now) && u.target == version)
        {
            updates.insert(device.to_string(), Update::new(&version, now));
        }
        drop(updates);
        self.update_changed.notify_waiters();
        Ok(UpdateAsked::Sent)
    }

    /// Wait up to `within` for `device`'s update to finish; its latest state either way.
    pub async fn wait_update(&self, device: &str, within: Duration) -> Option<Update> {
        let deadline = tokio::time::Instant::now() + within;
        loop {
            let changed = self.update_changed.notified();
            let update = self.update_of(device);
            if update.as_ref().is_none_or(|u| !u.running(now_secs())) {
                return update;
            }
            if tokio::time::timeout_at(deadline, changed).await.is_err() {
                return self.update_of(device);
            }
        }
    }

    /// The link reported a step of its update.
    pub(super) async fn update_reported(&self, device: &str, state: &str, detail: Option<String>) {
        if !devagotchi_contracts::UPDATE_STATES.contains(&state) {
            return;
        }
        let (target, newly_failed) = {
            let mut updates = self.updates.lock().expect("updates");
            let entry = updates
                .entry(device.to_string())
                .or_insert_with(|| Update::new("", now_secs()));
            let newly_failed = state == "failed" && entry.state != "failed";
            entry.state = state.to_string();
            entry.detail = detail.clone().map(|d| d.chars().take(300).collect());
            (entry.target.clone(), newly_failed)
        };
        // Once per failed update: its account's phones hear it.
        if newly_failed {
            self.push_machine(device, "update_failed").await;
        }
        if matches!(state, "done" | "failed") {
            let outcome = match &detail {
                Some(detail) => format!("{state}: {detail}"),
                None => state.to_string(),
            };
            let subject = format!("device:{device}");
            self.event(
                &subject,
                "link",
                "machine.update_status",
                device,
                &target,
                &outcome,
            )
            .await;
        }
        self.update_changed.notify_waiters();
    }

    /// A machine connected on `build`: an update that brought it there is done.
    pub(super) async fn connected_on(&self, device: &str, build: Option<&str>) {
        let done = {
            let updates = self.updates.lock().expect("updates");
            updates
                .get(device)
                .is_some_and(|u| u.running(now_secs()) && Some(u.target.as_str()) == build)
        };
        if done {
            self.update_reported(device, "done", None).await;
        }
    }

    /// Ask each connected machine that is due an update for one (`machines::update::select`),
    /// when `machines.auto_update` is on and this hub hands out a build.
    pub async fn auto_update_now(&self) {
        let (true, Some(version)) = (self.config.live().auto_update, self.offered.as_deref())
        else {
            return;
        };
        let live: Vec<(String, Option<String>)> = {
            let sessions = self.sessions.lock().expect("links");
            sessions
                .iter()
                .map(|(name, s)| (name.clone(), s.build.clone()))
                .collect()
        };
        let chosen = {
            let updates = self.updates.lock().expect("updates");
            crate::machines::update::select(&live, version, &updates, now_secs())
        };
        for device in chosen {
            let _ = self.request_update(&device, false, "hub", "auto").await;
        }
    }

    /// Once an hour, for as long as the hub runs: [`Self::auto_update_now`].
    pub async fn auto_update_hourly(self: Arc<Self>) {
        let mut every = tokio::time::interval(crate::machines::update::EVERY);
        loop {
            every.tick().await;
            self.auto_update_now().await;
        }
    }
}
