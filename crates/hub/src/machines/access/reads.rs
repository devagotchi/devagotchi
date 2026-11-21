//! Scoped history projections. A decision or listing is never an execution permit.
use super::*;
use devagotchi_agenthub::Actor;
use serde_json::Value;

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AccessPageQuery {
    pub machine: Option<String>,
    pub principal: Option<String>,
    pub status: Option<String>,
    #[serde(default)]
    pub live: bool,
    pub after: Option<String>,
    pub limit: Option<u32>,
}
#[derive(Debug, Serialize)]
pub(crate) struct AccessPage {
    pub rows: Vec<Value>,
    pub next: Option<String>,
}
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct AccessStatus {
    pub request_id: String,
    pub machine: String,
    pub status: String,
    pub grant_id: Option<String>,
    pub granted: Option<Value>,
    pub note: Option<String>,
}
#[derive(Debug, Serialize)]
pub(crate) struct AccessLookup {
    pub status: String,
    pub items: Vec<AccessStatus>,
}
impl AccessPageQuery {
    fn bounds(&self, grants: bool) -> BridgeResult<(i64, String, i64)> {
        let limit = self.limit.unwrap_or(100);
        if !(1..=100).contains(&limit)
            || [&self.machine, &self.principal]
                .into_iter()
                .flatten()
                .any(|v| v.is_empty() || v.len() > 512 || v.contains('\0'))
            || grants && self.status.is_some()
            || !grants && self.live
            || self.status.as_deref().is_some_and(|s| {
                !matches!(
                    s,
                    "pending" | "approved" | "narrowed" | "denied" | "cancelled" | "expired"
                )
            })
        {
            return Err(invalid("invalid access list filters or page limit"));
        }
        let Some(after) = &self.after else {
            return Ok((i64::MAX, String::new(), i64::from(limit)));
        };
        let (at, id) = after
            .split_once(':')
            .ok_or_else(|| invalid("invalid access cursor"))?;
        let at: i64 = at.parse().map_err(|_| invalid("invalid access cursor"))?;
        if at < 0
            || id.is_empty()
            || id.len() > 64
            || !id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
        {
            return Err(invalid("invalid access cursor"));
        }
        Ok((at, id.into(), i64::from(limit)))
    }
}
fn page(raw: Vec<String>, limit: i64, timestamp: &str) -> DbResult<AccessPage> {
    let mut rows: Vec<Value> = raw
        .into_iter()
        .map(|s| {
            serde_json::from_str(&s)
                .map_err(|_| refused("access_damaged", "invalid access history record"))
        })
        .collect::<DbResult<_>>()?;
    let next = if rows.len() > limit as usize {
        rows.truncate(limit as usize);
        let last = rows.last().expect("positive page size");
        Some(format!(
            "{}:{}",
            last[timestamp]
                .as_i64()
                .ok_or_else(|| refused("access_damaged", "invalid history time"))?,
            last["id"]
                .as_str()
                .ok_or_else(|| refused("access_damaged", "invalid history id"))?
        ))
    } else {
        None
    };
    Ok(AccessPage { rows, next })
}
impl Access {
    pub(crate) async fn access_requests(
        &self,
        account: &str,
        actor: &Actor<'_>,
        query: &AccessPageQuery,
    ) -> BridgeResult<AccessPage> {
        let (at, id, limit) = query.bounds(false)?;
        transaction!(&self.accounts.store, |c| {
            let found = self.accounts.machine_actor_in(c, account, actor).await?;
            let all = actor.human && found.owner;
            let now = Self::now_in(c).await?;
            let rows: Vec<String> = sqlx::query_scalar("SELECT jsonb_build_object(
                'id',r.id,'group_id',r.group_id,'account',r.account,'principal',r.principal,'machine',r.machine,
                'task',r.task_binding,'guide',r.guide_binding,'prepared',r.prepared_item,'preset',r.preset_snapshot,
                'duration',r.duration,'uses_max',r.uses_max,'reason',r.reason,'collapsed',r.collapsed,
                'status',CASE WHEN r.status='pending' AND r.expires_at<=$4 THEN 'expired' ELSE r.status END,
                'created_at',r.created_at,'expires_at',r.expires_at,'version',r.version,'decided_by',r.decided_by,
                'decided_at',r.decided_at,'decision_note',r.decision_note,'grant_id',r.grant_id)::text
                FROM machine_access_requests r WHERE r.tenant_id=$1 AND ($2 OR r.account=$3)
                AND ($5 OR r.principal=$6) AND ($7::text IS NULL OR r.machine=$7)
                AND ($8::text IS NULL OR r.principal=$8)
                AND ($9::text IS NULL OR (CASE WHEN r.status='pending' AND r.expires_at<=$4 THEN 'expired' ELSE r.status END)=$9)
                AND (r.created_at,r.id)<($10,$11) ORDER BY r.created_at DESC,r.id DESC LIMIT $12")
                .bind(&found.tenant).bind(all).bind(account).bind(now).bind(actor.human).bind(actor.id)
                .bind(&query.machine).bind(&query.principal).bind(&query.status).bind(at).bind(&id).bind(limit+1)
                .fetch_all(c).await?;
            page(rows, limit, "created_at")
        })
    }
    pub(crate) async fn access_grants(
        &self,
        account: &str,
        actor: &Actor<'_>,
        query: &AccessPageQuery,
    ) -> BridgeResult<AccessPage> {
        let (at, id, limit) = query.bounds(true)?;
        transaction!(&self.accounts.store, |c| {
            let found = self.accounts.machine_actor_in(c, account, actor).await?;
            let all = actor.human && found.owner;
            let now = Self::now_in(c).await?;
            let rows: Vec<String> = sqlx::query_scalar("WITH scoped AS (
                SELECT g.*, (g.ended_at IS NULL AND (g.expires_at IS NULL OR g.expires_at>$4)
                    AND (g.uses_max IS NULL OR g.uses<g.uses_max OR EXISTS(SELECT 1 FROM machine_admissions a
                        WHERE a.tenant_id=g.tenant_id AND a.grant_id=g.id AND a.state='admitted'))) AS live
                FROM machine_grants g WHERE g.tenant_id=$1 AND ($2 OR g.account=$3) AND ($5 OR g.principal=$6)
                    AND ($7::text IS NULL OR g.machine=$7) AND ($8::text IS NULL OR g.principal=$8))
                SELECT jsonb_build_object('id',g.id,'account',g.account,'principal',g.principal,'machine',g.machine,
                'task',g.task_binding,'guide',g.guide_binding,'request_id',g.request_id,'item',g.item,
                'duration',g.duration,'uses_max',g.uses_max,'uses',g.uses,'preset_id',g.preset_id,
                'full_access',g.full_access,'risks',g.risk_snapshot,'approved_by',g.approved_by,'approved_at',g.approved_at,
                'expires_at',g.expires_at,'paused',g.paused,'ended_at',g.ended_at,'end_reason',g.end_reason,
                'ended_by',g.ended_by,'version',g.version,'live',g.live,
                'high_risk',g.full_access OR g.item->>'sandbox_level'='unsandboxed' OR g.capability LIKE 'gui.%'
                    OR g.risk_snapshot @> '[{\"severity\":\"high\"}]')::text
                FROM scoped g WHERE (NOT $9 OR g.live) AND (g.approved_at,g.id)<($10,$11)
                ORDER BY g.approved_at DESC,g.id DESC LIMIT $12")
                .bind(&found.tenant).bind(all).bind(account).bind(now).bind(actor.human).bind(actor.id)
                .bind(&query.machine).bind(&query.principal).bind(query.live).bind(at).bind(&id).bind(limit+1)
                .fetch_all(c).await?;
            page(rows, limit, "approved_at")
        })
    }
    /// Revalidate on every wait iteration; a long poll never retains a database transaction.
    pub(crate) async fn access_lookup(
        &self,
        account: &str,
        actor: &Actor<'_>,
        id: &str,
    ) -> BridgeResult<AccessLookup> {
        if id.is_empty() || id.len() > 64 || id.contains('\0') {
            return Err(invalid("invalid access request or group id"));
        }
        transaction!(&self.accounts.store, |c| {
            let found = self.accounts.machine_actor_in(c, account, actor).await?;
            let now = Self::now_in(c).await?;
            let raw: Vec<String> = sqlx::query_scalar("SELECT jsonb_build_object('request_id',r.id,'machine',r.machine,
                'status',CASE WHEN r.status='pending' AND r.expires_at<=$4 THEN 'expired' ELSE r.status END,
                'grant_id',r.grant_id,'note',r.decision_note,'granted',CASE WHEN g.id IS NULL THEN NULL ELSE
                jsonb_build_object('item',g.item,'duration',g.duration,'uses_max',g.uses_max,'uses',g.uses,
                    'expires_at',g.expires_at,'ended_at',g.ended_at,'end_reason',g.end_reason,'version',g.version) END)::text
                FROM machine_access_requests r LEFT JOIN machine_grants g ON g.tenant_id=r.tenant_id AND g.id=r.grant_id
                WHERE r.tenant_id=$1 AND ($2 OR r.account=$3) AND ($5 OR r.principal=$6)
                AND (r.id=$7 OR r.group_id=$7) ORDER BY r.created_at,r.id LIMIT 65")
                .bind(&found.tenant).bind(actor.human && found.owner).bind(account).bind(now)
                .bind(actor.human).bind(actor.id).bind(id).fetch_all(c).await?;
            if raw.is_empty() {
                return Err(refused("not_found", "access request not found"));
            }
            if raw.len() > 64 {
                return Err(refused(
                    "access_damaged",
                    "access group exceeds its item limit",
                ));
            }
            let items: Vec<AccessStatus> = raw
                .into_iter()
                .map(|s| {
                    serde_json::from_str(&s)
                        .map_err(|_| refused("access_damaged", "invalid access decision record"))
                })
                .collect::<DbResult<_>>()?;
            let status = if items.iter().any(|s| s.status == "pending") {
                "pending"
            } else if items
                .iter()
                .any(|s| matches!(s.status.as_str(), "approved" | "narrowed"))
            {
                "approved"
            } else {
                &items[0].status
            }
            .to_owned();
            Ok(AccessLookup { status, items })
        })
    }
}
