//! A project's standalone board on disk (ADR 0002): `<project>/.devagotchi/board.sqlite3`,
//! with `config.json` beside it naming the project, the prefix its ids carry, and the hub it is
//! bound to, if any.

use std::path::Path;

use devagotchi_contracts::{BridgeError, BridgeResult};
use serde::{Deserialize, Serialize};

use crate::Board;

pub const BOARD_DIR: &str = ".devagotchi";
pub const BOARD_FILE: &str = "board.sqlite3";

/// `prefix-xxxx` from the project name: lowercase letters and digits, at most four.
pub fn default_prefix(project: &str) -> String {
    let prefix: String = project
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .take(4)
        .collect();
    if prefix.is_empty() {
        "tk".into()
    } else {
        prefix
    }
}

/// What `board init` writes beside the store, so every face of the board (command
/// line here, the hub's tools) opens it under the name its ids were minted with.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub project: String,
    pub prefix: String,
    pub created_at: i64,
    /// The hub this project is bound to (B11): with an account on file, every board
    /// command runs there as the account; absent, the board is the local store.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hub: Option<String>,
}

pub fn read_project_config(board_dir: &Path) -> BridgeResult<ProjectConfig> {
    let path = board_dir.join("config.json");
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| BridgeError::new("no_board", format!("{}: {e}", path.display())))?;
    serde_json::from_str(&raw)
        .map_err(|e| BridgeError::new("board_io", format!("config.json: {e}")))
}

/// Open an initialised project's board: `<project_dir>/.devagotchi/board.sqlite3` under
/// the project name recorded there. An uninitialised directory is `no_board`, never
/// silently created: `board init` is the deliberate act that also writes the ignore
/// file and the AGENTS.md block.
pub fn open_project(project_dir: &Path) -> BridgeResult<(ProjectConfig, Board)> {
    let board_dir = project_dir.join(BOARD_DIR);
    if !board_dir.join(BOARD_FILE).is_file() {
        return Err(BridgeError::new(
            "no_board",
            format!(
                "no board under {}; run `devagotchi board init --dir {}` there",
                project_dir.display(),
                project_dir.display()
            ),
        ));
    }
    let config = read_project_config(&board_dir)?;
    let board = Board::open(&board_dir.join(BOARD_FILE), &config.project)?;
    Ok((config, board))
}
