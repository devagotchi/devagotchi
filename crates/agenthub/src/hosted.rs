//! Native hosted-board transactions. Never opens a local board or falls back to SQLite.

use devagotchi_contracts::BridgeResult;
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};
use sqlx::{AnyConnection, Row};

use super::{
    Actor, Event, GENESIS, NewTicket, Note, Settings, Ticket, Workflow, check_environment,
    clean_anchor, clean_labels, default_prefix, event_hash, fail, invalid, lease::Lease,
};
use devagotchi_store::{Backend, Failed, Store, transaction};

mod approvals;
mod inbox;
mod issues;
mod operations;

#[derive(Clone)]
pub struct HostedBoard {
    store: Store,
    tenant: String,
    project: String,
    pub name: String,
    owner: Option<String>,
    runtime: tokio::runtime::Handle,
}

type DbResult<T> = std::result::Result<T, Failed>;

impl HostedBoard {
    /// The tenant whose rows this board's are.
    pub fn tenant(&self) -> &str {
        &self.tenant
    }

    /// The board's id in its tenant: what its events carry (`board_id`).
    pub fn board_id(&self) -> &str {
        &self.project
    }
}

fn decode<T: DeserializeOwned>(text: &str) -> BridgeResult<T> {
    serde_json::from_str(text).map_err(|error| {
        fail(
            "board_damaged",
            format!("invalid stored board data: {error}"),
        )
    })
}

impl HostedBoard {
    /// Tenant/project must come from the authenticated resolver, never a tool payload.
    pub fn new(store: Store, tenant: &str, project: &str) -> BridgeResult<Self> {
        if store.backend() != Backend::Postgres {
            return Err(fail("invalid_config", "hosted boards require Postgres"));
        }
        let internal = project.strip_prefix("b-").is_some_and(|hash| {
            hash.len() == 64
                && hash
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        });
        if !devagotchi_store::valid_tenant_id(tenant) || (!internal && !crate::valid_name(project))
        {
            return Err(invalid("hosted board requires a valid tenant and project"));
        }
        Ok(Self {
            store,
            tenant: tenant.into(),
            project: project.into(),
            name: project.into(),
            owner: None,
            runtime: tokio::runtime::Handle::try_current()
                .map_err(|error| fail("runtime", error.to_string()))?,
        })
    }

    pub fn for_owner(store: Store, tenant: &str, owner: &str, project: &str) -> BridgeResult<Self> {
        let mut board = Self::new(store, tenant, project)?;
        if owner.is_empty() || owner.len() > 256 {
            return Err(invalid("invalid board owner"));
        }
        board.project = format!(
            "b-{}",
            hex::encode(Sha256::digest(format!("{owner}\0{project}").as_bytes()))
        );
        board.owner = Some(owner.into());
        Ok(board)
    }

    // MCP/reaper use blocking workers; Tokio compensates for synchronous web callers.
    pub fn blocking<T>(
        &self,
        future: impl std::future::Future<Output = BridgeResult<T>>,
    ) -> BridgeResult<T> {
        blocking(&self.runtime, future)
    }

    pub async fn prefix(&self) -> BridgeResult<String> {
        transaction!(&self.store, |connection| {
            self.scope(connection).await?;
            Ok(sqlx::query_scalar(
                "SELECT value FROM board_metadata WHERE tenant_id=$1 AND board_id=$2 AND key='prefix'",
            ).bind(&self.tenant).bind(&self.project).fetch_one(&mut *connection).await?)
        })
    }

    pub async fn database_now(&self) -> BridgeResult<i64> {
        transaction!(&self.store, |connection| { self.scope(connection).await })
    }

    pub fn matches_notification(&self, tenant: &str, board: &str) -> bool {
        self.tenant == tenant && self.project == board
    }

    pub async fn initialize(&self) -> BridgeResult<bool> {
        self.initialize_limited(None).await
    }

