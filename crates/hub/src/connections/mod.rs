//! Connections: a tenant's outside services, called through the hub (plan
//! `.ai/dev/plans/connections-gateway.md`, X1 and X2). A person adds one on `/app/connections`
//! with its key, which is checked against the service, sealed, and never shown again. Their
//! agents then call the service with `connection_call`: the hub attaches the key, sends the
//! request to that service's own host and nowhere else, takes the key out of whatever comes
//! back, and audits the call under the agent's name. `connections` lists what may be called,
//! by name only.

use std::{sync::Arc, time::Duration};

pub(crate) mod catalog;
pub mod stored;

use catalog::Service;
use devagotchi_contracts::BridgeError;
use rmcp::{
    Json, RoleServer, handler::server::wrapper::Parameters, service::RequestContext, tool,
    tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{accounts::Accounts, auth::Subject, hub::Hub, sealed::Sealer};

/// The longest answer a call returns; more is cut, and says so.
const MAX_ANSWER: usize = 256 * 1024;
/// The largest body a call may send.
const MAX_BODY: usize = 1024 * 1024;

/// A tenant's connections and the calls through them.
pub struct Connections {
    accounts: Arc<Accounts>,
    sealer: Sealer,
    http: reqwest::Client,
    /// Tests send every service's calls to a stand-in here.
    base_override: std::sync::RwLock<Option<String>>,
}

fn refuse(code: &str, message: impl Into<String>) -> BridgeError {
    BridgeError::new(code, message)
}

/// What a call answered.
#[derive(Debug, Serialize, JsonSchema)]
pub struct CallAnswer {
    /// The service's HTTP status.
    pub status: u16,
    pub content_type: Option<String>,
    /// The answer as JSON when it is JSON, else as text; the key never appears in it.
    pub body: Value,
    /// Whether the answer was longer than 256 KiB and cut there.
    pub truncated: bool,
}

/// One of the tenant's connections, as an agent sees it: never its key.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ConnectionView {
    pub name: String,
    pub service: String,
    pub title: String,
    /// Unix seconds it was last called, if ever.
    pub last_used_at: Option<u64>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ConnectionsView {
    pub connections: Vec<ConnectionView>,
    /// Where a person adds one.
    pub add_at: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CallIn {
    /// The connection's name, from `connections`.
    pub connection: String,
    /// GET, POST, PUT, PATCH or DELETE.
    pub method: String,
    /// The path on the service, like `/emails`; query parameters go in `query`.
    pub path: String,
    /// Query parameters.
    #[serde(default)]
    pub query: std::collections::BTreeMap<String, String>,
    /// A JSON body, sent as `application/json`.
    #[serde(default)]
    pub body: Option<Value>,
}

impl Connections {
    pub fn new(accounts: Arc<Accounts>, sealer: Sealer) -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            // A redirect could carry the key to another host.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| error.to_string())?;
        Ok(Self {
            accounts,
            sealer,
            http,
            base_override: std::sync::RwLock::new(None),
        })
    }

    #[cfg(test)]
    pub(crate) fn send_everything_to(&self, base: &str) {
        *self.base_override.write().unwrap() = Some(base.to_string());
    }

    fn base(&self, service: &Service) -> String {
        self.base_override
            .read()
            .expect("base lock")
            .clone()
            .unwrap_or_else(|| service.base.to_string())
    }

    pub fn accounts(&self) -> &Accounts {
        &self.accounts
    }

    pub fn sealer(&self) -> &Sealer {
        &self.sealer
    }

    /// The name a key is sealed under: the tenant and the connection, so a sealed key moved
    /// to another row does not open.
    pub fn sealed_as(tenant: &str, name: &str) -> String {
        format!("connection:{tenant}:{name}")
    }

    async fn send(
        &self,
        service: &Service,
        key: &str,
        method: &str,
        path: &str,
        query: &[(String, String)],
        body: Option<&str>,
    ) -> Result<CallAnswer, BridgeError> {
        let method = method.to_ascii_uppercase();
        if !catalog::METHODS.contains(&method.as_str()) {
            return Err(refuse(
                "invalid",
                format!("{method:?}: a call is GET, POST, PUT, PATCH or DELETE"),
            ));
        }
        let url = catalog::target(&self.base(service), path, query)
            .map_err(|why| refuse("invalid", why))?;
        let mut request = self.http.request(
            reqwest::Method::from_bytes(method.as_bytes()).expect("a known method"),
            &url,
        );
        for (name, value) in catalog::headers(service, key) {
            request = request.header(name, value);
        }
        if let Some(body) = body {
            request = request
                .header("content-type", "application/json")
                .body(body.to_string());
        }
        let response = request.send().await.map_err(|error| {
            refuse(
                "service_unreachable",
                catalog::redact(&format!("{}: {error}", service.title), key),
            )
        })?;
        let status = response.status().as_u16();
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let bytes = response.bytes().await.map_err(|error| {
            refuse(
                "service_unreachable",
                catalog::redact(&error.to_string(), key),
            )
        })?;
        let truncated = bytes.len() > MAX_ANSWER;
        let text = String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_ANSWER)]).into_owned();
        let text = catalog::redact(&text, key);
        let body = if !truncated && content_type.as_deref().is_some_and(|t| t.contains("json")) {
            serde_json::from_str(&text).unwrap_or(Value::String(text))
        } else {
            Value::String(text)
        };
        Ok(CallAnswer {
            status,
            content_type,
            body,
            truncated,
        })
    }

    /// Whether `key` works on `service`: its check call answers 2xx. What the service said
    /// when it does not, with the key taken out.
    pub async fn check(&self, service: &Service, key: &str) -> Result<(), String> {
        let (method, path, body) = service.check;
        match self.send(service, key, method, path, &[], body).await {
            Ok(answer) if (200..300).contains(&answer.status) => Ok(()),
            Ok(answer) if matches!(answer.status, 401 | 403) => Err(format!(
                "{} refused this key ({}). Make a new one at {}.",
                service.title, answer.status, service.keys_at
            )),
            Ok(answer) => Err(format!(
                "{} answered {} to the check.",
                service.title, answer.status
            )),
            Err(error) => Err(error.message),
        }
    }

    /// A call through the tenant's connection `name`.
    pub async fn call(&self, tenant: &str, input: &CallIn) -> Result<CallAnswer, BridgeError> {
        let stored = self
            .accounts
            .connection(tenant, &input.connection)
            .await?
            .ok_or_else(|| {
                refuse(
                    "not_found",
                    format!(
                        "no connection {:?}: connections lists them; a person adds one on /app/connections",
                        input.connection
                    ),
                )
            })?;
        let service = catalog::service(&stored.service).ok_or_else(|| {
            refuse(
                "invalid",
                "this connection's service is not offered any more",
            )
        })?;
        let key = self
            .sealer
            .open_value(&Self::sealed_as(tenant, &stored.name), &stored.sealed_key)
            .ok_or_else(|| {
                refuse(
                    "connection_key",
                    "this connection's key cannot be opened on this hub; add it again",
                )
            })?;
        let body = match &input.body {
            Some(body) => {
                let text =
                    serde_json::to_string(body).map_err(|e| refuse("invalid", e.to_string()))?;
                if text.len() > MAX_BODY {
                    return Err(refuse("invalid", "a body is at most 1 MiB"));
                }
                Some(text)
            }
            None => None,
        };
        let query: Vec<(String, String)> = input
            .query
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let answer = self
            .send(
                service,
                &key,
                &input.method,
                &input.path,
                &query,
                body.as_deref(),
            )
            .await?;
        if let Err(error) = self.accounts.touch_connection(tenant, &stored.name).await {
            eprintln!("devagotchi hub: connection last used: {error}");
        }
        Ok(answer)
    }

    /// The tenant's connections, as an agent sees them.
    pub async fn view(
        &self,
        tenant: &str,
        public_url: &str,
    ) -> Result<ConnectionsView, BridgeError> {
        let connections = self
            .accounts
            .connections(tenant)
            .await?
            .into_iter()
            .map(|c| ConnectionView {
                title: catalog::service(&c.service)
                    .map(|s| s.title.to_string())
                    .unwrap_or_else(|| c.service.clone()),
                name: c.name,
                service: c.service,
                last_used_at: c.last_used_at,
            })
            .collect();
        Ok(ConnectionsView {
            connections,
            add_at: format!("{}/app/connections", public_url.trim_end_matches('/')),
        })
    }
}

