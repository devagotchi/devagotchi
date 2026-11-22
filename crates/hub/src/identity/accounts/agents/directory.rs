//! AgentHub recipient checks; identity SQL remains here, inside the board transaction.
use super::*;
use std::{future::Future, pin::Pin};

pub(crate) struct Directory(
    pub(crate) crate::config::Live,
    pub(crate) Option<crate::tenancy::orgs::ProjectRef>,
);

impl devagotchi_agenthub::hosted::PrincipalDirectory for Directory {
    fn project_retirement<'a>(
        &'a self,
        connection: &'a mut AnyConnection,
        tenant: &'a str,
        account: &'a str,
        project: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), Failed>> + Send + 'a>> {
        Box::pin(async move {
            let now = database_now(connection).await?;
            let used: i64=sqlx::query_scalar(
                "SELECT count(*) FROM auth_api_keys WHERE tenant_id=$1 AND account=$2
                 AND revoked_at IS NULL AND expires_at>$3 AND projects::jsonb @> jsonb_build_array($4::text)",
            ).bind(tenant).bind(account).bind(secs(now)).bind(project).fetch_one(connection).await?;
            if used > 0 {
                return Err(BridgeError::new(
                    "in_use",
                    "revoke the active keys explicitly naming this project before removing it",
                )
                .into());
            }
            Ok(())
        })
    }

    fn authorize_request<'a>(
        &'a self,
        connection: &'a mut AnyConnection,
        tenant: &'a str,
        principal: &'a str,
        actor: &'a devagotchi_agenthub::Actor<'_>,
    ) -> Pin<Box<dyn Future<Output = Result<(), Failed>> + Send + 'a>> {
        Box::pin(async move {
            // Provider/dev subjects are verified by the transport on every request.
            // They have no auth_accounts row. Preserve that existing authority contract,
            // while never letting a local or registered identity fall through to it.
            if !principal.starts_with("acct_")
                && !principal.starts_with('@')
                && crate::config::valid_subject(principal).is_ok()
                && actor.policy.is_none()
            {
                return Ok(());
            }
            // Organization and shared-board members keep their account in a different
            // tenant. Recheck their exact project role without changing board scope.
            if actor.policy.is_none()
                && valid_account_id(principal)
                && let Some(at) = &self.1
                && at.tenant == tenant
                && let Some(role) = crate::tenancy::orgs::effective_role_in(
                    connection,
                    tenant,
                    &at.owner,
                    &at.project,
                    principal,
                )
                .await?
                && if actor.human {
                    actor.id == principal && role.is_person()
                } else {
                    role.may_work()
                }
            {
                let live: bool = sqlx::query_scalar("SELECT request_account_live($1, $2)")
                    .bind(principal)
                    .bind(self.0.get().unsecured.idle_secs() as i64)
                    .fetch_one(&mut *connection)
                    .await?;
                if live {
                    return Ok(());
                }
            }
            if !self.enabled(connection, tenant, principal).await? {
                return Err(BridgeError::new(
                    "not_granted",
                    "request principal is no longer enabled",
                )
                .into());
            }
            if let Some(rest) = principal.strip_prefix("@agent:") {
                let (account, id) = rest.split_once(':').expect("enabled canonical agent");
                let profile = agent_in(connection, tenant, account, id).await?;
                let matches = actor.policy.as_ref().is_some_and(|identity| {
                    identity.kind == profile.kind && identity.roles == profile.roles
                });
                if actor.id != principal || actor.human || !matches {
                    return Err(BridgeError::new(
                        "not_granted",
                        "request profile authority changed; authenticate again",
                    )
                    .into());
                }
            } else if actor.policy.is_some() && !actor.human {
                return Err(BridgeError::new(
                    "not_granted",
                    "registered actor needs its canonical request principal",
                )
                .into());
            }
            Ok(())
        })
    }

    fn enabled<'a>(
        &'a self,
        connection: &'a mut AnyConnection,
        tenant: &'a str,
        principal: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<bool, Failed>> + Send + 'a>> {
        Box::pin(async move {
            let (account, agent) = if let Some(rest) = principal.strip_prefix("@agent:") {
                let Some((account, id)) = rest.split_once(':') else {
                    return Ok(false);
                };
                if !valid_account_id(account) || !devagotchi_agenthub::valid_name(id) {
                    return Ok(false);
                }
                (account, Some(id))
            } else if valid_account_id(principal) {
                (principal, None)
            } else {
                return Ok(false);
            };
            let Some(found) = account_in(connection, account).await? else {
                return Ok(false);
            };
            let now = database_now(connection).await?;
            let lapsed = found.provisional
                && now
                    >= found
                        .last_active
                        .unwrap_or(0)
                        .saturating_add(self.0.get().unsecured.idle_secs());
            if found.tenant != tenant || found.disabled_at.is_some() || lapsed {
                return Ok(false);
            }
            if let Some(id) = agent {
                let profile = match agent_in(connection, tenant, account, id).await {
                    Ok(profile) => profile,
                    Err(Failed::Refused(error)) if error.code == "not_found" => return Ok(false),
                    Err(error) => return Err(error),
                };
                if !profile.enabled {
                    return Ok(false);
                }
                catalog::check_profile_in(connection, tenant, &profile.kind, &profile.roles)
                    .await?;
            }
            Ok(true)
        })
    }
}
