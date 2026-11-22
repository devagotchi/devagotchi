//! Current-person resume alerts expose immutable approved risk evidence, never authority.
use super::*;
use crate::identity::accounts::Account;
use devagotchi_agenthub::{
    Actor,
    policy::{Resource, Subject, catalog as policy},
};
use devagotchi_contracts::GrantItem;
use devagotchi_store::receipts::{self, Outcome, Request};
use serde_json::json;
use sqlx::Row;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ResumeReason {
    SignIn,
    Load,
    Reload,
    IdleReturn,
    AgentResumed,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ResumeCheck {
    pub shown: u32,
    pub next: Option<String>,
    pub idle_minutes: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ResumeAlert {
    pub id: String,
    pub version: i64,
    pub grant_id: String,
    pub reason: String,
    pub shown_at: i64,
    pub machine: String,
    pub principal: String,
    pub item: GrantItem,
    pub risks: serde_json::Value,
    pub approved_at: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ResumePage {
    pub alerts: Vec<ResumeAlert>,
    pub next: Option<String>,
}
fn page(after: Option<&str>, limit: u32) -> BridgeResult<()> {
    if !(1..=100).contains(&limit) || after.is_some_and(|s| s.len() > 128 || s.contains('\0')) {
        return Err(invalid("invalid resume page"));
    }
    Ok(())
}
pub(super) fn ids(limit: u32) -> BridgeResult<Vec<String>> {
    (0..limit)
        .map(|_| {
            let mut bytes = [0u8; 16];
            getrandom::fill(&mut bytes).map_err(|e| BridgeError::new("random", e.to_string()))?;
            Ok(hex::encode(bytes))
        })
        .collect()
}
impl Access {
    pub(super) async fn resume_person_in(
        &self,
        c: &mut AnyConnection,
        person: &str,
        action: &str,
    ) -> DbResult<Account> {
        let found = self
            .accounts
            .machine_actor_in(c, person, &Actor::human(person))
            .await?;
        policy::initialize_in(c, &found.tenant).await?;
        let roles = if found.owner {
            vec!["admin".into()]
        } else {
            vec![]
        };
        policy::authorize_in(
            c,
            &found.tenant,
            &Subject {
                id: person,
                human: true,
                kind: "human",
                roles: &roles,
            },
            action,
            &Resource::default(),
        )
        .await?;
        Ok(found)
    }
    /// `session` is a noncredential browser instance label, never the login cookie/token.
    pub(crate) async fn check_resume(
        &self,
        person: &str,
        session: &str,
        reason: ResumeReason,
        after: Option<&str>,
        limit: u32,
        key: &str,
    ) -> BridgeResult<Changed<ResumeCheck>> {
        page(after, limit)?;
        if session.is_empty()
            || session.len() > 128
            || !session
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        {
            return Err(invalid(
                "resume session needs a noncredential instance label",
            ));
        }
        let ids = ids(limit)?;
        let reason = serde_json::to_value(reason).expect("closed resume reason");
        let original = json!({"session":session,"reason":reason,"after":after,"limit":limit});
        transaction!(&self.accounts.store, |c| {
            let found = self.resume_person_in(c, person, "resume.check").await?;
            Self::lock_in(c, &found.tenant).await?;
            let request = Request::new(&found.tenant, person, key, "resume_check", &original)?;
            let reservation = match receipts::begin_in(c, &request).await? {
                Outcome::Replay(value) => {
                    return Ok(Changed {
                        result: serde_json::from_value(value)
                            .map_err(|_| refused("receipt_damaged", "invalid resume receipt"))?,
                        replayed: true,
                    });
                }
                Outcome::Fresh(r) => r,
            };
            let now = Self::now_in(c).await?;
            let grants:Vec<String>=sqlx::query_scalar("SELECT g.id FROM machine_grants g WHERE g.tenant_id=$1 AND g.id>$2
                AND ($3 OR g.account=$4 OR g.approved_by=$4) AND g.ended_at IS NULL AND NOT g.paused AND (g.expires_at IS NULL OR g.expires_at>$5)
                AND (g.full_access OR g.item->>'sandbox_level'='unsandboxed'
                    OR (g.capability='gui.input' AND (g.item->'selector'->'apps' IS NULL OR g.item->'selector'->>'apps'='any')))
                AND (g.uses_max IS NULL OR g.uses<g.uses_max OR EXISTS(SELECT 1 FROM machine_admissions a WHERE a.tenant_id=g.tenant_id AND a.grant_id=g.id AND a.state='admitted'))
                ORDER BY g.id LIMIT $6")
                .bind(&found.tenant).bind(after.unwrap_or("")).bind(found.owner).bind(person).bind(now).bind(i64::from(limit)+1).fetch_all(&mut *c).await?;
            let more = grants.len() > limit as usize;
            let mut shown = 0;
            for (grant, id) in grants.iter().take(limit as usize).zip(&ids) {
                let added=sqlx::query("INSERT INTO machine_resume_alerts(tenant_id,id,grant_id,person,session_id,reason,shown_at,version)
                    VALUES($1,$2,$3,$4,$5,$6,$7,1) ON CONFLICT(tenant_id,grant_id,person) WHERE acknowledged_at IS NULL DO NOTHING")
                    .bind(&found.tenant).bind(id).bind(grant).bind(person).bind(session).bind(reason.as_str().expect("resume reason")).bind(now).execute(&mut *c).await?.rows_affected();
                if added == 1 {
                    shown += 1;
                    Self::audit_in(
                        c,
                        &found.tenant,
                        person,
                        "resume.alert_shown",
                        &json!({"alert":id,"grant":grant,"reason":reason}),
                        now,
                    )
                    .await?;
                }
            }
            let settings = Self::settings_in(c, &found.tenant).await?;
            let result = ResumeCheck {
                shown,
                next: more.then(|| grants[limit as usize - 1].clone()),
                idle_minutes: settings.resume_alert_idle_minutes,
            };
            reservation.complete_in(c, &result).await?;
            Ok(Changed {
                result,
                replayed: false,
            })
        })
    }
    pub(crate) async fn resume_alerts(
        &self,
        person: &str,
        after: Option<&str>,
        limit: u32,
    ) -> BridgeResult<ResumePage> {
        page(after, limit)?;
        transaction!(&self.accounts.store, |c| {
            let found = self.resume_person_in(c, person, "resume.read").await?;
            let now = Self::now_in(c).await?;
            let rows=sqlx::query("SELECT a.id,a.version,a.grant_id,a.reason,a.shown_at,g.machine,g.principal,g.item::text,g.risk_snapshot::text,g.approved_at
                FROM machine_resume_alerts a JOIN machine_grants g ON g.tenant_id=a.tenant_id AND g.id=a.grant_id
                WHERE a.tenant_id=$1 AND a.person=$2 AND a.id>$3 AND a.acknowledged_at IS NULL AND g.ended_at IS NULL
                AND (g.expires_at IS NULL OR g.expires_at>$4) ORDER BY a.id LIMIT $5")
                .bind(&found.tenant).bind(person).bind(after.unwrap_or("")).bind(now).bind(i64::from(limit)+1).fetch_all(c).await?;
            let more = rows.len() > limit as usize;
            let alerts: Vec<ResumeAlert> = rows
                .into_iter()
                .take(limit as usize)
                .map(|row| {
                    Ok(ResumeAlert {
                        id: row.try_get("id")?,
                        version: row.try_get("version")?,
                        grant_id: row.try_get("grant_id")?,
                        reason: row.try_get("reason")?,
                        shown_at: row.try_get("shown_at")?,
                        machine: row.try_get("machine")?,
                        principal: row.try_get("principal")?,
                        approved_at: row.try_get("approved_at")?,
                        item: serde_json::from_str(&row.try_get::<String, _>("item")?)
                            .map_err(|_| refused("access_damaged", "invalid alert grant scope"))?,
                        risks: serde_json::from_str(&row.try_get::<String, _>("risk_snapshot")?)
                            .map_err(|_| {
                                refused("access_damaged", "invalid alert risk snapshot")
                            })?,
                    })
                })
                .collect::<DbResult<_>>()?;
            let next = if more {
                alerts.last().map(|a| a.id.clone())
            } else {
                None
            };
            Ok(ResumePage { alerts, next })
        })
    }
}
