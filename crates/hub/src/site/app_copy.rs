//! Copy for the application and its board creation forms.
use super::model::DelegateCopy;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppCopy {
    pub access: super::access_copy::AccessCopy,
    pub tab_home: String,
    pub tab_machines: String,
    pub tab_boards: String,
    pub tab_approvals: String,
    /// The install banner: on iPhone the gesture, on Android the install prompt's button.
    pub install_iphone: String,
    pub install_android: String,
    pub install_button: String,
    pub home_title: String,
    pub needs_you: String,
    pub needs_you_empty: String,
    /// What each line under Needs you is.
    pub approval_waiting: String,
    pub assigned_to_you: String,
    pub update_failed: String,
    pub machines_title: String,
    pub machines_empty: String,
    pub online: String,
    pub offline: String,
    pub revoked: String,
    pub not_enrolled: String,
    pub updating: String,
    /// `{build}`: the build a machine runs.
    pub build: String,
    pub boards_title: String,
    pub boards_empty: String,
    /// `{n}`: how many tickets are open.
    pub open_count: String,
    pub offline_title: String,
    pub offline_text: String,
    /// The machine page (`web/app/machine.rs`).
    pub machine_status: String,
    pub machine_build: String,
    pub machine_build_unknown: String,
    pub last_seen: String,
    pub seen_now: String,
    pub seen_never: String,
    pub update_now: String,
    /// `{state}`: where the update is (`done`, `failed`, `requested`, …).
    pub update_state: String,
    pub jobs_title: String,
    pub jobs_empty: String,
    pub job_when: String,
    pub job_command: String,
    pub job_state: String,
    pub job_exit: String,
    pub machine_unknown: String,
    pub machine_unknown_text: String,
    /// A form posted from another site.
    pub not_from_here: String,
    /// The board and ticket pages (`web/app/agenthub/board.rs`, `web/app/agenthub/ticket.rs`). `not_changed`:
    /// `{why}`, the board's answer.
    pub filter_state: String,
    pub filter_assignee: String,
    pub filter_label: String,
    /// The anchor chips on the board page, and the ticket page's one line of them.
    pub filter_machine: String,
    pub filter_environment: String,
    pub filter_repo: String,
    pub filter_agent: String,
    pub ticket_anchors: String,
    pub search_label: String,
    pub search_button: String,
    pub tickets_empty: String,
    pub board_view: BoardViewCopy,
    pub you: String,
    pub nobody: String,
    pub ticket_state: String,
    pub ticket_priority: String,
    pub ticket_labels: String,
    pub ticket_assignee: String,
    pub ticket_holder: String,
    pub ticket_progress: String,
    pub issue_source: String,
    pub issue_source_state: String,
    pub issue_source_revision: String,
    pub issue_finalization: String,
    pub issue_finalization_pending: String,
    pub issue_finalization_reconcile: String,
    pub issue_finalization_none: String,
    pub notes_title: String,
    pub notes_empty: String,
    pub events_title: String,
    pub comment_label: String,
    pub comment_button: String,
    pub assign_me: String,
    pub close: String,
    pub undo: String,
    pub hand_to_ai: String,
    pub done_comment: String,
    pub done_assign: String,
    pub done_close: String,
    pub done_undo: String,
    pub done_hand: String,
    pub undo_late: String,
    /// The ticket page's "Assign to an agent" (`web/app/agenthub/ticket.rs`).
    #[serde(default)]
    pub delegate: DelegateCopy,
    pub not_changed: String,
    pub board_unknown: String,
    pub board_unknown_text: String,
    pub ticket_unknown: String,
    pub ticket_unknown_text: String,
    /// Pushes about a machine (`link.rs`): `{machine}` is its name.
    pub push_offline_title: String,
    pub push_offline_body: String,
    pub push_update_failed_title: String,
    pub push_update_failed_body: String,
    pub creation: CreationCopy,
    pub audit: AuditCopy,
    pub hub_audit: HubAuditCopy,
    pub people: PeopleCopy,
    pub agents: super::agents_copy::AgentsCopy,
    pub configuration: super::configuration_copy::ConfigurationCopy,
    pub projects: super::projects_copy::ProjectsCopy,
    pub catalog: super::catalog_copy::CatalogCopy,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoardViewCopy {
    pub summary: String,
    pub held_by_me: String,
    pub column_empty: String,
    pub ready: String,
    pub blocked_by: String,
    pub reserved_for: String,
    pub last_worker: String,
    pub lease_expired: String,
    pub lease_remaining: String,
    pub expiry_count: String,
    pub snapshot: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditCopy {
    pub title: String,
    pub description: String,
    pub actor: String,
    pub action: String,
    pub ticket: String,
    pub filter: String,
    pub refresh: String,
    pub verify: String,
    pub verified: String,
    pub broken: String,
    pub unverified: String,
    pub details: String,
    pub request: String,
    pub hash: String,
    pub empty: String,
    pub older: String,
    pub updated: String,
    pub unavailable: String,
    pub invalid_cursor: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreationCopy {
    pub board: String,
    pub board_name: String,
    pub board_help: String,
    pub task: String,
    pub title: String,
    pub description: String,
    pub priority: String,
    pub labels: String,
    pub labels_help: String,
    pub dependencies: String,
    pub dependencies_help: String,
    pub waiting: String,
    pub unblocked: String,
    pub create: String,
    pub cancel: String,
    pub refused: String,
    pub read_only: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HubAuditCopy {
    pub title: String,
    pub description: String,
    pub back: String,
    pub machine: String,
    pub outcome: String,
    pub via: String,
    pub operation: String,
    pub broken: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeopleCopy {
    pub access_help: String,
    pub title: String,
    pub description: String,
    pub manage: String,
    pub add: String,
    pub name: String,
    pub email: String,
    pub password: String,
    pub password_help: String,
    pub create: String,
    pub save: String,
    pub enabled: String,
    pub active: String,
    pub disabled: String,
    pub owner: String,
    pub user: String,
    pub back: String,
    pub settings: String,
    pub more: String,
    pub reset: String,
    pub reset_help: String,
    pub external: String,
    pub empty: String,
    pub created: String,
    pub updated: String,
    pub reset_done: String,
    pub refused: String,
    pub edit: String,
}
