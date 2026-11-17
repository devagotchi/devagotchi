//! When a machine was last seen, who hears that it went offline, and after how long.

use std::sync::Arc;

use crate::identity::preferences::wanting_machine_pushes;

use super::Links;

impl Links {
    /// When `device`'s link last went away since this hub started (it is online when
    /// `connected`).
    pub fn last_seen(&self, device: &str) -> Option<u64> {
        self.seen
            .lock()
            .expect("seen")
            .get(device)
            .map(|(at, _)| *at)
    }

    /// The accounts a machine's pushes go to: the account that added it; for a machine the
    /// config names, the hub's owner and every account granted it.
    pub(super) async fn machine_people(&self, device: &str) -> Vec<String> {
        if let Ok(Some(record)) = self.devices.get(device).await
            && let Some(owner) = record.owner
        {
            return vec![owner];
        }
        let mut people: Vec<String> = self.config.owner.iter().cloned().collect();
        for grant in &self.config.grants {
            if grant.brokers.iter().any(|b| b == device) && !people.contains(&grant.subject) {
                people.push(grant.subject.clone());
            }
        }
        people
    }

    /// The name a machine's person knows it by: on a hosted hub, without its tenant's tag
    /// (`tenancy/machines.rs`).
    pub(super) fn shown(&self, device: &str) -> String {
        if self.config.multi_tenant()
            && let Some((tag, name)) = device.split_once('-')
            && tag.len() == 13
            && tag.starts_with('t')
            && tag[1..].chars().all(|c| c.is_ascii_hexdigit())
            && !name.is_empty()
        {
            return name.to_string();
        }
        device.to_string()
    }

    /// Tell the phones of `device`'s account that it is `offline` or its `update_failed`;
    /// tapping it opens the machine's page in the phone app.
    pub(super) async fn push_machine(&self, device: &str, what: &str) {
        let site = self.config.site();
        let copy = &site.messages.app;
        let shown = self.shown(device);
        let (title, body) = match what {
            "offline" => (&copy.push_offline_title, &copy.push_offline_body),
            _ => (
                &copy.push_update_failed_title,
                &copy.push_update_failed_body,
            ),
        };
        let words = crate::site::Ctx::new(&site).with("machine", shown.clone());
        let payload = serde_json::json!({
            "title": words.plain(title),
            "body": words.plain(body),
            "url": format!("/app/machines/{shown}"),
            "tag": format!("devagotchi-machine-{what}-{device}"),
        });
        let people = self.machine_people(device).await;
        #[cfg(test)]
        self.pushed
            .lock()
            .expect("pushed")
            .push((people.clone(), payload.clone()));
        if let (Some(push), false) = (self.push.clone(), people.is_empty()) {
            let preferences = self.preferences.get().cloned();
            tokio::spawn(async move {
                let people = wanting_machine_pushes(preferences.as_ref(), people).await;
                if !people.is_empty() {
                    push.notify(&people, &payload).await;
                }
            });
        }
    }

    /// `device`'s link went away (the `episode`th drop): if it is still away, and not
    /// revoked, after [`OFFLINE_PUSH_AFTER`], its account's phones hear it once.
    pub(super) fn watch_offline(self: Arc<Self>, device: String, episode: u64) {
        tokio::spawn(async move {
            tokio::time::sleep(self.offline_after).await;
            let same = self
                .seen
                .lock()
                .expect("seen")
                .get(&device)
                .is_some_and(|(_, seen)| *seen == episode);
            if same
                && !self.connected(&device)
                && matches!(self.devices.is_revoked(&device).await, Ok(false))
            {
                self.push_machine(&device, "offline").await;
            }
        });
    }
}