    pub async fn initialize_limited(&self, limit: Option<usize>) -> BridgeResult<bool> {
        transaction!(&self.store, |connection| {
            self.store.scope(connection, &self.tenant).await?;
            sqlx::query("SELECT id FROM tenancy_tenants WHERE id=$1 FOR UPDATE")
                .bind(&self.tenant)
                .fetch_one(&mut *connection)
                .await?;
            if let (Some(limit), Some(owner)) = (limit, &self.owner) {
                let exists: i64 = sqlx::query_scalar(
                    "SELECT count(*) FROM board_projects WHERE tenant_id=$1 AND id=$2",
                )
                .bind(&self.tenant)
                .bind(&self.project)
                .fetch_one(&mut *connection)
                .await?;
                if exists == 0 {
                    let count: i64 = sqlx::query_scalar(
                        "SELECT count(*) FROM board_projects WHERE tenant_id=$1 AND owner_id=$2",
                    )
                    .bind(&self.tenant)
                    .bind(owner)
                    .fetch_one(&mut *connection)
                    .await?;
                    if count >= i64::try_from(limit).unwrap_or(i64::MAX) {
                        return Err(fail("plan_limit",format!("this account or org already holds {count} project(s); limit {limit}")).into());
                    }
                }
            }
            let now = Self::server_now(connection).await?;
            let created = sqlx::query(
                "INSERT INTO board_projects (tenant_id,id,source_hash,imported_at,owner_id,project_name)
                 VALUES ($1,$2,'native',$3,$4,$5) ON CONFLICT (tenant_id,id) DO NOTHING",
            )
            .bind(&self.tenant)
            .bind(&self.project)
            .bind(now)
            .bind(&self.owner)
            .bind(&self.name)
            .execute(&mut *connection)
            .await?
            .rows_affected()
                == 1;
            if created {
                for (key, value) in [
                    ("schema_version", super::SCHEMA_VERSION.to_string()),
                    ("project", self.name.clone()),
                    ("prefix", default_prefix(&self.name)),
                    ("created_at", now.to_string()),
                ] {
                    sqlx::query(
                        "INSERT INTO board_metadata (tenant_id,board_id,key,value) VALUES ($1,$2,$3,$4)",
                    ).bind(&self.tenant).bind(&self.project).bind(key).bind(value)
                        .execute(&mut *connection).await?;
                }
                for (key, value) in [
                    (
                        "settings",
                        serde_json::to_string(&Settings::default()).expect("settings json"),
                    ),
                    (
                        "workflow",
                        serde_json::to_string(&Workflow::default_workflow())
                            .expect("workflow json"),
                    ),
                ] {
                    sqlx::query(
                        "INSERT INTO board_settings (tenant_id,board_id,key,value) VALUES ($1,$2,$3,$4)",
                    ).bind(&self.tenant).bind(&self.project).bind(key).bind(value)
                        .execute(&mut *connection).await?;
                }
                sqlx::query(
                    "INSERT INTO board_fence (tenant_id,board_id,id,value) VALUES ($1,$2,1,0)",
                )
                .bind(&self.tenant)
                .bind(&self.project)
                .execute(&mut *connection)
                .await?;
            }
            Ok(created)
        })
    }

    async fn server_now(connection: &mut AnyConnection) -> DbResult<i64> {
        Ok(
            sqlx::query_scalar(
                "SELECT floor(extract(epoch FROM clock_timestamp()) * 1000)::bigint",
            )
            .fetch_one(&mut *connection)
            .await?,
        )
    }

    async fn scope(&self, connection: &mut AnyConnection) -> DbResult<i64> {
        self.store.scope(connection, &self.tenant).await?;
        // One database lock orders events, fences, WIP and graph changes across hub instances.
        let project: Option<String> = sqlx::query_scalar(
            "SELECT id FROM board_projects WHERE tenant_id=$1 AND id=$2 FOR UPDATE",
        )
        .bind(&self.tenant)
        .bind(&self.project)
        .fetch_optional(&mut *connection)
        .await?;
        if project.is_none() {
            return Err(fail("not_found", "hosted board does not exist").into());
        }
        Self::server_now(connection).await
    }

