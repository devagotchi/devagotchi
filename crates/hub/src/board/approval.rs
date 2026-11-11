//! Approvals: an agent asks a person to approve one step, the person approves or denies
//! (from the hub's `/approvals` page on a phone, or any surface that calls
//! `decide_approval`), and the agent continues on the decision. An approval is a ticket of
//! kind `approval` that starts in the workflow's attention state, so it shows where people
//! look and never sits in an agent's queue. Only a person's decision moves it: approved
//! goes to a done state, denied to cancelled, each with an `approval.decided` event.
//!
//! Not every step needs a person in the moment. An agent names the kind of step it asks for
//! (`kind`: `restart`, `deploy`, `edit-file` …) and, when it matters, what it is aimed at
//! (`target`: a machine, a service). A person may approve a kind ahead of time on a board,
//! for that target or any, for up to `MAX_STANDING_DAYS` (`grant_standing`): a standing
//! authorization, kept in the board's settings and granted, used and revoked on its chain.
//! An approval asked of a kind a live one covers is approved the moment it is asked, by
//! `standing:<id>`, with a note naming who granted it and until when; nobody is notified.
//! Only a person grants or revokes one; an agent never can (`Actor::human`). What people
//! decided is the record of what they want: after `SUGGEST_AFTER` approvals of one kind and
//! target in 30 days with none denied, `suggestions` offers to approve it ahead of time.

use devagotchi_contracts::BridgeResult;
use rusqlite::{OptionalExtension, params};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    Actor, Board, Filter, StateKind, Ticket, Workflow, fail, insert_note, invalid, load_workflow,
    read_ticket, store_error,
};
use crate::config::valid_name;

pub const KIND: &str = "approval";

/// The longest a person may approve a kind of step ahead of time.
pub const MAX_STANDING_DAYS: u32 = 90;
/// Live standing authorizations a board holds at most.
const MAX_LIVE: usize = 50;
/// Approvals of one kind and target, none denied, in 30 days before it is suggested.
pub const SUGGEST_AFTER: usize = 3;
const DAY_MS: i64 = 86_400_000;

/// A kind of step a person approved ahead of time on one board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Standing {
    /// `st-` and 8 hex digits: what revoking names it by.
    pub id: String,
    /// The kind of step it covers, as agents name it.
    pub kind: String,
    /// The one target it covers; absent, any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    pub granted_by: String,
    /// Milliseconds since the epoch.
    pub granted_at: i64,
    pub expires_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revoked_by: Option<String>,
}

impl Standing {
    pub fn live(&self, now: i64) -> bool {
        self.revoked_at.is_none() && now < self.expires_at
    }

    pub fn covers(&self, kind: &str, target: Option<&str>) -> bool {
        self.kind == kind && (self.target.is_none() || self.target.as_deref() == target)
    }
}

/// One approval an agent asks for.
#[derive(Debug, Clone, Copy, Default)]
pub struct Ask<'a> {
    pub title: &'a str,
    pub what: &'a str,
    pub why: &'a str,
    pub undo: &'a str,
    pub kind: Option<&'a str>,
    pub target: Option<&'a str>,
}

/// A kind of step people keep approving and never deny: worth approving ahead of time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Suggestion {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// How many were approved in the last 30 days.
    pub approved: usize,
}

/// The value after `key` (`kind:`, `target:`) among an approval's labels.
pub(crate) fn label_value(labels: &[String], key: &str) -> Option<String> {
    labels
        .iter()
        .find_map(|label| label.strip_prefix(key))
        .map(str::to_string)
}

