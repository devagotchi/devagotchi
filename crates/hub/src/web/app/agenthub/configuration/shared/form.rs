//! Strict forms for publication, selection, detachment and deletion.
use super::super::form::Fields;
use crate::agenthub::{Actor, Board};
use devagotchi_contracts::BridgeResult;

#[derive(Clone, serde::Serialize)]
pub(super) enum Change {
    Publish {
        request_id: Option<String>,
        board: i64,
        id: String,
        name: String,
    },
    Bind {
        request_id: Option<String>,
        board: i64,
        selection: String,
    },
    Detach {
        request_id: Option<String>,
        board: i64,
    },
    Delete {
        request_id: Option<String>,
        id: String,
        version: i64,
    },
}
fn positive(fields: &mut Fields, name: &str) -> Result<i64, ()> {
    let value = fields.number(name)?;
    if value > 0 { Ok(value) } else { Err(()) }
}
impl Change {
    pub fn parse(pairs: Vec<(String, String)>) -> Result<Self, ()> {
        let mut f = Fields::new(pairs)?;
        let request_id = f.optional("request_id");
        let change = match f.take("action")?.as_str() {
            "publish" => Self::Publish {
                request_id: request_id.clone(),
                board: positive(&mut f, "board_version")?,
                id: f.take("id")?,
                name: f.take("name")?,
            },
            "bind" => Self::Bind {
                request_id: request_id.clone(),
                board: positive(&mut f, "board_version")?,
                selection: f.take("selection")?,
            },
            "detach" => Self::Detach {
                request_id: request_id.clone(),
                board: positive(&mut f, "board_version")?,
            },
            "delete" => Self::Delete {
                request_id: request_id.clone(),
                id: f.take("id")?,
                version: positive(&mut f, "version")?,
            },
            _ => return Err(()),
        };
        f.finish()?;
        Ok(change)
    }
    pub fn action(&self) -> &'static str {
        match self {
            Self::Publish { .. } => "publish",
            Self::Bind { .. } => "bind",
            Self::Detach { .. } => "detach",
            Self::Delete { .. } => "delete",
        }
    }
    pub fn request_id(&self) -> Option<&str> {
        match self {
            Self::Publish { request_id, .. }
            | Self::Bind { request_id, .. }
            | Self::Detach { request_id, .. }
            | Self::Delete { request_id, .. } => request_id.as_deref(),
        }
    }
    pub fn apply(&self, board: &Board, me: &str) -> BridgeResult<bool> {
        let operation = match self {
            Self::Publish { .. } => "publish_workflow",
            Self::Bind { .. } => "bind_workflow",
            Self::Detach { .. } => "detach_workflow",
            Self::Delete { .. } => "delete_shared_workflow",
        };
        let requested = self
            .request_id()
            .map(|key| board.requested(me, key, operation, self))
            .transpose()?;
        let board = requested.as_ref().unwrap_or(board);
        let actor = Actor::human(me);
        let result = match self {
            Self::Publish {
                board: version,
                id,
                name,
                ..
            } => board
                .publish_workflow(&actor, *version, id, name)
                .map(|_| ()),
            Self::Bind {
                board: version,
                selection,
                ..
            } => {
                let (id, selected) = selection
                    .rsplit_once(':')
                    .and_then(|(id, version)| {
                        version
                            .parse::<i64>()
                            .ok()
                            .filter(|v| *v > 0)
                            .map(|v| (id, v))
                    })
                    .ok_or_else(|| {
                        devagotchi_contracts::BridgeError::new(
                            "invalid",
                            "choose a shared workflow",
                        )
                    })?;
                board
                    .bind_workflow(&actor, *version, id, selected)
                    .map(|_| ())
            }
            Self::Detach { board: version, .. } => {
                board.detach_workflow(&actor, *version).map(|_| ())
            }
            Self::Delete { id, version, .. } => board.delete_shared_workflow(&actor, id, *version),
        };
        result.map(|_| board.hosted().replayed())
    }
}
