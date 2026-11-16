//! What an API key may do: its scopes, and the one scope each tool needs. A tool with no
//! scope here is refused to every key, so a new tool is closed until it is given one.

use serde::{Deserialize, Serialize};

/// What a key may do. Each tool needs exactly one of these (`scope_for`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Scope {
    #[serde(rename = "machines.read")]
    MachinesRead,
    #[serde(rename = "machines.run")]
    MachinesRun,
    #[serde(rename = "machines.manage")]
    MachinesManage,
    #[serde(rename = "files.read")]
    FilesRead,
    #[serde(rename = "files.write")]
    FilesWrite,
    #[serde(rename = "boards.read")]
    BoardsRead,
    #[serde(rename = "boards.write")]
    BoardsWrite,
    #[serde(rename = "boards.admin")]
    BoardsAdmin,
    #[serde(rename = "approvals.request")]
    ApprovalsRequest,
}

impl Scope {
    pub const ALL: [Scope; 9] = [
        Scope::MachinesRead,
        Scope::MachinesRun,
        Scope::MachinesManage,
        Scope::FilesRead,
        Scope::FilesWrite,
        Scope::BoardsRead,
        Scope::BoardsWrite,
        Scope::BoardsAdmin,
        Scope::ApprovalsRequest,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Scope::MachinesRead => "machines.read",
            Scope::MachinesRun => "machines.run",
            Scope::MachinesManage => "machines.manage",
            Scope::FilesRead => "files.read",
            Scope::FilesWrite => "files.write",
            Scope::BoardsRead => "boards.read",
            Scope::BoardsWrite => "boards.write",
            Scope::BoardsAdmin => "boards.admin",
            Scope::ApprovalsRequest => "approvals.request",
        }
    }

    /// What it allows, as a person reads it.
    pub fn title(self) -> &'static str {
        match self {
            Scope::MachinesRead => "See machines and running jobs",
            Scope::MachinesRun => "Run commands on machines",
            Scope::MachinesManage => "Add and update machines, start setup links",
            Scope::FilesRead => "Read files on machines",
            Scope::FilesWrite => "Write and move files on machines",
            Scope::BoardsRead => "Read boards and notes",
            Scope::BoardsWrite => "Work tickets and leave notes",
            Scope::BoardsAdmin => "Change board settings and webhooks",
            Scope::ApprovalsRequest => "Ask you for approvals",
        }
    }

    pub fn parse(text: &str) -> Option<Scope> {
        Scope::ALL.into_iter().find(|scope| scope.as_str() == text)
    }
}

/// The scope a tool needs. `None` means no key may call it.
pub fn scope_for(tool: &str) -> Option<Scope> {
    Some(match tool {
        "machines" | "operation_get" | "operation_read" => Scope::MachinesRead,
        "operation_wait" | "operation_list" => Scope::MachinesRead,
        "get_config" | "list_processes" => Scope::MachinesRead,
        "get_recent_tool_calls" | "get_usage_stats" => Scope::MachinesRead,
        "run" | "operation_start" | "operation_cancel" => Scope::MachinesRun,
        "operation_input" | "operation_resize" => Scope::MachinesRun,
        "kill_process" | "read_url" => Scope::MachinesRun,
        "set_config_value" => Scope::MachinesManage,
        "machine_add" | "machine_update" | "workflow_list" | "workflow_start"
        | "workflow_status" | "workflow_wait" | "workflow_cancel" => Scope::MachinesManage,
        "read_file" | "read_multiple_files" | "read_image" => Scope::FilesRead,
        "list_directory" | "get_file_info" | "search_files" => Scope::FilesRead,
        "write_file" | "edit_file" | "create_directory" | "move_file" => Scope::FilesWrite,
        "approval_request" | "approval_wait" => Scope::ApprovalsRequest,
        "board_settings" | "board_wake" => Scope::BoardsAdmin,
        "board_attach" | "board_issue_context" | "board_prime" => Scope::BoardsRead,
        "board_ready" | "board_list" | "board_get" | "board_events" => Scope::BoardsRead,
        "board_wait" | "board_inbox" | "board_verify" => Scope::BoardsRead,
        tool if tool.starts_with("board_") => Scope::BoardsWrite,
        _ => return None,
    })
}

/// A tool every key may call: it says who the key is and touches nothing.
pub const ALWAYS: [&str; 1] = ["whoami"];
