//! How one board tool call runs: who the caller is (subject, seat, actor), the board it reaches, and the line it leaves on the audit chain.

use devagotchi_contracts::{BridgeError, BridgeResult};
use rmcp::{Json, RoleServer, service::RequestContext};

use super::{BoardError, BoardOut, Kind, SEAT_HEADER, refuse, viewer_may_call};
use crate::{
    agenthub::{Actor, Board},
    config::valid_name,
    identity::Subject,
    mcp::Hub,
    tenancy::orgs::Role,
};

impl Hub {
    /// The seat this request names (`X-Devagotchi-Seat`), when it names a valid one.
    pub(crate) fn request_seat(&self, context: &RequestContext<RoleServer>) -> Option<String> {
        self.seat(context).ok().flatten()
    }

    pub(super) fn seat(
        &self,
        context: &RequestContext<RoleServer>,
    ) -> Result<Option<String>, Json<BoardError>> {
        let Some(value) = context
            .extensions
            .get::<http::request::Parts>()
            .and_then(|parts| parts.headers.get(SEAT_HEADER))
        else {
            return Ok(None);
        };
        match value.to_str() {
            Ok(seat) if valid_name(seat) => Ok(Some(seat.to_string())),
            _ => Err(refuse(
                BridgeError::new(
                    "invalid",
                    "X-Devagotchi-Seat must be a name like a board's: [a-z0-9][a-z0-9-]{0,62}",
                ),
                None,
            )),
        }
    }

    /// Who the store sees: the seat on the account's own board, `<account>/<seat>` on an
    /// org's or a shared one (two members' Claudes are two actors), the account itself
    /// without a seat. A seat is never a person; the account is the person on its own
    /// board and on its org's, and on a shared one when its grant says so.
    pub(crate) fn actor_for(
        &self,
        subject: &Subject,
        seat: Option<&str>,
        kind: Kind,
        role: Role,
    ) -> (String, bool) {
        // An AI the hub connected itself (`via: hub`) is never the person: without a seat
        // of its own it works as the account's "ai" seat.
        let seat = match (seat, subject.via) {
            (None, "hub" | "key") => Some("ai"),
            (seat, _) => seat,
        };
        match (seat, kind) {
            (Some(seat), Kind::Account) => (seat.to_string(), false),
            (Some(seat), _) => (format!("{}/{seat}", subject.id), false),
            (None, _) => (subject.id.clone(), role.is_person()),
        }
    }

    /// The names this caller's AI client answers to (`inbox::client_aliases`).
    pub(super) fn client_names(&self, context: &RequestContext<RoleServer>) -> Vec<String> {
        self.subject(context)
            .ok()
            .and_then(|subject| subject.client)
            .map(|client| crate::agenthub::inbox::client_aliases(&client))
            .unwrap_or_default()
    }

    /// Run one store call for the authenticated subject, on a blocking thread, and
    /// audit it: who, which tool, which project, which ticket, how it ended.
    pub(super) async fn board_call<T, F>(
        &self,
        tool: &'static str,
        project: &str,
        ticket: Option<&str>,
        context: &RequestContext<RoleServer>,
        f: F,
    ) -> BoardOut<T>
    where
        T: Send + 'static,
        F: FnOnce(&Board, &Actor<'_>) -> BridgeResult<T> + Send + 'static,
    {
        self.board_call_scoped(tool, project, ticket, context, None, f)
            .await
    }

    /// The call itself. Every refusal from here on is audited, the seat's included (G7): a
    /// bad `X-Devagotchi-Seat` is a call that was made and refused. The audit line comes
    /// after the store committed, so a failed append never turns a write that happened into
    /// a refusal: the real result goes back and the operator alert goes up (D1).
    pub(super) async fn board_call_scoped<T, F>(
        &self,
        tool: &'static str,
        project: &str,
        ticket: Option<&str>,
        context: &RequestContext<RoleServer>,
        required_kind: Option<Kind>,
        f: F,
    ) -> BoardOut<T>
    where
        T: Send + 'static,
        F: FnOnce(&Board, &Actor<'_>) -> BridgeResult<T> + Send + 'static,
    {
        let subject = self.board_subject(context)?;
        let result = match self.seat(context) {
            Err(Json(refused)) => Err(BridgeError::new(&refused.code, refused.message)),
            Ok(seat) => self.run_scoped(&subject, seat, tool, project, required_kind, f).await,
        };
        let outcome = match &result {
            Ok(_) => "ok".to_string(),
            Err(error) => error.code.clone(),
        };
        self.record_completion(
            &subject,
            tool,
            project,
            ticket.unwrap_or_default(),
            &outcome,
        )
        .await;
        result.map(Json).map_err(|error| refuse(error, ticket))
    }

    async fn run_scoped<T, F>(
        &self,
        subject: &Subject,
        seat: Option<String>,
        tool: &'static str,
        project: &str,
        required_kind: Option<Kind>,
        f: F,
    ) -> BridgeResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&Board, &Actor<'_>) -> BridgeResult<T> + Send + 'static,
    {
        match self.board_access(subject, project)? {
            (_, kind, _) if required_kind.is_some_and(|required| kind != required) => {
                Err(BridgeError::new(
                    "not_granted",
                    "issue repository belongs to the configured shared board",
                ))
            }
            (_, _, role) if !role.may_work() && !viewer_may_call(tool) => {
                Err(BridgeError::new(
                    "read_only",
                    format!(
                        "{} is a viewer on {project:?}: it reads this board and changes nothing",
                        subject.id
                    ),
                ))
            }
            (board, kind, role) => {
                let (id, human) = self.actor_for(subject, seat.as_deref(), kind, role);
                tokio::task::spawn_blocking(move || {
                    let actor = Actor {
                        id: &id,
                        human,
                        wip_limit: None,
                    };
                    f(&board, &actor)
                })
                .await
                .unwrap_or_else(|join| {
                    Err(BridgeError::new(
                        "board_store",
                        format!("board call did not complete: {join}"),
                    ))
                })
            }
        }
    }

    pub(super) fn board_subject(
        &self,
        context: &RequestContext<RoleServer>,
    ) -> Result<Subject, Json<BoardError>> {
        self.subject(context)
            .map_err(|Json(error)| refuse(error, None))
    }

    /// The key the fence memory uses: the account, and the seat when one is named, so two
    /// seats of one account never pick up each other's fences. A seat that is not a name is
    /// left out here; the call itself refuses it, with its audit line (`board_call_scoped`).
    pub(super) fn subject_id(
        &self,
        context: &RequestContext<RoleServer>,
    ) -> Result<String, Json<BoardError>> {
        let subject = self.board_subject(context)?;
        Ok(match self.seat(context).ok().flatten() {
            Some(seat) => format!("{}#{seat}", subject.id),
            None => subject.id,
        })
    }
}
