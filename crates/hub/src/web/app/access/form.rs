use super::*;
use std::collections::BTreeMap;
pub(super) struct Draft {
    pub request_id: String,
    pub version: String,
    pub action: String,
    pub duration: String,
    pub uses_max: String,
    pub level: String,
    pub paths: String,
    pub paths_mode: String,
    pub note: String,
}
impl Draft {
    pub fn new(review: &AccessReview) -> Self {
        Self {
            request_id: requests::request_id(),
            version: review.version.to_string(),
            action: "approve".into(),
            duration: String::new(),
            uses_max: String::new(),
            level: serde_json::to_value(review.terms.item.sandbox_level)
                .expect("enum")
                .as_str()
                .unwrap()
                .into(),
            paths: review
                .terms
                .item
                .selector
                .paths
                .as_ref()
                .map(|p| p.join("\n"))
                .unwrap_or_default(),
            paths_mode: "keep".into(),
            note: String::new(),
        }
    }
    pub fn submitted(fields: Vec<(String, String)>) -> (Self, Option<BridgeError>) {
        let mut values = BTreeMap::new();
        let mut error = None;
        for (key, value) in fields {
            if ![
                "request_id",
                "version",
                "action",
                "duration",
                "uses_max",
                "level",
                "paths",
                "paths_mode",
                "note",
            ]
            .contains(&key.as_str())
                || value.len() > 16384
                || values.contains_key(&key)
            {
                error = Some(invalid("unknown, repeated or oversized form field"));
                continue;
            }
            values.insert(key, value);
        }
        let mut take = |key: &str| values.remove(key).unwrap_or_default();
        (
            Self {
                request_id: take("request_id"),
                version: take("version"),
                action: take("action"),
                duration: take("duration"),
                uses_max: take("uses_max"),
                level: take("level"),
                paths: take("paths"),
                paths_mode: take("paths_mode"),
                note: take("note"),
            },
            error,
        )
    }
    pub fn version(&self) -> BridgeResult<i64> {
        self.version
            .parse::<i64>()
            .ok()
            .filter(|v| *v > 0)
            .ok_or_else(|| invalid("reload the request to obtain its current version"))
    }
    pub fn decision(&self, terms: &crate::machines::access::GrantTerms) -> BridgeResult<Decision> {
        match self.action.as_str() {
            "approve" => Ok(Decision::Approve {
                narrow: Narrow {
                    note: self.note.clone(),
                    ..Default::default()
                },
            }),
            "deny" => Ok(Decision::Deny {
                note: self.note.clone(),
            }),
            "narrow" => {
                let mut item = terms.item.clone();
                item.sandbox_level = serde_json::from_value(serde_json::json!(self.level))
                    .map_err(|_| invalid("choose a valid sandbox level"))?;
                match self.paths_mode.as_str() {
                    "keep" => {}
                    "restrict" => {
                        item.selector.paths = Some(
                            self.paths
                                .lines()
                                .map(str::trim)
                                .filter(|p| !p.is_empty())
                                .map(str::to_owned)
                                .collect(),
                        )
                    }
                    _ => {
                        return Err(invalid(
                            "choose whether to keep or restrict the requested paths",
                        ));
                    }
                }
                let duration = if self.duration.is_empty() {
                    None
                } else {
                    Some(
                        GrantDuration::try_from(self.duration.clone())
                            .map_err(|_| invalid("choose a valid duration"))?,
                    )
                };
                let uses_max = if self.uses_max.is_empty() {
                    None
                } else {
                    Some(
                        self.uses_max
                            .parse::<u32>()
                            .ok()
                            .filter(|v| *v > 0)
                            .ok_or_else(|| invalid("use limit must be a positive number"))?,
                    )
                };
                Ok(Decision::Approve {
                    narrow: Narrow {
                        item: Some(item),
                        duration,
                        uses_max,
                        note: self.note.clone(),
                    },
                })
            }
            _ => Err(invalid("choose approve, approve narrower or deny")),
        }
    }
}
