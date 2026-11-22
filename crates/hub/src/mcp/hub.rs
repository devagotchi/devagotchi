//! The `Hub` one MCP session holds: what it reaches, who is calling, the machines this session routed to, and routing one call to a broker with its audit line.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use devagotchi_contracts::{BridgeError, RequestBody, ResponseBody};
use rmcp::{Json, RoleServer, service::RequestContext};

use crate::{
    agenthub::mcp::Boards,
    config::HubConfig,
    identity::Subject,
    machines::link::Links,
    ops::{analytics::Analytics, audit::Audit},
};

#[derive(Clone)]
pub struct Hub {
    pub config: Arc<HubConfig>,
    pub audit: Arc<Audit>,
    pub links: Arc<Links>,
    /// The project boards this hub serves (`board_*` tools, `crate::agenthub::mcp`).
    pub boards: Arc<Boards>,
    /// Web Push to people's phones when an approval waits; present with browser sign-in.
    pub push: Option<Arc<crate::notify::push::Push>>,
    /// The build this hub hands to machines that join it (`/join`, `/dist`), when configured.
    pub downloads: Option<Arc<crate::machines::downloads::Downloads>>,
    /// What a hosted hub counts for its operator (`crate::analytics`), when it counts.
    pub analytics: Option<Arc<Analytics>>,
    /// The workflows an AI starts for its person (`crate::workflows`), on a hub with its own
    /// accounts.
    pub workflows: Option<Arc<crate::workflows::Workflows>>,
    /// The site's offers and how an account is linked (`crate::offers`), on a hub with its
    /// own accounts.
    pub offers: Option<Arc<crate::offers::Offers>>,
    /// The account's outside services, called through the hub (`crate::connections`), on a
    /// hub with its own accounts.
    pub connections: Option<Arc<crate::connections::Connections>>,
    /// The machines this MCP session's calls were routed to (the service makes one `Hub`
    /// per session, `for_session`): what fills a new ticket's `machine` (P2c).
    pub routed: Arc<Mutex<Routed>>,
}

/// The distinct machines one MCP session routed calls to, as named: two say there were more.
#[derive(Debug, Default)]
pub struct Routed(Vec<String>);

impl Routed {
    fn add(&mut self, machine: &str) {
        if self.0.len() < 2 && !self.0.iter().any(|m| m == machine) {
            self.0.push(machine.to_string());
        }
    }

    /// The one machine this session routed to, when there was exactly one.
    pub fn only(&self) -> Option<String> {
        match self.0.as_slice() {
            [one] => Some(one.clone()),
            _ => None,
        }
    }
}

/// Where a granted broker lives: on this host, or behind a machine's link.
pub(crate) enum Route {
    Local(PathBuf),
    Link(String),
}

impl Hub {
    pub(crate) fn subject(
        &self,
        context: &RequestContext<RoleServer>,
    ) -> Result<Subject, Json<BridgeError>> {
        context
            .extensions
            .get::<http::request::Parts>()
            .and_then(|parts| parts.extensions.get::<Subject>())
            .cloned()
            .ok_or_else(|| {
                Json(BridgeError::new(
                    "unauthenticated",
                    "no authenticated subject on this request",
                ))
            })
    }

    /// This hub for one new MCP session: shared stores, its own routed machines.
    pub(crate) fn for_session(&self) -> Self {
        Self {
            routed: Arc::default(),
            ..self.clone()
        }
    }

    pub(crate) async fn route(
        &self,
        subject: &Subject,
        broker: &str,
    ) -> Result<Route, Json<BridgeError>> {
        let route = self.resolve(subject, broker).await?;
        self.routed.lock().expect("routed lock").add(broker);
        Ok(route)
    }

    pub(super) async fn resolve(
        &self,
        subject: &Subject,
        broker: &str,
    ) -> Result<Route, Json<BridgeError>> {
        let allowed = self.config.brokers_for(&subject.id);
        let unknown = || {
            Err(Json(BridgeError::new(
                "unknown_broker",
                format!("no broker named {broker:?} is enrolled"),
            )))
        };
        match allowed.iter().find(|b| b.name == broker) {
            Some(entry) => Ok(match &entry.socket {
                Some(socket) => Route::Local(PathBuf::from(socket)),
                None => Route::Link(entry.name.clone()),
            }),
            // A machine the caller added is the caller's alone; anyone else is told it
            // does not exist, so names of other accounts' machines are not revealed.
            None if self.config.broker(broker).is_none() => {
                (self.own_machine(subject, broker).await)
                    .map_or_else(unknown, |device| Ok(Route::Link(device.name)))
            }
            None => Err(Json(BridgeError::new(
                "not_granted",
                format!("{} holds no grant for broker {broker:?}", subject.id),
            ))),
        }
    }

    /// One broker request along `route`, unaudited: the caller records the call.
    pub(crate) async fn send(
        &self,
        route: &Route,
        body: RequestBody,
    ) -> Result<ResponseBody, BridgeError> {
        match route {
            Route::Local(socket) => devagotchi_ipc::request(socket, body).await,
            Route::Link(device) => self.links.call(device, body).await,
        }
    }

    /// One audited broker request for this session's subject, as its caller.
    pub(crate) async fn call(
        &self,
        tool: &str,
        broker: &str,
        mut body: RequestBody,
        context: RequestContext<RoleServer>,
    ) -> Result<ResponseBody, Json<BridgeError>> {
        let subject = self.subject(&context)?;
        body.set_caller(&Self::caller(&subject, &context));
        let operation_id = body.target();
        // An inspection waits at most its own bound, and its outcome (`inspection_timeout`,
        // `api_version`) is what the audit records.
        let inspection = matches!(body, RequestBody::BrokerCapabilities(_));
        let result = match self.route(&subject, broker).await {
            Ok(route) => {
                let sent = self.send_for_request(tool, &route, body, &context);
                let answer = async {
                    if inspection {
                        devagotchi_mcp::capabilities::response(sent).await
                    } else {
                        sent.await
                    }
                };
                tokio::select! {
                    biased;
                    _ = context.ct.cancelled() => Err(Json(BridgeError::new(
                        "request_cancelled",
                        "stopped waiting for the broker response; admitted operations are not cancelled",
                    ))),
                    response = answer => response.map_err(Json),
                }
            }
            Err(error) => Err(error),
        };
        let outcome = match &result {
            Ok(_) => "ok".to_string(),
            Err(Json(error)) => error.code.clone(),
        };
        let recorded_id = match &result {
            Ok(ResponseBody::Start(output)) => output.operation.operation_id.clone(),
            _ => operation_id,
        };
        self.record_completion(&subject, tool, broker, &recorded_id, &outcome)
            .await;
        result
    }
}

pub(crate) fn unexpected_response(expected: &str, actual: &ResponseBody) -> Json<BridgeError> {
    Json(devagotchi_mcp::unexpected(expected, actual))
}