    async fn configuration<T: DeserializeOwned>(
        &self,
        connection: &mut AnyConnection,
        key: &str,
    ) -> DbResult<T> {
        let text: String = sqlx::query_scalar(
            "SELECT value FROM board_settings WHERE tenant_id=$1 AND board_id=$2 AND key=$3",
        )
        .bind(&self.tenant)
        .bind(&self.project)
        .bind(key)
        .fetch_one(&mut *connection)
        .await?;
        Ok(decode(&text)?)
    }

    async fn fresh_id(&self, connection: &mut AnyConnection) -> DbResult<String> {
        let prefix: String = sqlx::query_scalar(
            "SELECT value FROM board_metadata WHERE tenant_id=$1 AND board_id=$2 AND key='prefix'",
        )
        .bind(&self.tenant)
        .bind(&self.project)
        .fetch_one(&mut *connection)
        .await?;
        let mut random = [0; 16];
        getrandom::fill(&mut random).map_err(|error| fail("board_random", error.to_string()))?;
        let suffix = hex::encode(random);
        for length in 4..=12 {
            let id = format!("{prefix}-{}", &suffix[..length]);
            let count: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM board_tickets WHERE tenant_id=$1 AND board_id=$2 AND id=$3",
            )
            .bind(&self.tenant)
            .bind(&self.project)
            .bind(&id)
            .fetch_one(&mut *connection)
            .await?;
            if count == 0 {
                return Ok(id);
            }
        }
        Err(fail("board_id", "could not allocate a unique id").into())
    }

    async fn ticket(
        &self,
        connection: &mut AnyConnection,
        id: &str,
        workflow: &Workflow,
        now: i64,
    ) -> DbResult<Ticket> {
        let text: Option<String> = sqlx::query_scalar(
            "SELECT row_to_json(t)::text FROM board_tickets t
             WHERE tenant_id=$1 AND board_id=$2 AND id=$3",
        )
        .bind(&self.tenant)
        .bind(&self.project)
        .bind(id)
        .fetch_optional(&mut *connection)
        .await?;
        let text = text.ok_or_else(|| fail("not_found", format!("ticket {id} not found")))?;
        let mut value: serde_json::Value = decode(&text)?;
        for field in ["labels", "progress"] {
            if let Some(text) = value[field].as_str() {
                value[field] = decode(text)?;
            }
        }
        value["blocked_by"] = serde_json::json!([]);
        value["lease_expired"] = serde_json::json!(
            value["lease_expires_at"]
                .as_i64()
                .is_some_and(|at| at <= now)
        );
        let mut ticket: Ticket = serde_json::from_value(value)
            .map_err(|error| fail("board_damaged", error.to_string()))?;
        let blockers = sqlx::query(
            "SELECT d.depends_on,t.state FROM board_deps d JOIN board_tickets t
               ON t.tenant_id=d.tenant_id AND t.board_id=d.board_id AND t.id=d.depends_on
             WHERE d.tenant_id=$1 AND d.board_id=$2 AND d.ticket=$3 AND d.kind='blocks'
             ORDER BY d.depends_on COLLATE \"C\"",
        )
        .bind(&self.tenant)
        .bind(&self.project)
        .bind(id)
        .fetch_all(&mut *connection)
        .await?;
        for blocker in blockers {
            if !workflow.is_done(&blocker.try_get::<String, _>("state")?) {
                ticket.blocked_by.push(blocker.try_get("depends_on")?);
            }
        }
        Ok(ticket)
    }

    pub async fn get(&self, id: &str) -> BridgeResult<Ticket> {
        transaction!(&self.store, |connection| {
            let now = self.scope(connection).await?;
            let workflow = self.configuration(connection, "workflow").await?;
            self.ticket(connection, id, &workflow, now).await
        })
    }

    async fn event(
        &self,
        connection: &mut AnyConnection,
        actor: &str,
        now: i64,
        action: &str,
        ticket: Option<&str>,
        fence: Option<i64>,
        data: serde_json::Value,
    ) -> DbResult<()> {
        let last = sqlx::query(
            "SELECT seq,hash FROM board_events WHERE tenant_id=$1 AND board_id=$2 ORDER BY seq DESC LIMIT 1",
        ).bind(&self.tenant).bind(&self.project).fetch_optional(&mut *connection).await?;
        let (seq, prev) = match last {
            Some(row) => (
                row.try_get::<i64, _>("seq")? + 1,
                row.try_get::<String, _>("hash")?,
            ),
            None => (1, GENESIS.to_string()),
        };
        let mut event = Event {
            seq,
            at: now,
            actor: actor.into(),
            action: action.into(),
            ticket: ticket.map(str::to_string),
            data,
            fence,
            prev_hash: prev.clone(),
            hash: String::new(),
        };
        event.hash = event_hash(&prev, &event);
        sqlx::query(
            "INSERT INTO board_events (tenant_id,board_id,seq,at,actor,action,ticket,data,fence,prev_hash,hash)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
        ).bind(&self.tenant).bind(&self.project).bind(seq).bind(now).bind(actor).bind(action)
            .bind(ticket).bind(event.data.to_string()).bind(fence).bind(prev).bind(&event.hash)
            .execute(&mut *connection).await?;
        sqlx::query(
            "INSERT INTO board_outbox (tenant_id,board_id,event_id,kind,payload,created_at)
             VALUES ($1,$2,$3,$4,$5,$6)",
        )
        .bind(&self.tenant)
        .bind(&self.project)
        .bind(format!("event:{seq}:{}", event.hash))
        .bind(action)
        .bind(serde_json::to_string(&event).expect("event json"))
        .bind(now)
        .execute(&mut *connection)
        .await?;
        sqlx::query("SELECT pg_notify('devagotchi_board_changes',$1)")
            .bind(serde_json::json!([self.tenant, self.project]).to_string())
            .execute(&mut *connection)
            .await?;
        Ok(())
    }

    pub async fn events(&self) -> BridgeResult<Vec<Event>> {
        self.events_page(0, usize::MAX).await
    }

    pub async fn events_page(&self, after: i64, limit: usize) -> BridgeResult<Vec<Event>> {
        transaction!(&self.store, |connection| {
            self.scope(connection).await?;
            let rows: Vec<String> = sqlx::query_scalar(
                "SELECT row_to_json(e)::text FROM board_events e
                 WHERE tenant_id=$1 AND board_id=$2 AND seq>$3 ORDER BY seq LIMIT $4",
            )
            .bind(&self.tenant)
            .bind(&self.project)
            .bind(after)
            .bind(i64::try_from(limit.max(1)).unwrap_or(i64::MAX))
            .fetch_all(&mut *connection)
            .await?;
            let mut events = Vec::with_capacity(rows.len());
            for text in rows {
                let mut value: serde_json::Value = decode(&text)?;
                let data = value["data"]
                    .as_str()
                    .ok_or_else(|| fail("board_damaged", "invalid event data"))?;
                value["data"] = decode(data)?;
                events.push(
                    serde_json::from_value(value)
                        .map_err(|error| fail("board_damaged", error.to_string()))?,
                );
            }
            Ok(events)
        })
    }

    pub async fn create(&self, actor: &Actor<'_>, new: &NewTicket) -> BridgeResult<Ticket> {
        transaction!(&self.store, |connection| {
            let now = self.scope(connection).await?;
            self.create_in(connection, actor, new, now).await
        })
    }

    async fn create_in(
        &self,
        connection: &mut AnyConnection,
        actor: &Actor<'_>,
        new: &NewTicket,
        now: i64,
    ) -> DbResult<Ticket> {
        let title = new.title.trim();
        if title.is_empty() || title.chars().count() > 300 {
            return Err(invalid("title must be 1 to 300 characters").into());
        }
        let kind = new.kind.as_deref().unwrap_or("task");
        if !matches!(kind, "task" | "epic" | "message") {
            return Err(invalid("kind must be task, epic or message").into());
        }
        let occurrences = new.occurrences.unwrap_or(1);
        if occurrences < 1 {
            return Err(invalid("occurrences must be at least 1").into());
        }
        let workflow: Workflow = self.configuration(connection, "workflow").await?;
        let settings: Settings = self.configuration(connection, "settings").await?;
        let environment = clean_anchor(&new.anchors.environment);
        check_environment(&settings, environment.as_deref())?;
        let id = if let Some(parent) = &new.parent {
            let parent_ticket = self.ticket(connection, parent, &workflow, now).await?;
            if workflow.is_done(&parent_ticket.state) {
                return Err(fail("parent_closed", format!("{parent} is closed")).into());
            }
            let next: i64 = sqlx::query_scalar(
                "UPDATE board_tickets SET child_seq=child_seq+1
                     WHERE tenant_id=$1 AND board_id=$2 AND id=$3 RETURNING child_seq",
            )
            .bind(&self.tenant)
            .bind(&self.project)
            .bind(parent)
            .fetch_one(&mut *connection)
            .await?;
            format!("{parent}.{next}")
        } else {
            self.fresh_id(connection).await?
        };
        if let Some(from) = &new.discovered_from {
            self.ticket(connection, from, &workflow, now).await?;
        }
        sqlx::query(
                "INSERT INTO board_tickets
                 (tenant_id,board_id,id,kind,title,description,state,priority,labels,parent,assignee,
                  created_by,created_at,updated_at,machine,repo,environment,agent,session,fingerprint,occurrences,last_seen_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$13,$14,$15,$16,$17,$18,$19,$20,$21)",
            ).bind(&self.tenant).bind(&self.project).bind(&id).bind(kind).bind(title)
                .bind(&new.description).bind(&workflow.initial_state)
                .bind(new.priority.unwrap_or(settings.default_priority))
                .bind(serde_json::to_string(&clean_labels(&new.labels)).expect("labels json"))
                .bind(&new.parent).bind(&new.assignee).bind(actor.id).bind(now)
                .bind(clean_anchor(&new.anchors.machine)).bind(clean_anchor(&new.anchors.repo))
                .bind(environment).bind(clean_anchor(&new.anchors.agent))
                .bind(clean_anchor(&new.anchors.session)).bind(clean_anchor(&new.anchors.fingerprint))
                .bind(occurrences).bind(new.last_seen_at).execute(&mut *connection).await?;
        for (target, kind) in [
            (&new.parent, "parent"),
            (&new.discovered_from, "discovered-from"),
        ] {
            if let Some(target) = target {
                sqlx::query(
                        "INSERT INTO board_deps (tenant_id,board_id,ticket,depends_on,kind,created_by,created_at)
                         VALUES ($1,$2,$3,$4,$5,$6,$7)",
                    ).bind(&self.tenant).bind(&self.project).bind(&id).bind(target).bind(kind)
                        .bind(actor.id).bind(now).execute(&mut *connection).await?;
            }
        }
        self.event(
            connection,
            actor.id,
            now,
            "ticket.created",
            Some(&id),
            None,
            serde_json::json!({"title":title,"state":workflow.initial_state,"parent":new.parent}),
        )
        .await?;
        self.ticket(connection, &id, &workflow, now).await
    }

    async fn clear_lease(
        &self,
        connection: &mut AnyConnection,
        actor: &str,
        now: i64,
        ticket: &Ticket,
        to: &str,
        action: &str,
        mut data: serde_json::Value,
    ) -> DbResult<()> {
        sqlx::query(
            "UPDATE board_tickets SET holder=NULL,lease_id=NULL,lease_ttl_ms=NULL,lease_expires_at=NULL,
             state=$4,version=version+1,updated_at=$5 WHERE tenant_id=$1 AND board_id=$2 AND id=$3",
        ).bind(&self.tenant).bind(&self.project).bind(&ticket.id).bind(to).bind(now)
            .execute(&mut *connection).await?;
        data["from"] = serde_json::json!(ticket.state);
        data["to"] = serde_json::json!(to);
        self.event(
            connection,
            actor,
            now,
            action,
            Some(&ticket.id),
            Some(ticket.fence),
            data,
        )
        .await
    }

    async fn expire(
        &self,
        connection: &mut AnyConnection,
        actor: &str,
        now: i64,
        ticket: &Ticket,
        workflow: &Workflow,
        settings: &Settings,
    ) -> DbResult<()> {
        let count = ticket.expiry_count + 1;
        let mut to = super::lease::release_state(workflow, ticket);
        if settings.max_lease_expiries > 0 && count >= settings.max_lease_expiries {
            to = workflow.attention_state.clone();
        }
        sqlx::query(
            "UPDATE board_tickets SET expiry_count=$4 WHERE tenant_id=$1 AND board_id=$2 AND id=$3",
        )
        .bind(&self.tenant)
        .bind(&self.project)
        .bind(&ticket.id)
        .bind(count)
        .execute(&mut *connection)
        .await?;
        self.clear_lease(
            connection,
            actor,
            now,
            ticket,
            &to,
            "lease.expired",
            serde_json::json!({"previous_holder":ticket.holder,"expiry_count":count}),
        )
        .await
    }

    pub async fn claim(
        &self,
        actor: &Actor<'_>,
        id: &str,
        ttl_ms: Option<i64>,
    ) -> BridgeResult<Lease> {
        transaction!(&self.store, |connection| {
            let now = self.scope(connection).await?;
            let workflow: Workflow = self.configuration(connection, "workflow").await?;
            let settings: Settings = self.configuration(connection, "settings").await?;
            self.claim_in(connection, actor, id, ttl_ms, now, &workflow, &settings)
                .await
        })
    }

    async fn claim_in(
        &self,
        connection: &mut AnyConnection,
        actor: &Actor<'_>,
        id: &str,
        ttl_ms: Option<i64>,
        now: i64,
        workflow: &Workflow,
        settings: &Settings,
    ) -> DbResult<Lease> {
        let mut ticket = self.ticket(connection, id, &workflow, now).await?;
        if ticket.holder.is_some() && !ticket.lease_expired {
            if ticket.holder.as_deref() == Some(actor.id) {
                return Ok(lease(ticket, &settings, true)?);
            }
            return Err(fail(
                "held_by_other",
                format!("{id} is leased by another principal"),
            )
            .into());
        }
        if ticket.holder.is_some() {
            self.expire(connection, actor.id, now, &ticket, &workflow, &settings)
                .await?;
            ticket = self.ticket(connection, id, &workflow, now).await?;
        }
        let lane = workflow.lane_by_queue(&ticket.state).ok_or_else(|| {
            fail(
                "not_claimable",
                format!("state {:?} is not a claimable queue", ticket.state),
            )
        })?;
        if !ticket.blocked_by.is_empty() {
            return Err(fail(
                "blocked",
                format!("waiting on {}", ticket.blocked_by.join(", ")),
            )
            .into());
        }
        if lane.distinct_from_previous && ticket.last_worker.as_deref() == Some(actor.id) {
            return Err(fail(
                "distinct_required",
                "this lane requires a different principal",
            )
            .into());
        }
        let limit = actor.wip_limit.unwrap_or(settings.wip_limit_default);
        if limit > 0 {
            let held: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM board_tickets
                     WHERE tenant_id=$1 AND board_id=$2 AND holder=$3 AND lease_expires_at>$4",
            )
            .bind(&self.tenant)
            .bind(&self.project)
            .bind(actor.id)
            .bind(now)
            .fetch_one(&mut *connection)
            .await?;
            if held >= limit {
                return Err(fail(
                    "wip_limit",
                    format!("you already hold {held} ticket(s); the WIP limit is {limit}"),
                )
                .into());
            }
        }
        let ttl = super::lease::clamp_ttl(&settings, ttl_ms);
        let fence: i64 = sqlx::query_scalar(
                "UPDATE board_fence SET value=value+1 WHERE tenant_id=$1 AND board_id=$2 AND id=1 RETURNING value",
            ).bind(&self.tenant).bind(&self.project).fetch_one(&mut *connection).await?;
        let mut random = [0; 16];
        getrandom::fill(&mut random).map_err(|error| fail("board_random", error.to_string()))?;
        sqlx::query(
            "UPDATE board_tickets SET holder=$4,lease_id=$5,lease_ttl_ms=$6,lease_expires_at=$7,
                 heartbeat_at=$8,fence=$9,state=$10,version=version+1,updated_at=$8
                 WHERE tenant_id=$1 AND board_id=$2 AND id=$3",
        )
        .bind(&self.tenant)
        .bind(&self.project)
        .bind(id)
        .bind(actor.id)
        .bind(hex::encode(random))
        .bind(ttl)
        .bind(now + ttl)
        .bind(now)
        .bind(fence)
        .bind(&lane.active)
        .execute(&mut *connection)
        .await?;
        self.event(connection, actor.id, now, "ticket.claimed", Some(id), Some(fence),
                serde_json::json!({"from":ticket.state,"to":lane.active,"ttl_ms":ttl,"lease_expires_at":now+ttl})).await?;
        Ok(lease(
            self.ticket(connection, id, &workflow, now).await?,
            &settings,
            false,
        )?)
    }

    pub async fn heartbeat(
        &self,
        actor: &Actor<'_>,
        id: &str,
        fence: i64,
        progress: Option<serde_json::Value>,
    ) -> BridgeResult<Lease> {
        transaction!(&self.store, |connection| {
            let now = self.scope(connection).await?;
            let workflow = self.configuration(connection, "workflow").await?;
            let settings: Settings = self.configuration(connection, "settings").await?;
            let ticket = self.ticket(connection, id, &workflow, now).await?;
            super::lease::check_held(&ticket, actor.id, Some(fence))?;
            let ttl = ticket.lease_ttl_ms.unwrap_or(settings.lease_ttl_ms);
            sqlx::query(
                "UPDATE board_tickets SET lease_expires_at=$4,heartbeat_at=$5,
                 progress=coalesce($6,progress),updated_at=$5 WHERE tenant_id=$1 AND board_id=$2 AND id=$3",
            ).bind(&self.tenant).bind(&self.project).bind(id).bind(now+ttl).bind(now)
                .bind(progress.as_ref().map(serde_json::Value::to_string))
                .execute(&mut *connection).await?;
            self.event(
                connection,
                actor.id,
                now,
                "ticket.heartbeat",
                Some(id),
                Some(fence),
                serde_json::json!({"lease_expires_at":now+ttl,"progress":progress}),
            )
            .await?;
            Ok(lease(
                self.ticket(connection, id, &workflow, now).await?,
                &settings,
                true,
            )?)
        })
    }

    async fn insert_note(
        &self,
        connection: &mut AnyConnection,
        actor: &str,
        now: i64,
        id: &str,
        kind: &str,
        body: &str,
        to: Option<&str>,
        fence: Option<i64>,
    ) -> DbResult<Note> {
        let seq: i64 = sqlx::query_scalar(
            "SELECT coalesce(max(seq),0)+1 FROM board_notes WHERE tenant_id=$1 AND board_id=$2",
        )
        .bind(&self.tenant)
        .bind(&self.project)
        .fetch_one(&mut *connection)
        .await?;
        sqlx::query(
            "INSERT INTO board_notes (tenant_id,board_id,seq,ticket,author,kind,body,to_principal,fence,created_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
        ).bind(&self.tenant).bind(&self.project).bind(seq).bind(id).bind(actor).bind(kind)
            .bind(body).bind(to).bind(fence).bind(now).execute(&mut *connection).await?;
        Ok(Note {
            seq,
            ticket: id.into(),
            author: actor.into(),
            kind: kind.into(),
            body: body.into(),
            to_principal: to.map(str::to_string),
            fence,
            created_at: now,
        })
    }

    pub async fn add_note(
        &self,
        actor: &Actor<'_>,
        id: &str,
        kind: &str,
        body: &str,
        to: Option<&str>,
        fence: Option<i64>,
    ) -> BridgeResult<Note> {
        let body = body.trim();
        if body.is_empty() || body.chars().count() > 20_000 {
            return Err(invalid("note body must be 1 to 20000 characters"));
        }
        let kind = if kind.trim().is_empty() {
            "note"
        } else {
            kind.trim()
        };
        transaction!(&self.store, |connection| {
            let now = self.scope(connection).await?;
            let workflow = self.configuration(connection, "workflow").await?;
            let ticket = self.ticket(connection, id, &workflow, now).await?;
            if ticket.holder.as_deref() == Some(actor.id) && !ticket.lease_expired {
                super::lease::check_held(&ticket, actor.id, fence)?;
            }
            let note = self
                .insert_note(connection, actor.id, now, id, kind, body, to, fence)
                .await?;
            self.event(
                connection,
                actor.id,
                now,
                "note.added",
                Some(id),
                fence,
                serde_json::json!({"seq":note.seq,"kind":kind,"to":to}),
            )
            .await?;
            Ok(note)
        })
    }

    pub async fn complete(
        &self,
        actor: &Actor<'_>,
        id: &str,
        fence: i64,
        summary: &str,
    ) -> BridgeResult<Ticket> {
        transaction!(&self.store, |connection| {
            let now = self.scope(connection).await?;
            let workflow: Workflow = self.configuration(connection, "workflow").await?;
            let ticket = self.ticket(connection, id, &workflow, now).await?;
            super::lease::check_held(&ticket, actor.id, Some(fence))?;
            let to = workflow
                .lane_by_active(&ticket.state)
                .map(|lane| lane.complete_to.clone())
                .unwrap_or_else(|| workflow.initial_state.clone());
            if !summary.trim().is_empty() {
                self.insert_note(
                    connection,
                    actor.id,
                    now,
                    id,
                    "summary",
                    summary.trim(),
                    None,
                    Some(fence),
                )
                .await?;
            }
            sqlx::query(
                "UPDATE board_tickets SET last_worker=$4,closed_at=coalesce($5,closed_at)
                 WHERE tenant_id=$1 AND board_id=$2 AND id=$3",
            )
            .bind(&self.tenant)
            .bind(&self.project)
            .bind(id)
            .bind(actor.id)
            .bind(workflow.is_done(&to).then_some(now))
            .execute(&mut *connection)
            .await?;
            self.clear_lease(
                connection,
                actor.id,
                now,
                &ticket,
                &to,
                "ticket.completed",
                serde_json::json!({"summary":!summary.trim().is_empty()}),
            )
            .await?;
            self.ticket(connection, id, &workflow, now).await
        })
    }

    pub async fn release(
        &self,
        actor: &Actor<'_>,
        id: &str,
        fence: i64,
        reason: &str,
    ) -> BridgeResult<Ticket> {
        transaction!(&self.store, |connection| {
            let now = self.scope(connection).await?;
            let workflow = self.configuration(connection, "workflow").await?;
            let ticket = self.ticket(connection, id, &workflow, now).await?;
            super::lease::check_held(&ticket, actor.id, Some(fence))?;
            self.clear_lease(
                connection,
                actor.id,
                now,
                &ticket,
                &super::lease::release_state(&workflow, &ticket),
                "ticket.released",
                serde_json::json!({"reason":reason}),
            )
            .await?;
            self.ticket(connection, id, &workflow, now).await
        })
    }
}

fn lease(ticket: Ticket, settings: &Settings, already_held: bool) -> BridgeResult<Lease> {
    let ttl = ticket.lease_ttl_ms.unwrap_or(settings.lease_ttl_ms);
    Ok(Lease {
        fence: ticket.fence,
        lease_id: ticket
            .lease_id
            .clone()
            .ok_or_else(|| fail("board_damaged", "held ticket has no lease ID"))?,
        lease_expires_at: ticket
            .lease_expires_at
            .ok_or_else(|| fail("board_damaged", "held ticket has no lease expiry"))?,
        heartbeat_every_ms: super::lease::beat_every(ttl),
        already_held,
        ticket,
    })
}

pub fn blocking<T>(
    runtime: &tokio::runtime::Handle,
    future: impl std::future::Future<Output = BridgeResult<T>>,
) -> BridgeResult<T> {
    match tokio::runtime::Handle::try_current() {
        Ok(handle) if handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread => {
            tokio::task::block_in_place(|| runtime.block_on(future))
        }
        Ok(_) => Err(fail(
            "board_runtime",
            "synchronous hosted board calls require a blocking worker or multi-thread runtime",
        )),
        Err(_) => runtime.block_on(future),
    }
}
