//! An authenticated agent resume warns each approving person without acting as a person.
use super::resume::{ResumeCheck, ids};
use super::*;
use devagotchi_agenthub::Actor;
use devagotchi_store::receipts::{self, Outcome, Request};
use serde_json::json;
use sqlx::Row;

impl Access {
    pub(crate) async fn agent_resumed(
        &self,
        account: &str,
        actor: &Actor<'_>,
        session: &str,
        after: Option<&str>,
        limit: u32,
        key: &str,
    ) -> BridgeResult<Changed<ResumeCheck>> {
        if actor.human
            || !(1..=100).contains(&limit)
            || after.is_some_and(|s| s.len() > 128 || s.contains('\0'))
            || session.is_empty()
            || session.len() > 128
            || !session
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        {
            return Err(invalid(
                "agent resume needs current agent identity and a bounded noncredential session label",
            ));
        }
        let ids = ids(limit)?;
        let original = json!({"session":session,"after":after,"limit":limit});
        transaction!(&self.accounts.store, |c| {
            let found = self.accounts.machine_actor_in(c, account, actor).await?;
            Self::lock_in(c, &found.tenant).await?;
            let request = Request::new(&found.tenant, actor.id, key, "resume_agent", &original)?;
            let reservation = match receipts::begin_in(c, &request).await? {
                Outcome::Replay(value) => {
                    return Ok(Changed {
                        result: serde_json::from_value(value).map_err(|_| {
                            refused("receipt_damaged", "invalid agent resume receipt")
                        })?,
                        replayed: true,
                    });
                }
                Outcome::Fresh(r) => r,
            };
            let now = Self::now_in(c).await?;
            let rows=sqlx::query("SELECT g.id,g.approved_by FROM machine_grants g WHERE g.tenant_id=$1 AND g.account=$2 AND g.principal=$3 AND g.id>$4
                AND g.ended_at IS NULL AND NOT g.paused AND (g.expires_at IS NULL OR g.expires_at>$5)
                AND (g.full_access OR g.item->>'sandbox_level'='unsandboxed'
                    OR (g.capability='gui.input' AND (g.item->'selector'->'apps' IS NULL OR g.item->'selector'->>'apps'='any')))
                AND (g.uses_max IS NULL OR g.uses<g.uses_max OR EXISTS(SELECT 1 FROM machine_admissions a WHERE a.tenant_id=g.tenant_id AND a.grant_id=g.id AND a.state='admitted'))
                ORDER BY g.id LIMIT $6")
                .bind(&found.tenant).bind(account).bind(actor.id).bind(after.unwrap_or("")).bind(now).bind(i64::from(limit)+1).fetch_all(&mut *c).await?;
            let more = rows.len() > limit as usize;
            let mut shown = 0;
            let mut last = None;
            for (row, id) in rows.into_iter().take(limit as usize).zip(&ids) {
                let grant: String = row.try_get("id")?;
                let person: String = row.try_get("approved_by")?;
                let added=sqlx::query("INSERT INTO machine_resume_alerts(tenant_id,id,grant_id,person,session_id,reason,shown_at,version)
                    VALUES($1,$2,$3,$4,$5,'agent_resumed',$6,1) ON CONFLICT(tenant_id,grant_id,person) WHERE acknowledged_at IS NULL DO NOTHING")
                    .bind(&found.tenant).bind(id).bind(&grant).bind(&person).bind(session).bind(now).execute(&mut *c).await?.rows_affected();
                if added == 1 {
                    shown += 1;
                    Self::audit_in(
                        c,
                        &found.tenant,
                        actor.id,
                        "resume.alert_shown",
                        &json!({"alert":id,"grant":grant,"human":person,"reason":"agent_resumed"}),
                        now,
                    )
                    .await?;
                }
                last = Some(grant);
            }
            let settings = Self::settings_in(c, &found.tenant).await?;
            let result = ResumeCheck {
                shown,
                next: if more { last } else { None },
                idle_minutes: settings.resume_alert_idle_minutes,
            };
            reservation.complete_in(c, &result).await?;
            Ok(Changed {
                result,
                replayed: false,
            })
        })
    }
}
