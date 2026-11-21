//! AgentHub recipient checks; identity SQL remains here, inside the board transaction.
use super::*;
use std::{future::Future, pin::Pin};

pub(crate) struct Directory(pub(crate) crate::config::Live);

impl devagotchi_agenthub::hosted::PrincipalDirectory for Directory {
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