impl Hub {
    fn connections_for(&self, subject: &Subject) -> Result<(&Connections, String), BridgeError> {
        let connections = self.connections.as_deref().ok_or_else(|| {
            refuse(
                "not_offered",
                "this hub keeps no accounts, so it holds no connections",
            )
        })?;
        let tenant = subject
            .tenant
            .clone()
            .ok_or_else(|| refuse("not_offered", "connections belong to an account"))?;
        Ok((connections, tenant))
    }
}

#[tool_router(router = connections_router, vis = "pub(crate)")]
impl Hub {
    #[tool(
        name = "connections",
        description = "The outside services this account connected for you to call through the hub (Resend, Stripe, GitHub, Slack, Vercel, Supabase, Linear, OpenAI, Anthropic, Postmark, Notion), by name. You never get their keys: call one with connection_call. None yet: give the person add_at, where they add one.",
        title = "Services you may call",
        annotations(
            title = "Services you may call",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn connections(
        &self,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<ConnectionsView>, Json<BridgeError>> {
        let subject = self.subject(&context)?;
        let result = match self.connections_for(&subject) {
            Ok((connections, tenant)) => connections.view(&tenant, &self.config.public_url).await,
            Err(error) => Err(error),
        };
        let outcome = result
            .as_ref()
            .map(|_| "ok")
            .unwrap_or_else(|e| e.code.as_str())
            .to_string();
        self.record(&subject, "connections", "", "", &outcome)
            .await?;
        result.map(Json).map_err(Json)
    }

    #[tool(
        name = "connection_call",
        description = "Call an outside service through one of this account's connections (see connections): method, path on that service (like /emails), optional query and JSON body. The hub adds the key, sends it only to that service, and returns its status and answer with the key taken out. Follow the service's own API documentation for paths and bodies. Sending email, money or messages acts in the person's name: ask with approval_request first when they have not asked for it.",
        title = "Call a connected service",
        annotations(
            title = "Call a connected service",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn connection_call(
        &self,
        Parameters(input): Parameters<CallIn>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<CallAnswer>, Json<BridgeError>> {
        let subject = self.subject(&context)?;
        let result = match self.connections_for(&subject) {
            Ok((connections, tenant)) => connections.call(&tenant, &input).await,
            Err(error) => Err(error),
        };
        let outcome = match &result {
            Ok(answer) => format!("http_{}", answer.status),
            Err(error) => error.code.clone(),
        };
        let what = format!("{} {}", input.method.to_ascii_uppercase(), input.path);
        self.record(
            &subject,
            "connection_call",
            &input.connection,
            &what,
            &outcome,
        )
        .await?;
        result.map(Json).map_err(Json)
    }
}
