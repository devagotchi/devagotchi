//! Source grant ordering, canonical broker matching and permanent retry affinity.
use super::*;
use sqlx::Row;
mod eligibility;
#[cfg(test)]
mod tests;

pub(crate) struct StartSelection<'a> {
    pub account: &'a str,
    pub actor: &'a Actor<'a>,
    pub machine: &'a MachineBinding,
    pub links: &'a Links,
    pub client: &'a str,
    pub boards: Option<&'a Arc<crate::agenthub::mcp::Boards>>,
    /// Empty means unrestricted; these come from verified credential limits.
    pub projects: &'a [String],
}
impl StartSelection<'_> {
    fn execution<'a>(&'a self, id: &'a str) -> LinkExecution<'a> {
        LinkExecution {
            account: self.account,
            actor: self.actor,
            board: None,
            links: self.links,
            grant_id: id,
            client: self.client,
        }
    }
}
struct Candidate {
    id: String,
    item: GrantItem,
    rank: i32,
    at: i64,
}
impl Access {
    /// The retry key never selects a new grant after its first admission, including
    /// when that original grant is exhausted, revoked or its operation has finished.
    async fn original_start_grant(
        &self,
        selection: &StartSelection<'_>,
        key: &str,
    ) -> BridgeResult<Option<String>> {
        transaction!(&self.accounts.store, |c| {
            let found = self
                .accounts
                .machine_actor_in(c, selection.account, selection.actor)
                .await?;
            Devices::new(self.accounts.store.clone())
                .check_access_in(c, Some(selection.account), selection.machine)
                .await?;
            let row = sqlx::query(
                "SELECT account,principal,grant_id FROM machine_admissions
                WHERE tenant_id=$1 AND machine=$2 AND generation=$3 AND operation_key=$4",
            )
            .bind(&found.tenant)
            .bind(&selection.machine.name)
            .bind(&selection.machine.generation)
            .bind(key)
            .fetch_optional(c)
            .await?;
            row.map(|row| {
                if row.try_get::<String, _>("account")? != selection.account
                    || row.try_get::<String, _>("principal")? != selection.actor.id
                {
                    return Err(refused(
                        "request_conflict",
                        "retry identity belongs to another principal",
                    ));
                }
                Ok(row.try_get("grant_id")?)
            })
            .transpose()
        })
    }
    async fn start_candidates(
        &self,
        selection: &StartSelection<'_>,
        capability: GrantCapability,
        after: &(i32, i64, String),
    ) -> BridgeResult<Vec<Candidate>> {
        transaction!(&self.accounts.store, |c| {
            let found = self
                .accounts
                .machine_actor_in(c, selection.account, selection.actor)
                .await?;
            Devices::new(self.accounts.store.clone())
                .check_access_in(c, Some(selection.account), selection.machine)
                .await?;
            let now = Self::now_in(c).await?;
            let encoded_capability = serde_json::to_value(capability)
                .map_err(|_| invalid("invalid start capability"))?;
            let capability = encoded_capability
                .as_str()
                .ok_or_else(|| invalid("start capability is not a name"))?;
            let rows = sqlx::query("WITH candidates AS (
                SELECT id,item,approved_at,CASE item->>'sandbox_level'
                    WHEN 'read-only' THEN 0 WHEN 'workspace-write' THEN 1 WHEN 'desktop' THEN 1 ELSE 2 END AS rank
                FROM machine_grants WHERE tenant_id=$1 AND account=$2 AND principal=$3
                    AND machine_binding=$4::jsonb AND (capability=$5 OR ($5='fs.read' AND capability='fs.write')) AND guide_binding IS NULL
                    AND ended_at IS NULL AND NOT paused AND (expires_at IS NULL OR expires_at>$6)
                    AND (uses_max IS NULL OR uses<uses_max))
                SELECT id,item::text,approved_at,rank FROM candidates
                WHERE (rank,approved_at,id)>($7,$8,$9) ORDER BY rank,approved_at,id LIMIT 100")
                .bind(&found.tenant).bind(selection.account).bind(selection.actor.id)
                .bind(encoded(selection.machine)?).bind(capability).bind(now)
                .bind(after.0).bind(after.1).bind(&after.2).fetch_all(c).await?;
            rows.into_iter()
                .map(|row| {
                    let item = serde_json::from_str(&row.try_get::<String, _>("item")?)
                        .map_err(|_| refused("access_damaged", "invalid grant selector"))?;
                    Ok(Candidate {
                        id: row.try_get("id")?,
                        item,
                        rank: row.try_get("rank")?,
                        at: row.try_get("approved_at")?,
                    })
                })
                .collect()
        })
    }
    /// Read-only preparation, then least-privileged matching grant in approval order.
    /// At most one broker preparation per confinement level, with bounded SQL pages.
    /// This is not permission: execute_prepared rechecks all live authority atomically.
    pub(crate) async fn prepare_selected_start(
        &self,
        selection: &StartSelection<'_>,
        input: StartInput,
    ) -> BridgeResult<PreparedCall> {
        self.prepare_selected_call(selection, PreparedInput::Start(input))
            .await
    }
    pub(crate) async fn prepare_selected_files(
        &self,
        selection: &StartSelection<'_>,
        input: FileInvocation,
    ) -> BridgeResult<PreparedCall> {
        input.validate()?;
        self.prepare_selected_call(selection, PreparedInput::Files(input))
            .await
    }
    async fn prepare_selected_call(
        &self,
        selection: &StartSelection<'_>,
        input: PreparedInput,
    ) -> BridgeResult<PreparedCall> {
        if let Some(id) = self.original_start_grant(selection, input.key()).await? {
            let target = self
                .execution_target(selection.account, selection.actor, &id)
                .await?;
            if !selection.project_allowed(target.project.as_deref()) {
                return Err(BridgeError::new(
                    "key_denied",
                    "the original retry grant is outside this key's project limits",
                ));
            }
            return self.prepare_call(&selection.execution(&id), input).await;
        }
        let capability = input.action().capability();
        let mut after = (-1, 0, String::new());
        let mut prepared: Vec<PreparedCall> = Vec::new();
        loop {
            let candidates = self.start_candidates(selection, capability, &after).await?;
            let count = candidates.len();
            for candidate in candidates {
                after = (candidate.rank, candidate.at, candidate.id.clone());
                let target = self
                    .execution_target(selection.account, selection.actor, &candidate.id)
                    .await?;
                if target.machine != *selection.machine
                    || target.level != candidate.item.sandbox_level
                {
                    return Err(BridgeError::new(
                        "authority_mismatch",
                        "grant target changed during selection",
                    ));
                }
                let action = input.action();
                if !self
                    .start_candidate_live(selection, &candidate.id, &target, action)
                    .await?
                {
                    continue;
                }
                let canonical = if let Some(cached) =
                    prepared.iter().find(|p| p.target.level == target.level)
                {
                    cached.clone()
                } else {
                    let p = self
                        .prepare_call(&selection.execution(&candidate.id), input.clone())
                        .await?;
                    prepared.push(p.clone());
                    p
                };
                match candidate.item.selector.permits(
                    candidate.item.capability,
                    target.level,
                    &canonical.call,
                ) {
                    Ok(()) => {
                        if !self
                            .start_candidate_live(selection, &candidate.id, &target, action)
                            .await?
                        {
                            continue;
                        }
                        return Ok(PreparedCall {
                            grant_id: candidate.id,
                            target,
                            ..canonical
                        });
                    }
                    Err(error) if error.code == "outside_grant" => (),
                    Err(error) => return Err(error),
                }
            }
            if count < 100 {
                return Err(BridgeError::new(
                    "grant_required",
                    "no live grant covers this call; use request_access",
                ));
            }
        }
    }
}
