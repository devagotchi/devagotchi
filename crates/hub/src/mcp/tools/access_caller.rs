//! Access tools bind only the live bearer principal; no persistent session credential.
use crate::identity::{
    Subject,
    accounts::{
        Accounts,
        api_keys::{KeyLimits, Scope},
    },
    bearer::BearerVerifier,
};
use devagotchi_agenthub::{Actor, policy::Identity};
use devagotchi_contracts::{BridgeError, BridgeResult};
use rmcp::{RoleServer, service::RequestContext};
use std::sync::Arc;

pub(crate) struct AccessCaller {
    pub subject: Subject,
    pub accounts: Arc<Accounts>,
    pub profile: Option<Identity>,
    verifier: BearerVerifier,
    token: String,
    limits: Option<KeyLimits>,
}
impl AccessCaller {
    pub async fn new(context: &RequestContext<RoleServer>) -> BridgeResult<Self> {
        let refused = || {
            BridgeError::new(
                "unauthenticated",
                "machine access requires an authenticated request",
            )
        };
        let parts = context
            .extensions
            .get::<http::request::Parts>()
            .ok_or_else(refused)?;
        if parts
            .headers
            .contains_key(crate::agenthub::mcp::SEAT_HEADER)
        {
            return Err(BridgeError::new(
                "not_granted",
                "machine access uses the credential-bound principal, not a seat",
            ));
        }
        let subject = parts
            .extensions
            .get::<Subject>()
            .cloned()
            .ok_or_else(refused)?;
        let verifier = parts
            .extensions
            .get::<BearerVerifier>()
            .cloned()
            .ok_or_else(refused)?;
        let accounts = verifier.accounts.clone().ok_or_else(|| {
            BridgeError::new("not_offered", "account machine access is unavailable")
        })?;
        let token = parts
            .headers
            .get(http::header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .and_then(|h| {
                h.strip_prefix("Bearer ")
                    .or_else(|| h.strip_prefix("bearer "))
            })
            .map(str::trim)
            .filter(|h| !h.is_empty())
            .ok_or_else(refused)?
            .to_owned();
        let mut caller = Self {
            subject,
            accounts,
            profile: None,
            verifier,
            token,
            limits: None,
        };
        caller.refresh().await?;
        Ok(caller)
    }
    pub async fn refresh(&mut self) -> BridgeResult<()> {
        let (subject, limits) =
            self.verifier
                .verify(&self.token)
                .await
                .map_err(|error| match error {
                    crate::identity::auth::AuthError::Invalid(_) => {
                        BridgeError::new("unauthenticated", "credential is no longer valid")
                    }
                    crate::identity::auth::AuthError::Unavailable(_) => BridgeError::new(
                        "identity_unavailable",
                        "identity verification is unavailable",
                    ),
                })?;
        if subject.id != self.subject.id
            || subject.via != self.subject.via
            || subject.tenant != self.subject.tenant
            || subject.org != self.subject.org
            || subject.agent != self.subject.agent
            || subject.client != self.subject.client
        {
            return Err(BridgeError::new(
                "unauthenticated",
                "request identity changed",
            ));
        }
        if subject.waiting() {
            return Err(BridgeError::new(
                "early_access",
                "account access is not admitted",
            ));
        }
        self.profile = self.accounts.machine_identity(&subject).await?;
        self.subject = subject;
        self.limits = limits;
        Ok(())
    }
    pub fn actor(&self) -> Actor<'_> {
        Actor {
            id: self.subject.agent.as_deref().unwrap_or(&self.subject.id),
            human: self.profile.is_none(),
            wip_limit: None,
            policy: self.profile.clone(),
        }
    }
    pub fn scope(&self, scope: Scope) -> BridgeResult<()> {
        if self
            .limits
            .as_ref()
            .is_some_and(|limits| !limits.scopes.contains(&scope))
        {
            return Err(BridgeError::new(
                "key_denied",
                format!("this key needs {}", scope.as_str()),
            ));
        }
        Ok(())
    }
    pub fn allowed_projects(&self) -> &[String] {
        self.limits
            .as_ref()
            .map_or(&[], |limits| limits.projects.as_slice())
    }
    pub fn request_scope(&self, machine: &str, project: Option<&str>) -> BridgeResult<()> {
        if let Some(why) = self
            .limits
            .as_ref()
            .and_then(|limits| limits.access_request(machine, project))
        {
            return Err(BridgeError::new("key_denied", why));
        }
        Ok(())
    }
    pub fn history_scope(&self, machine: Option<&str>) -> BridgeResult<()> {
        if let Some(why) = self
            .limits
            .as_ref()
            .and_then(|limits| limits.access_history(machine))
        {
            return Err(BridgeError::new("key_denied", why));
        }
        Ok(())
    }
}
