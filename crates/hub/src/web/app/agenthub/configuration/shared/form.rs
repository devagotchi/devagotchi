//! Strict forms for publication, selection, detachment and deletion.
use super::super::form::Fields;
use crate::agenthub::{Actor, Board};
use devagotchi_contracts::BridgeResult;

#[derive(Clone)]
pub(super) enum Change {
    Publish {
        board: i64,
        id: String,
        name: String,
    },
    Bind {
        board: i64,
        selection: String,
    },
    Detach {
        board: i64,
    },
    Delete {
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
        let change = match f.take("action")?.as_str() {
            "publish" => Self::Publish {
                board: positive(&mut f, "board_version")?,
                id: f.take("id")?,
                name: f.take("name")?,
            },
            "bind" => Self::Bind {
                board: positive(&mut f, "board_version")?,
                selection: f.take("selection")?,
            },
            "detach" => Self::Detach {
                board: positive(&mut f, "board_version")?,
            },
            "delete" => Self::Delete {
                id: f.take("id")?,
                version: positive(&mut f, "version")?,
            },
            _ => return Err(()),
        };
        f.finish()?;
        Ok(change)
    }
    pub fn apply(&self, board: &Board, me: &str) -> BridgeResult<()> {
        let actor = Actor::human(me);
        match self {
            Self::Publish {
                board: version,
                id,
                name,
            } => board
                .publish_workflow(&actor, *version, id, name)
                .map(|_| ()),
            Self::Bind {
                board: version,
                selection,
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
            Self::Detach { board: version } => board.detach_workflow(&actor, *version).map(|_| ()),
            Self::Delete { id, version } => board.delete_shared_workflow(&actor, id, *version),
        }
    }
}
