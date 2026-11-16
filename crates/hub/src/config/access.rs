//! `brokers`, `boards`, `grants`, `account_boards`: who may reach which broker and which
//! board, and the boards the hub hosts for accounts. No grant, no route; the owner reaches
//! everything.

use std::{collections::BTreeSet, path::Path};

use devagotchi_contracts::{BridgeError, BridgeResult};
use serde::Deserialize;

use super::{HubConfig, valid_name, valid_subject};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrokerEntry {
    /// Stable name callers use in `broker` fields; `[a-z0-9][a-z0-9-]{0,62}`. For a
    /// linked broker it is also the device name the machine enrolls under.
    pub name: String,
    /// A broker on this host: the absolute endpoint on its ready line (Unix socket path
    /// or named pipe). Exactly one of `socket` and `link`.
    #[serde(default)]
    pub socket: Option<String>,
    /// A broker on another machine, reached over the outbound link it dials to `/link`.
    #[serde(default)]
    pub link: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grant {
    /// The subject as the identity provider names it (`sub`), or the dev token's subject.
    pub subject: String,
    /// Broker names this subject may route to.
    #[serde(default)]
    pub brokers: Vec<String>,
    /// Project boards this subject may work on.
    #[serde(default)]
    pub boards: Vec<String>,
    /// The subject is a person, not an agent: on a board they may force-release a ticket
    /// and move one by hand. The owner is always a person; agents never are.
    #[serde(default)]
    pub human: bool,
    /// This subject may read and change hub operator settings (`/api/v1/admin/settings`).
    /// The owner can always do that; this flag is how a control-plane account can too.
    #[serde(default)]
    pub operator: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoardEntry {
    /// The project name callers pass as `project`; `[a-z0-9][a-z0-9-]{0,62}`. The board is in
    /// the hub's Postgres, owned by `@shared`, and made on its first use; the grants that
    /// name it say who works it.
    pub project: String,
}

/// The hosted tier's boards: an account binds a project and gets a board of its own on this
/// hub, reached by name like a configured one; its seats (`X-Devagotchi-Seat`) are the
/// account's own agent names on it.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountBoardsConfig {
    /// How many projects a free account may bind here (D21: realistically one). The owner
    /// and the `unlimited` accounts have no limit.
    #[serde(default)]
    pub free_limit: Option<u32>,
    /// Account IDs with no limit (paid, or friends).
    #[serde(default)]
    pub unlimited: Vec<String>,
}

/// What a free account gets when the config does not say.
pub const DEFAULT_FREE_LIMIT: u32 = 1;

impl HubConfig {
    /// At most 256 brokers and boards, each named once; grants name only those.
    pub(super) fn check_access(&self) -> BridgeResult<()> {
        let invalid = |message: String| BridgeError::new("invalid_config", message);
        // Both lists may be empty: a hub that serves nothing yet still answers whoami and
        // /healthz, so an installer can bring it up before its first board or machine.
        if self.brokers.len() > 256 || self.boards.len() > 256 {
            return Err(invalid("at most 256 brokers and 256 boards".into()));
        }
        let mut names = BTreeSet::new();
        for broker in &self.brokers {
            if !valid_name(&broker.name) {
                return Err(invalid(format!(
                    "broker name {:?} is not [a-z0-9][a-z0-9-]{{0,62}}",
                    broker.name
                )));
            }
            if !names.insert(broker.name.as_str()) {
                return Err(invalid(format!(
                    "broker name {:?} is listed twice",
                    broker.name
                )));
            }
            match (&broker.socket, broker.link) {
                (Some(socket), false) if Path::new(socket).is_absolute() => {}
                (Some(_), false) => {
                    return Err(invalid(format!(
                        "broker {:?}: socket must be an absolute path",
                        broker.name
                    )));
                }
                _ => {
                    if broker.socket.is_some() || !broker.link {
                        return Err(invalid(format!(
                            "broker {:?}: give exactly one of socket or \"link\": true",
                            broker.name
                        )));
                    }
                }
            }
        }
        let mut projects = BTreeSet::new();
        for board in &self.boards {
            if !valid_name(&board.project) {
                return Err(invalid(format!(
                    "board project {:?} is not [a-z0-9][a-z0-9-]{{0,62}}",
                    board.project
                )));
            }
            if !projects.insert(board.project.as_str()) {
                return Err(invalid(format!(
                    "board project {:?} is listed twice",
                    board.project
                )));
            }
        }
        if let Some(accounts) = &self.account_boards {
            for subject in &accounts.unlimited {
                valid_subject(subject)
                    .map_err(|why| invalid(format!("account_boards.unlimited {why}")))?;
            }
        }
        if self.grants.len() > 4096 {
            return Err(invalid("at most 4096 grants".into()));
        }
        for grant in &self.grants {
            valid_subject(&grant.subject).map_err(|why| invalid(format!("grant subject {why}")))?;
            for name in &grant.brokers {
                if !names.contains(name.as_str()) {
                    return Err(invalid(format!(
                        "grant for {:?} names unknown broker {:?}",
                        grant.subject, name
                    )));
                }
            }
            for project in &grant.boards {
                if !projects.contains(project.as_str()) {
                    return Err(invalid(format!(
                        "grant for {:?} names unknown board {:?}",
                        grant.subject, project
                    )));
                }
            }
        }
        Ok(())
    }

    /// Owner, or a grant marked `operator`. Those accounts may call the admin settings API.
    pub fn is_operator(&self, subject: &str) -> bool {
        self.is_owner(subject)
            || self
                .grants
                .iter()
                .any(|g| g.subject == subject && g.operator)
    }

    /// The people who decide approvals on `project`: the owner and every grant that is
    /// marked human and names the board.
    pub fn people_for_board(&self, project: &str) -> Vec<String> {
        let mut people: Vec<String> = self.owner_id().map(str::to_string).into_iter().collect();
        for grant in &self.grants {
            if grant.human
                && grant.boards.iter().any(|b| b == project)
                && !people.contains(&grant.subject)
            {
                people.push(grant.subject.clone());
            }
        }
        people
    }

    /// Whether this subject acts as a person on boards: the owner, or a grant that says so.
    pub fn is_human(&self, subject: &str) -> bool {
        self.is_owner(subject) || self.grants.iter().any(|g| g.subject == subject && g.human)
    }

    /// Boards the subject may work on, in config order. The owner reaches every board.
    pub fn boards_for(&self, subject: &str) -> Vec<&BoardEntry> {
        if self.is_owner(subject) {
            return self.boards.iter().collect();
        }
        let mut allowed = BTreeSet::new();
        for grant in self.grants.iter().filter(|g| g.subject == subject) {
            allowed.extend(grant.boards.iter().map(String::as_str));
        }
        self.boards
            .iter()
            .filter(|b| allowed.contains(b.project.as_str()))
            .collect()
    }

    pub fn board(&self, project: &str) -> Option<&BoardEntry> {
        self.boards.iter().find(|b| b.project == project)
    }

    /// Broker names the subject may route to, in config order, without duplicates.
    /// The owner reaches every broker.
    pub fn brokers_for(&self, subject: &str) -> Vec<&BrokerEntry> {
        if self.is_owner(subject) {
            return self.brokers.iter().collect();
        }
        let mut allowed = BTreeSet::new();
        for grant in self.grants.iter().filter(|g| g.subject == subject) {
            allowed.extend(grant.brokers.iter().map(String::as_str));
        }
        self.brokers
            .iter()
            .filter(|b| allowed.contains(b.name.as_str()))
            .collect()
    }

    pub fn broker(&self, name: &str) -> Option<&BrokerEntry> {
        self.brokers.iter().find(|b| b.name == name)
    }
}
