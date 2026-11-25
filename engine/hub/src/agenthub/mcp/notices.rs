//! What an agent hears on its next call: waking a recipient, and the banner a tool result carries when notes wait.

use std::sync::Arc;

use crate::{
    agenthub::{Actor, Board},
    identity::Subject,
    mcp::Hub,
};

/// How often one caller's inbox is looked at, however fast it calls tools.
#[cfg(not(test))]
const INBOX_CHECK_GAP: std::time::Duration = std::time::Duration::from_secs(10);
#[cfg(test)]
const INBOX_CHECK_GAP: std::time::Duration = std::time::Duration::ZERO;
/// Boards one check looks at.
const INBOX_BOARDS: usize = 25;

impl Hub {
    /// A note was left for `to`: call its webhook, at most once a window.
    pub(crate) fn wake_recipient(&self, project: &str, board: Arc<Board>, to: &str) {
        use crate::agenthub::wake::{self, Admit};
        let allow_private = !self.config.multi_tenant();
        let limits = self.boards.wake_limits.clone();
        let key = format!("{project}\0{to}");
        let delay = match limits.admit(&key, std::time::Instant::now(), wake::WINDOW) {
            Admit::Now => None,
            Admit::After(delay) => Some(delay),
            Admit::Coalesced => return,
        };
        let (project, to) = (project.to_string(), to.to_string());
        tokio::spawn(async move {
            if let Some(delay) = delay {
                tokio::time::sleep(delay).await;
                limits.fired(&key, std::time::Instant::now(), wake::WINDOW);
            }
            wake::flush(board, project, to, allow_private).await;
        });
    }

    /// What this caller is owed from its notes, as one block of text for the answer to the
    /// tool it just called; nothing when it has nothing new. Looked at no more often than
    /// `INBOX_CHECK_GAP` per caller.
    pub(crate) async fn inbox_banner(
        &self,
        subject: &Subject,
        seat: Option<&str>,
    ) -> Option<String> {
        let key = subject.agent.clone().unwrap_or_else(|| {
            format!(
                "{}\0{}\0{}",
                subject.id,
                seat.unwrap_or(""),
                subject.client.as_deref().unwrap_or("")
            )
        });
        {
            let mut checks = self.boards.inbox_checks.lock().expect("inbox checks");
            let now = std::time::Instant::now();
            if checks
                .get(&key)
                .is_some_and(|last| now.duration_since(*last) < INBOX_CHECK_GAP)
            {
                return None;
            }
            checks.insert(key, now);
        }
        let projects = self.board_names(subject).ok()?;
        let aliases = if let Some(principal) = &subject.agent {
            vec![principal.clone()]
        } else {
            subject
                .client
                .as_deref()
                .map(crate::agenthub::inbox::client_aliases)
                .unwrap_or_default()
        };
        let mut boards = Vec::new();
        for project in projects.into_iter().take(INBOX_BOARDS) {
            if let Ok((board, kind, role)) = self.board_access(subject, &project) {
                let (id, _) = self.actor_for(subject, seat, kind, role);
                boards.push((project, board, id));
            }
        }
        tokio::task::spawn_blocking(move || {
            let mut heard = Vec::new();
            for (project, board, id) in boards {
                let mut who = aliases.clone();
                who.push(id.clone());
                let actor = Actor {
                    id: &id,
                    human: false,
                    policy: None,
                    wip_limit: None,
                };
                match board.inbox_deliver(&actor, &who, &id, true) {
                    Ok(delivery) if !delivery.is_quiet() => heard.push((project, delivery)),
                    Ok(_) => {}
                    Err(error) => eprintln!("devagotchi hub: inbox on {project}: {error}"),
                }
            }
            crate::agenthub::inbox::banner(&heard)
        })
        .await
        .ok()
        .flatten()
    }
}
