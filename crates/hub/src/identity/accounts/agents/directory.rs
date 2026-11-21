//! AgentHub recipient checks; identity SQL remains here, inside the board transaction.
use super::*;
use std::{future::Future, pin::Pin};

pub(crate) struct Directory(pub(crate) crate::config::Live);

impl devagotchi_agenthub::hosted::PrincipalDirectory for Directory {
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
