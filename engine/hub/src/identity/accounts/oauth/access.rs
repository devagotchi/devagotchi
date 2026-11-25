//! Live access-token verification and its authenticated subject projection.
use super::*;

impl Grant {
    pub(crate) fn subject(self, accounts: &Accounts) -> crate::identity::Subject {
        crate::identity::Subject {
            client: self.client_name,
            agent: self.agent,
            ..accounts.subject(&self.account, "hub")
        }
    }
}

impl Accounts {
    /// Whether `account` may get tokens: it exists, is not disabled, and (made on first
    /// connect and not secured) has not lapsed unused. The refusal is a value, so the caller
    /// can commit what it already did (a spent code stays spent). Using it counts as use.
    pub(super) async fn live_account(
        &self,
        connection: &mut AnyConnection,
        account: &str,
        client: &str,
        now: u64,
    ) -> Result<Result<(), BridgeError>, Failed> {
        Ok(match account_in(connection, account).await? {
            Some(found) if found.disabled_at.is_none() && !self.lapsed(&found, now) => {
                match super::agents::oauth_agent_in(connection, &found.tenant, account, client)
                    .await
                {
                    Ok(_) => {}
                    Err(Failed::Refused(error))
                        if matches!(error.code.as_str(), "agent_disabled" | "not_found") =>
                    {
                        return Ok(Err(invalid_grant()));
                    }
                    Err(error) => return Err(error),
                }
                let mut found = found;
                tenants::touch(connection, &mut found, now).await?;
                Ok(())
            }
            Some(found) if found.disabled_at.is_some() => Err(account_disabled(account)),
            Some(_) => Err(invalid_grant()),
            None => Err(invalid_grant()),
        })
    }

    /// Who an access token speaks for, when it is live and its account is not disabled.
    pub async fn verify_access(&self, token: &str) -> BridgeResult<Option<Grant>> {
        self.verify_access_at(token, now_secs()).await
    }

    pub(crate) async fn verify_access_at(
        &self,
        token: &str,
        now: u64,
    ) -> BridgeResult<Option<Grant>> {
        if !token.starts_with("dgh_") || token.len() > 128 {
            return Ok(None);
        }
        let key = hash(token);
        let verified = transaction!(&self.store, |connection| {
            if self.enter(connection, Lookup::Token(&key)).await?.is_none() {
                return Ok(None);
            }
            let found = sqlx::query(concat!(
                "SELECT ",
                account_columns_a!(),
                ", t.client, c.name FROM oauth_tokens t
                 JOIN auth_accounts a ON a.id = t.account
                 LEFT JOIN oauth_clients c ON c.id = t.client
                 WHERE t.hash = $1 AND t.kind = 'access' AND t.expires_at > $2
                   AND a.disabled_at IS NULL"
            ))
            .bind(&key)
            .bind(secs(now))
            .fetch_optional(&mut *connection)
            .await?
            .map(|row| -> Result<Grant, sqlx::Error> {
                Ok(Grant {
                    account: account_row(&row)?,
                    client: row.try_get(7)?,
                    client_name: row.try_get(8)?,
                    agent: None,
                })
            })
            .transpose()?;
            let Some(mut grant) = found else {
                return Ok(None);
            };
            // An account nobody secured that went unused too long has ended, with its tokens.
            if self.lapsed(&grant.account, now) {
                return Ok(None);
            }
            match super::agents::oauth_agent_in(
                connection,
                &grant.account.tenant,
                &grant.account.id,
                &grant.client,
            )
            .await
            {
                Ok(agent) => grant.agent = agent.map(|agent| agent.principal()),
                Err(Failed::Refused(error))
                    if matches!(error.code.as_str(), "agent_disabled" | "not_found") =>
                {
                    return Ok(None);
                }
                Err(error) => return Err(error),
            }
            tenants::touch(connection, &mut grant.account, now).await?;
            // Its tenant's access, settled now: let in, if sign-up opened while it waited.
            let admitted = self.settle_in(connection, &mut grant.account, now).await?;
            Ok(Some((grant, admitted)))
        })?;
        Ok(verified.map(|(grant, admitted)| {
            self.announce(admitted);
            grant
        }))
    }
}