fn checked(field: &str, value: &str) -> BridgeResult<String> {
    let value = value.trim();
    if valid_name(value) {
        Ok(value.to_string())
    } else {
        Err(invalid(format!(
            "{field} {value:?} is not [a-z0-9][a-z0-9-]{{0,62}}"
        )))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Approved,
    Denied,
}

/// Where an approval stands. `status` is `pending`, `approved` or `denied`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ApprovalStatus {
    pub id: String,
    pub title: String,
    pub status: String,
    /// What will happen, why, and how to undo it, as the agent wrote them.
    pub description: String,
    pub requested_by: String,
    pub requested_at: i64,
    /// Changes whenever the approval does; a decision names the version the person saw.
    pub version: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decided_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decided_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// The kind of step, when the agent named one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// What it is aimed at, when the agent named it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

pub(super) fn text(field: &str, value: &str, max: usize, required: bool) -> BridgeResult<String> {
    let value = value.trim();
    if (required && value.is_empty()) || value.chars().count() > max {
        let need = if required { "1" } else { "0" };
        return Err(invalid(format!(
            "{field} must be {need} to {max} characters"
        )));
    }
    Ok(value.to_string())
}

/// The done state a verdict lands in: `done` / `cancelled` when the workflow has them as
/// done states, else its first done state.
pub(super) fn landing(workflow: &Workflow, verdict: Verdict) -> BridgeResult<String> {
    let preferred = match verdict {
        Verdict::Approved => "done",
        Verdict::Denied => "cancelled",
    };
    if workflow.is_done(preferred) {
        return Ok(preferred.into());
    }
    workflow
        .states
        .iter()
        .find(|s| s.kind == StateKind::Done)
        .map(|s| s.key.clone())
        .ok_or_else(|| fail("no_done_state", "the workflow has no done state"))
}

impl Board {
    /// Ask a person to approve one step. `what` is required; `why` and `undo` should be.
    pub fn request_approval(
        &self,
        actor: &Actor<'_>,
        title: &str,
        what: &str,
        why: &str,
        undo: &str,
    ) -> BridgeResult<ApprovalStatus> {
        self.ask(
            actor,
            &Ask {
                title,
                what,
                why,
                undo,
                kind: None,
                target: None,
            },
        )
        .map(|(status, _)| status)
    }

    /// Ask for one approval. When a live standing authorization covers its kind and target,
    /// it is approved at once and the authorization comes back with it.
    pub fn ask(
        &self,
        actor: &Actor<'_>,
        ask: &Ask<'_>,
    ) -> BridgeResult<(ApprovalStatus, Option<Standing>)> {
        let kind = ask.kind.map(|kind| checked("kind", kind)).transpose()?;
        let target = ask
            .target
            .map(|target| checked("target", target))
            .transpose()?;
        if target.is_some() && kind.is_none() {
            return Err(invalid(
                "a target goes with a kind: name the kind of step too",
            ));
        }
        let mut labels = vec![KIND.to_string()];
        labels.extend(kind.iter().map(|kind| format!("kind:{kind}")));
        labels.extend(target.iter().map(|target| format!("target:{target}")));
        let status = self.open_approval(actor, ask, &labels)?;
        let Some(kind) = kind else {
            return Ok((status, None));
        };
        let now = self.now();
        let standing = self
            .settings()?
            .standing
            .into_iter()
            .find(|s| s.live(now) && s.covers(&kind, target.as_deref()));
        let Some(standing) = standing else {
            return Ok((status, None));
        };
        let by = format!("standing:{}", standing.id);
        let note = format!(
            "Approved at once: {} approved {} ahead of time until {}.",
            standing.granted_by,
            match &standing.target {
                Some(target) => format!("{} on {target}", standing.kind),
                None => standing.kind.clone(),
            },
            crate::web::utc((standing.expires_at / 1000).max(0) as u64)
        );
        let decided = self.decide_approval(
            &Actor {
                id: &by,
                human: true,
                wip_limit: None,
            },
            &status.id,
            status.version,
            Verdict::Approved,
            &note,
        )?;
        Ok((decided, Some(standing)))
    }

    fn open_approval(
        &self,
        actor: &Actor<'_>,
        ask: &Ask<'_>,
        labels: &[String],
    ) -> BridgeResult<ApprovalStatus> {
        let (title, what, why, undo) = (ask.title, ask.what, ask.why, ask.undo);
        if let Some(hosted) = &self.hosted {
            return self.hosted_call(
                hosted.request_approval(actor, title, what, why, undo, labels),
                true,
            );
        }
        let labels = serde_json::to_string(labels).expect("labels json");
        let title = text("title", title, 300, true)?;
        let what = text("what", what, 4000, true)?;
        let why = text("why", why, 4000, false)?;
        let undo = text("undo", undo, 4000, false)?;
        let description = format!("What: {what}\n\nWhy: {why}\n\nUndo: {undo}");
        let id = self.write(actor, |w| {
            let workflow = load_workflow(w.tx)?;
            let id = w.fresh_id(&self.prefix, &self.project, &title, actor.id)?;
            w.tx.execute(
                "INSERT INTO tickets (id, kind, title, description, state, priority, labels, parent,
                   assignee, created_by, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 1, ?8, NULL, NULL, ?6, ?7, ?7)",
                params![id, KIND, title, description, workflow.attention_state, actor.id, w.now, labels],
            )
            .map_err(store_error)?;
            w.event(
                "approval.requested",
                Some(&id),
                None,
                serde_json::json!({ "title": title }),
            )?;
            Ok(id)
        })?;
        self.approval_status(&id)
    }

    /// A person approves or denies `id` as they saw it at `version`.
    pub fn decide_approval(
        &self,
        actor: &Actor<'_>,
        id: &str,
        version: i64,
        verdict: Verdict,
        note: &str,
    ) -> BridgeResult<ApprovalStatus> {
        if let Some(hosted) = &self.hosted {
            return self.hosted_call(
                hosted.decide_approval(actor, id, version, verdict, note),
                true,
            );
        }
        if !actor.human {
            return Err(fail("humans_only", "only a person can approve or deny"));
        }
        let note = text("note", note, 2000, false)?;
        self.write(actor, |w| {
            let workflow = load_workflow(w.tx)?;
            let ticket = read_ticket(w.tx, &workflow, id, w.now)?;
            if ticket.kind != KIND {
                return Err(fail("not_an_approval", format!("{id} is not an approval")));
            }
            if ticket.state != workflow.attention_state {
                return Err(fail("already_decided", format!("{id} is already decided")));
            }
            if ticket.version != version {
                return Err(fail(
                    "approval_changed",
                    format!("{id} changed since you saw it; look again before deciding"),
                ));
            }
            let to = landing(&workflow, verdict)?;
            w.tx.execute(
                "UPDATE tickets SET state = ?2, closed_at = ?3, version = version + 1, updated_at = ?3
                 WHERE id = ?1",
                params![id, to, w.now],
            )
            .map_err(store_error)?;
            if !note.is_empty() {
                insert_note(w.tx, id, actor.id, "decision", &note, None, None, w.now)?;
            }
            w.event(
                "approval.decided",
                Some(id),
                None,
                serde_json::json!({ "verdict": verdict, "note": note, "to": to }),
            )?;
            Ok(())
        })?;
        self.approval_status(id)
    }

    pub fn approval_status(&self, id: &str) -> BridgeResult<ApprovalStatus> {
        if let Some(hosted) = &self.hosted {
            return self.hosted_call(hosted.approval_status(id), false);
        }
        let connection = self.local()?;
        let workflow = load_workflow(&connection)?;
        let ticket = read_ticket(&connection, &workflow, id, self.now())?;
        if ticket.kind != KIND {
            return Err(fail("not_an_approval", format!("{id} is not an approval")));
        }
        let decision: Option<(String, i64, String)> = connection
            .query_row(
                "SELECT actor, at, data FROM events WHERE ticket = ?1 AND action = 'approval.decided'
                 ORDER BY seq DESC LIMIT 1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(store_error)?;
        Ok(status_of(ticket, decision))
    }

    /// A person approves `kind` (on `target`, or any) ahead of time on this board for
    /// `days` (1 to `MAX_STANDING_DAYS`).
    pub fn grant_standing(
        &self,
        actor: &Actor<'_>,
        kind: &str,
        target: Option<&str>,
        days: u32,
    ) -> BridgeResult<Standing> {
        if !actor.human {
            return Err(fail(
                "humans_only",
                "only a person approves a kind of step ahead of time",
            ));
        }
        let kind = checked("kind", kind)?;
        let target = target.map(|t| checked("target", t)).transpose()?;
        if !(1..=MAX_STANDING_DAYS).contains(&days) {
            return Err(invalid(format!(
                "approve ahead of time for 1 to {MAX_STANDING_DAYS} days"
            )));
        }
        let now = self.now();
        let standing = Standing {
            id: format!("st-{}", hex::encode(crate::link::random::<4>())),
            kind,
            target,
            granted_by: actor.id.to_string(),
            granted_at: now,
            expires_at: now + i64::from(days) * DAY_MS,
            revoked_at: None,
            revoked_by: None,
        };
        let made = standing.clone();
        self.edit_settings(actor, "approval.standing_granted", move |settings| {
            // What ended over 30 days ago is forgotten; the chain keeps it.
            settings.standing.retain(|s| {
                s.live(now) || now - s.revoked_at.unwrap_or(s.expires_at).min(s.expires_at) < 30 * DAY_MS
            });
            if settings.standing.iter().filter(|s| s.live(now)).count() >= MAX_LIVE {
                return Err(fail(
                    "too_many",
                    format!("this board already approves {MAX_LIVE} kinds ahead of time; revoke one first"),
                ));
            }
            settings.standing.push(made.clone());
            Ok(serde_json::json!({
                "id": made.id, "kind": made.kind, "target": made.target,
                "expires_at": made.expires_at,
            }))
        })?;
        Ok(standing)
    }

    /// A person ends a standing authorization at once.
    pub fn revoke_standing(&self, actor: &Actor<'_>, id: &str) -> BridgeResult<Standing> {
        if !actor.human {
            return Err(fail(
                "humans_only",
                "only a person ends an approval given ahead of time",
            ));
        }
        let now = self.now();
        let who = actor.id.to_string();
        let settings = self.edit_settings(actor, "approval.standing_revoked", |settings| {
            let found = settings
                .standing
                .iter_mut()
                .find(|s| s.id == id && s.live(now))
                .ok_or_else(|| {
                    fail(
                        "not_found",
                        format!("no live standing authorization {id} here"),
                    )
                })?;
            found.revoked_at = Some(now);
            found.revoked_by = Some(who.clone());
            Ok(serde_json::json!({ "id": id, "kind": found.kind, "target": found.target }))
        })?;
        settings
            .standing
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| fail("not_found", format!("no standing authorization {id} here")))
    }

    /// The board's standing authorizations, live first.
    pub fn standing(&self) -> BridgeResult<Vec<Standing>> {
        let now = self.now();
        let mut all = self.settings()?.standing;
        all.sort_by_key(|s| (!s.live(now), std::cmp::Reverse(s.granted_at)));
        Ok(all)
    }

    /// Kinds of step people approved at least `SUGGEST_AFTER` times in the last 30 days and
    /// never denied, that nothing approves ahead of time yet.
    pub fn suggestions(&self) -> BridgeResult<Vec<Suggestion>> {
        let now = self.now();
        let since = now - 30 * DAY_MS;
        let workflow = self.workflow()?;
        let approved = landing(&workflow, Verdict::Approved)?;
        let denied = landing(&workflow, Verdict::Denied)?;
        let live: Vec<Standing> = self
            .settings()?
            .standing
            .into_iter()
            .filter(|s| s.live(now))
            .collect();
        let asked = self.list(&Filter {
            kind: Some(KIND.into()),
            ..Filter::default()
        })?;
        let mut tally: std::collections::BTreeMap<(String, Option<String>), (usize, usize)> =
            Default::default();
        for ticket in asked.iter().filter(|t| t.created_at >= since) {
            let Some(kind) = label_value(&ticket.labels, "kind:") else {
                continue;
            };
            let target = label_value(&ticket.labels, "target:");
            let entry = tally.entry((kind, target)).or_default();
            if ticket.state == approved {
                entry.0 += 1;
            } else if ticket.state == denied {
                entry.1 += 1;
            }
        }
        Ok(tally
            .into_iter()
            .filter(|((kind, target), (yes, no))| {
                *no == 0
                    && *yes >= SUGGEST_AFTER
                    && !live.iter().any(|s| s.covers(kind, target.as_deref()))
            })
            .map(|((kind, target), (yes, _))| Suggestion {
                kind,
                target,
                approved: yes,
            })
            .collect())
    }

    /// Approvals waiting for a person, oldest first.
    pub fn pending_approvals(&self) -> BridgeResult<Vec<ApprovalStatus>> {
        if let Some(hosted) = &self.hosted {
            return self.hosted_call(hosted.pending_approvals(), false);
        }
        let ids: Vec<String> = {
            let connection = self.local()?;
            let workflow = load_workflow(&connection)?;
            let mut statement = connection
                .prepare(
                    "SELECT id FROM tickets WHERE kind = ?1 AND state = ?2 ORDER BY created_at, id",
                )
                .map_err(store_error)?;
            let rows = statement
                .query_map(params![KIND, workflow.attention_state], |row| row.get(0))
                .map_err(store_error)?;
            rows.collect::<Result<_, _>>().map_err(store_error)?
        };
        ids.iter().map(|id| self.approval_status(id)).collect()
    }
}

fn status_of(ticket: Ticket, decision: Option<(String, i64, String)>) -> ApprovalStatus {
    let (status, decided_by, decided_at, note) = match decision {
        Some((by, at, data)) => {
            let data: serde_json::Value = serde_json::from_str(&data).unwrap_or_default();
            let verdict = data["verdict"].as_str().unwrap_or("denied").to_string();
            let note = data["note"]
                .as_str()
                .filter(|n| !n.is_empty())
                .map(str::to_string);
            (verdict, Some(by), Some(at), note)
        }
        None => ("pending".to_string(), None, None, None),
    };
    ApprovalStatus {
        kind: label_value(&ticket.labels, "kind:"),
        target: label_value(&ticket.labels, "target:"),
        id: ticket.id,
        title: ticket.title,
        status,
        description: ticket.description,
        requested_by: ticket.created_by,
        requested_at: ticket.created_at,
        version: ticket.version,
        decided_by,
        decided_at,
        note,
    }
}
