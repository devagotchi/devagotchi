use super::*;
use crate::board::{ChainStatus, Dep, DepKind, Filter, Patch, ProjectSettings};

impl HostedBoard {
    pub(crate) async fn welcome_if_new(
        &self,
        actor: &Actor<'_>,
        title: &str,
        steps: &str,
    ) -> BridgeResult<Option<String>> {
        transaction!(&self.store, |connection| {
            let now = self.scope(connection).await?;
            let count: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM board_tickets WHERE tenant_id=$1 AND board_id=$2",
            )
            .bind(&self.tenant)
            .bind(&self.project)
            .fetch_one(&mut *connection)
            .await?;
            if count > 0 {
                return Ok(None);
            }
            let workflow: Workflow = self.configuration(connection, "workflow").await?;
            let id = self.fresh_id(connection).await?;
            sqlx::query(
                "INSERT INTO board_tickets (tenant_id,board_id,id,kind,title,description,state,priority,labels,created_by,created_at,updated_at)
                 VALUES ($1,$2,$3,'task',$4,$5,$6,1,'[\"getting-started\"]',$7,$8,$8)",
            ).bind(&self.tenant).bind(&self.project).bind(&id).bind(title).bind(steps)
                .bind(&workflow.attention_state).bind(actor.id).bind(now).execute(&mut *connection).await?;
            self.event(
                connection,
                actor.id,
                now,
                "board.welcomed",
                Some(&id),
                None,
                serde_json::json!({"title":title}),
            )
            .await?;
            Ok(Some(id))
        })
    }

    pub(crate) async fn claim_next(
        &self,
        actor: &Actor<'_>,
        filter: &crate::board::NextFilter,
    ) -> BridgeResult<Option<Lease>> {
        transaction!(&self.store, |connection| {
            let now = self.scope(connection).await?;
            let workflow: Workflow = self.configuration(connection, "workflow").await?;
            let settings: Settings = self.configuration(connection, "settings").await?;
            let limit = actor.wip_limit.unwrap_or(settings.wip_limit_default);
            if limit > 0 {
                let held:i64=sqlx::query_scalar(
                    "SELECT count(*) FROM board_tickets WHERE tenant_id=$1 AND board_id=$2 AND holder=$3 AND lease_expires_at>$4",
                ).bind(&self.tenant).bind(&self.project).bind(actor.id).bind(now).fetch_one(&mut *connection).await?;
                if held >= limit {
                    return Err(fail(
                        "wip_limit",
                        format!("you already hold {held} ticket(s); the WIP limit is {limit}"),
                    )
                    .into());
                }
            }
            let list = Filter {
                label: filter.label.clone(),
                parent: filter.parent.clone(),
                limit: Some(25),
                ..Default::default()
            };
            let candidates = self
                .list_in(connection, &workflow, now, &list, Some(actor.id))
                .await?;
            for mut ticket in candidates {
                if !filter.states.is_empty() && !filter.states.contains(&ticket.state) {
                    continue;
                }
                if ticket.holder.is_some() {
                    self.expire(connection, actor.id, now, &ticket, &workflow, &settings)
                        .await?;
                    ticket = self.ticket(connection, &ticket.id, &workflow, now).await?;
                }
                let Some(lane) = workflow.lane_by_queue(&ticket.state) else {
                    continue;
                };
                if !filter.states.is_empty() && !filter.states.contains(&ticket.state) {
                    continue;
                }
                if lane.distinct_from_previous && ticket.last_worker.as_deref() == Some(actor.id) {
                    continue;
                }
                return Ok(Some(
                    self.claim_in(
                        connection,
                        actor,
                        &ticket.id,
                        filter.ttl_ms,
                        now,
                        &workflow,
                        &settings,
                    )
                    .await?,
                ));
            }
            Ok(None)
        })
    }

    pub(crate) async fn settings(&self) -> BridgeResult<Settings> {
        transaction!(&self.store, |connection| {
            self.scope(connection).await?;
            self.configuration(connection, "settings").await
        })
    }

    pub(crate) async fn workflow(&self) -> BridgeResult<Workflow> {
        transaction!(&self.store, |connection| {
            self.scope(connection).await?;
            self.configuration(connection, "workflow").await
        })
    }

    pub(crate) async fn put_settings(
        &self,
        actor: &Actor<'_>,
        settings: &Settings,
    ) -> BridgeResult<()> {
        if settings.lease_min_ttl_ms <= 0
            || settings.lease_max_ttl_ms < settings.lease_min_ttl_ms
            || settings.lease_ttl_ms < settings.lease_min_ttl_ms
            || settings.lease_ttl_ms > settings.lease_max_ttl_ms
        {
            return Err(invalid(
                "lease ttl bounds must satisfy min <= default <= max, min > 0",
            ));
        }
        transaction!(&self.store, |connection| {
            let now = self.scope(connection).await?;
            self.save_configuration(connection, "settings", settings)
                .await?;
            self.event(
                connection,
                actor.id,
                now,
                "admin.settings.put",
                None,
                None,
                serde_json::json!({}),
            )
            .await?;
            Ok(())
        })
    }

    pub(super) async fn save_configuration<T: serde::Serialize>(
        &self,
        connection: &mut AnyConnection,
        key: &str,
        value: &T,
    ) -> DbResult<()> {
        let value = serde_json::to_string(value).map_err(|error| invalid(error.to_string()))?;
        sqlx::query(
            "UPDATE board_settings SET value=$4 WHERE tenant_id=$1 AND board_id=$2 AND key=$3",
        )
        .bind(&self.tenant)
        .bind(&self.project)
        .bind(key)
        .bind(value)
        .execute(&mut *connection)
        .await?;
        Ok(())
    }

    pub(crate) async fn set_project(
        &self,
        actor: &Actor<'_>,
        patch: &ProjectSettings,
    ) -> BridgeResult<Settings> {
        let repo = patch
            .repo
            .as_deref()
            .map(crate::board::check_repo)
            .transpose()?;
        let environments = patch
            .environments
            .as_deref()
            .map(crate::board::check_environments)
            .transpose()?;
        transaction!(&self.store, |connection| {
            let now = self.scope(connection).await?;
            let mut settings: Settings = self.configuration(connection, "settings").await?;
            let mut changed = serde_json::Map::new();
            if let Some(repo) = &repo {
                settings.repo = Some(repo.clone());
                changed.insert("repo".into(), serde_json::json!(repo));
            }
            if let Some(environments) = &environments {
                settings.environments = environments.clone();
                changed.insert("environments".into(), serde_json::json!(environments));
            }
            if !changed.is_empty() {
                self.save_configuration(connection, "settings", &settings)
                    .await?;
                self.event(
                    connection,
                    actor.id,
                    now,
                    "board.settings_changed",
                    None,
                    None,
                    changed.into(),
                )
                .await?;
            }
            Ok(settings)
        })
    }

    pub(crate) async fn put_workflow(
        &self,
        actor: &Actor<'_>,
        workflow: &Workflow,
    ) -> BridgeResult<()> {
        workflow.validate().map_err(invalid)?;
        transaction!(&self.store, |connection| {
            let now = self.scope(connection).await?;
            let states: Vec<String> = sqlx::query_scalar(
                "SELECT DISTINCT state FROM board_tickets WHERE tenant_id=$1 AND board_id=$2",
            )
            .bind(&self.tenant)
            .bind(&self.project)
            .fetch_all(&mut *connection)
            .await?;
            if let Some(unknown) = states.iter().find(|state| workflow.state(state).is_none()) {
                return Err(invalid(format!(
                    "tickets are in state {unknown:?}, which the new workflow lacks"
                ))
                .into());
            }
            self.save_configuration(connection, "workflow", workflow)
                .await?;
            self.event(
                connection,
                actor.id,
                now,
                "admin.workflow.put",
                None,
                None,
                serde_json::json!({"id":workflow.id}),
            )
            .await?;
            Ok(())
        })
    }

    async fn list_in(
        &self,
        connection: &mut AnyConnection,
        workflow: &Workflow,
        now: i64,
        filter: &Filter,
        ready_for: Option<&str>,
    ) -> DbResult<Vec<Ticket>> {
        let pattern = filter.text.as_ref().map(|text| {
            format!(
                "%{}%",
                text.replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_")
            )
        });
        let ids: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM board_tickets
             WHERE tenant_id=$1 AND board_id=$2
               AND ($3::text IS NULL OR state=$3)
               AND ($4::text IS NULL OR holder=$4)
               AND ($5::text IS NULL OR assignee=$5)
               AND ($6::text IS NULL OR parent=$6)
               AND ($7::text IS NULL OR kind=$7)
               AND ($8::text IS NULL OR jsonb_exists(labels::jsonb,$8))
               AND ($9::text IS NULL OR title ILIKE $9 ESCAPE '\\' OR description ILIKE $9 ESCAPE '\\')
               AND ($10::text IS NULL OR machine=$10)
               AND ($11::text IS NULL OR repo=$11)
               AND ($12::text IS NULL OR environment=$12)
               AND ($13::text IS NULL OR agent=$13)
               AND ($14::text IS NULL OR session=$14)
               AND ($15::text IS NULL OR fingerprint=$15)
             ORDER BY (assignee IS NOT NULL) DESC,priority,created_at,id COLLATE \"C\"",
        ).bind(&self.tenant).bind(&self.project).bind(&filter.state).bind(&filter.holder)
            .bind(&filter.assignee).bind(&filter.parent).bind(&filter.kind).bind(&filter.label).bind(pattern)
            .bind(clean_anchor(&filter.anchors.machine)).bind(clean_anchor(&filter.anchors.repo))
            .bind(clean_anchor(&filter.anchors.environment)).bind(clean_anchor(&filter.anchors.agent))
            .bind(clean_anchor(&filter.anchors.session)).bind(clean_anchor(&filter.anchors.fingerprint))
            .fetch_all(&mut *connection).await?;
        let mut tickets = Vec::new();
        let limit = filter.limit.unwrap_or(200).max(1);
        for id in ids {
            let ticket = self.ticket(connection, &id, workflow, now).await?;
            if filter.open_only && workflow.is_done(&ticket.state) {
                continue;
            }
            if filter.ready || ready_for.is_some() {
                if ticket.holder.is_some() && !ticket.lease_expired {
                    continue;
                }
                if workflow.lane_by_queue(&ticket.state).is_none() && !ticket.lease_expired {
                    continue;
                }
                if !ticket.blocked_by.is_empty() {
                    continue;
                }
                if let Some(actor) = ready_for {
                    if ticket
                        .assignee
                        .as_deref()
                        .is_some_and(|assigned| assigned != actor)
                    {
                        continue;
                    }
                    let lane = workflow
                        .lane_by_queue(&ticket.state)
                        .or_else(|| workflow.lane_by_active(&ticket.state));
                    if lane.is_some_and(|lane| lane.distinct_from_previous)
                        && ticket.last_worker.as_deref() == Some(actor)
                    {
                        continue;
                    }
                }
            }
            tickets.push(ticket);
            if tickets.len() >= limit {
                break;
            }
        }
        Ok(tickets)
    }

    pub(crate) async fn list(&self, filter: &Filter) -> BridgeResult<Vec<Ticket>> {
        transaction!(&self.store, |connection| {
            let now = self.scope(connection).await?;
            let workflow = self.configuration(connection, "workflow").await?;
            self.list_in(connection, &workflow, now, filter, None).await
        })
    }

    pub(crate) async fn ready(&self, actor: &Actor<'_>, limit: usize) -> BridgeResult<Vec<Ticket>> {
        transaction!(&self.store, |connection| {
            let now = self.scope(connection).await?;
            let workflow = self.configuration(connection, "workflow").await?;
            self.list_in(
                connection,
                &workflow,
                now,
                &Filter {
                    ready: true,
                    limit: Some(limit),
                    ..Default::default()
                },
                Some(actor.id),
            )
            .await
        })
    }

    async fn deps(
        &self,
        connection: &mut AnyConnection,
        id: &str,
        reverse: bool,
    ) -> DbResult<Vec<Dep>> {
        let rows = sqlx::query(
            "SELECT ticket,depends_on,kind FROM board_deps WHERE tenant_id=$1 AND board_id=$2
             AND (($4=0 AND ticket=$3) OR ($4=1 AND depends_on=$3))
             ORDER BY kind COLLATE \"C\",ticket COLLATE \"C\",depends_on COLLATE \"C\"",
        )
        .bind(&self.tenant)
        .bind(&self.project)
        .bind(id)
        .bind(i64::from(reverse))
        .fetch_all(&mut *connection)
        .await?;
        let mut deps = Vec::with_capacity(rows.len());
        for row in rows {
            let kind: String = row.try_get("kind")?;
            deps.push(Dep {
                ticket: row.try_get("ticket")?,
                depends_on: row.try_get("depends_on")?,
                kind: DepKind::parse(&kind)
                    .ok_or_else(|| fail("board_damaged", "invalid dependency kind"))?,
            });
        }
        Ok(deps)
    }

    pub(crate) async fn view(&self, id: &str) -> BridgeResult<crate::board::TicketView> {
        transaction!(&self.store, |connection| {
            let now = self.scope(connection).await?;
            let workflow = self.configuration(connection, "workflow").await?;
            let ticket = self.ticket(connection, id, &workflow, now).await?;
            let deps = self.deps(connection, id, false).await?;
            let dependents = self.deps(connection, id, true).await?;
            let children=sqlx::query_scalar(
                "SELECT id FROM board_tickets WHERE tenant_id=$1 AND board_id=$2 AND parent=$3 ORDER BY created_at,id COLLATE \"C\"",
            ).bind(&self.tenant).bind(&self.project).bind(id).fetch_all(&mut *connection).await?;
            let texts:Vec<String>=sqlx::query_scalar(
                "SELECT row_to_json(n)::text FROM board_notes n WHERE tenant_id=$1 AND board_id=$2 AND ticket=$3 ORDER BY seq",
            ).bind(&self.tenant).bind(&self.project).bind(id).fetch_all(&mut *connection).await?;
            let notes = texts
                .iter()
                .map(|text| decode(text))
                .collect::<BridgeResult<Vec<Note>>>()?;
            Ok(crate::board::TicketView {
                ticket,
                deps,
                dependents,
                children,
                notes,
            })
        })
    }

    pub(crate) async fn add_dep(
        &self,
        actor: &Actor<'_>,
        id: &str,
        depends_on: &str,
        kind: DepKind,
    ) -> BridgeResult<Vec<Dep>> {
        if id == depends_on {
            return Err(invalid("a ticket cannot depend on itself"));
        }
        transaction!(&self.store, |connection| {
            let now = self.scope(connection).await?;
            let workflow = self.configuration(connection, "workflow").await?;
            self.ticket(connection, id, &workflow, now).await?;
            self.ticket(connection, depends_on, &workflow, now).await?;
            if kind == DepKind::Blocks {
                let count:i64=sqlx::query_scalar(
                    "WITH RECURSIVE walk(id) AS (
                       SELECT $3::text UNION SELECT d.depends_on FROM board_deps d JOIN walk ON d.ticket=walk.id
                       WHERE d.tenant_id=$1 AND d.board_id=$2 AND d.kind='blocks'
                     ) SELECT count(*) FROM walk WHERE id=$4",
                ).bind(&self.tenant).bind(&self.project).bind(depends_on).bind(id)
                    .fetch_one(&mut *connection).await?;
                if count > 0 {
                    return Err(fail(
                        "dependency_cycle",
                        format!("{depends_on} already depends on {id}"),
                    )
                    .into());
                }
            }
            sqlx::query(
                "INSERT INTO board_deps (tenant_id,board_id,ticket,depends_on,kind,created_by,created_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING",
            ).bind(&self.tenant).bind(&self.project).bind(id).bind(depends_on).bind(kind.as_str())
                .bind(actor.id).bind(now).execute(&mut *connection).await?;
            self.event(
                connection,
                actor.id,
                now,
                "dep.added",
                Some(id),
                None,
                serde_json::json!({"depends_on":depends_on,"kind":kind.as_str()}),
            )
            .await?;
            self.deps(connection, id, false).await
        })
    }

    pub(crate) async fn remove_dep(
        &self,
        actor: &Actor<'_>,
        id: &str,
        depends_on: &str,
        kind: DepKind,
    ) -> BridgeResult<Vec<Dep>> {
        transaction!(&self.store, |connection| {
            let now = self.scope(connection).await?;
            let removed=sqlx::query(
                "DELETE FROM board_deps WHERE tenant_id=$1 AND board_id=$2 AND ticket=$3 AND depends_on=$4 AND kind=$5",
            ).bind(&self.tenant).bind(&self.project).bind(id).bind(depends_on).bind(kind.as_str())
                .execute(&mut *connection).await?.rows_affected();
            if removed == 0 {
                return Err(fail("not_found", "no such dependency").into());
            }
            self.event(
                connection,
                actor.id,
                now,
                "dep.removed",
                Some(id),
                None,
                serde_json::json!({"depends_on":depends_on,"kind":kind.as_str()}),
            )
            .await?;
            self.deps(connection, id, false).await
        })
    }

    pub(crate) async fn update(
        &self,
        actor: &Actor<'_>,
        id: &str,
        version: i64,
        patch: &Patch,
    ) -> BridgeResult<Ticket> {
        transaction!(&self.store, |connection| {
            let now = self.scope(connection).await?;
            let workflow: Workflow = self.configuration(connection, "workflow").await?;
            let current = self.ticket(connection, id, &workflow, now).await?;
            if current.kind == crate::board::approval::KIND {
                return Err(fail(
                    "approval_decide_only",
                    "an approval changes only through a person's decision",
                )
                .into());
            }
            if current.version != version {
                return Err(fail(
                    "version_conflict",
                    format!(
                        "version {version} is not current ({}); re-read and retry",
                        current.version
                    ),
                )
                .into());
            }
            let mut changed = serde_json::Map::new();
            let mut ticket = current.clone();
            if let Some(title) = &patch.title {
                let title = title.trim();
                if title.is_empty() || title.chars().count() > 300 {
                    return Err(invalid("title must be 1 to 300 characters").into());
                }
                ticket.title = title.into();
                changed.insert("title".into(), serde_json::json!(title));
            }
            if let Some(description) = &patch.description {
                ticket.description = description.clone();
                changed.insert("description".into(), true.into());
            }
            if let Some(priority) = patch.priority {
                ticket.priority = priority;
                changed.insert("priority".into(), priority.into());
            }
            if let Some(labels) = &patch.labels {
                ticket.labels = clean_labels(labels);
                changed.insert("labels".into(), serde_json::json!(ticket.labels));
            }
            if let Some(assignee) = &patch.assignee {
                ticket.assignee = assignee.clone();
                changed.insert("assignee".into(), serde_json::json!(assignee));
            }
            for (column, value) in patch.anchors.columns() {
                if let Some(value) = value {
                    let value = clean_anchor(value);
                    if column == "environment" {
                        let settings: Settings = self.configuration(connection, "settings").await?;
                        check_environment(&settings, value.as_deref())?;
                    }
                    match column {
                        "machine" => ticket.machine = value.clone(),
                        "repo" => ticket.repo = value.clone(),
                        "environment" => ticket.environment = value.clone(),
                        "agent" => ticket.agent = value.clone(),
                        "session" => ticket.session = value.clone(),
                        "fingerprint" => ticket.fingerprint = value.clone(),
                        _ => unreachable!("fixed anchor columns"),
                    }
                    changed.insert(column.into(), serde_json::json!(value));
                }
            }
            if let Some(occurrences) = patch.occurrences {
                if occurrences < 1 {
                    return Err(invalid("occurrences must be at least 1").into());
                }
                ticket.occurrences = occurrences;
                changed.insert("occurrences".into(), occurrences.into());
            }
            if let Some(last_seen_at) = patch.last_seen_at {
                ticket.last_seen_at = last_seen_at;
                changed.insert("last_seen_at".into(), serde_json::json!(last_seen_at));
            }
            if let Some(to) = patch.state.as_ref().filter(|to| *to != &current.state) {
                crate::board::check_transition(&workflow, &current, to, actor.id)?;
                ticket.state = to.clone();
                ticket.closed_at = workflow.is_done(to).then_some(now);
                changed.insert("from".into(), serde_json::json!(current.state));
                changed.insert("to".into(), serde_json::json!(to));
            }
            if changed.is_empty() {
                return Ok(current);
            }
            sqlx::query(
                "UPDATE board_tickets SET title=$4,description=$5,priority=$6,labels=$7,assignee=$8,
                 machine=$9,repo=$10,environment=$11,agent=$12,session=$13,fingerprint=$14,occurrences=$15,
                 last_seen_at=$16,state=$17,closed_at=$18,version=version+1,updated_at=$19
                 WHERE tenant_id=$1 AND board_id=$2 AND id=$3",
            ).bind(&self.tenant).bind(&self.project).bind(id).bind(&ticket.title).bind(&ticket.description)
                .bind(ticket.priority).bind(serde_json::to_string(&ticket.labels).expect("labels json"))
                .bind(&ticket.assignee).bind(&ticket.machine).bind(&ticket.repo).bind(&ticket.environment)
                .bind(&ticket.agent).bind(&ticket.session).bind(&ticket.fingerprint).bind(ticket.occurrences)
                .bind(ticket.last_seen_at).bind(&ticket.state).bind(ticket.closed_at).bind(now)
                .execute(&mut *connection).await?;
            self.event(
                connection,
                actor.id,
                now,
                "ticket.updated",
                Some(id),
                None,
                changed.into(),
            )
            .await?;
            self.ticket(connection, id, &workflow, now).await
        })
    }

    pub(crate) async fn handoff(
        &self,
        actor: &Actor<'_>,
        id: &str,
        fence: i64,
        note: &str,
        to: Option<&str>,
    ) -> BridgeResult<Ticket> {
        let note = note.trim();
        if note.is_empty() {
            return Err(fail(
                "invalid",
                "a handoff needs a note for whoever picks it up",
            ));
        }
        transaction!(&self.store, |connection| {
            let now = self.scope(connection).await?;
            let workflow = self.configuration(connection, "workflow").await?;
            let ticket = self.ticket(connection, id, &workflow, now).await?;
            crate::board::lease::check_held(&ticket, actor.id, Some(fence))?;
            self.insert_note(
                connection,
                actor.id,
                now,
                id,
                "handoff",
                note,
                to,
                Some(fence),
            )
            .await?;
            sqlx::query(
                "UPDATE board_tickets SET assignee=$4 WHERE tenant_id=$1 AND board_id=$2 AND id=$3",
            )
            .bind(&self.tenant)
            .bind(&self.project)
            .bind(id)
            .bind(to)
            .execute(&mut *connection)
            .await?;
            self.clear_lease(
                connection,
                actor.id,
                now,
                &ticket,
                &crate::board::lease::release_state(&workflow, &ticket),
                "ticket.handoff",
                serde_json::json!({"to":to}),
            )
            .await?;
            self.ticket(connection, id, &workflow, now).await
        })
    }

    pub(crate) async fn force_release(
        &self,
        actor: &Actor<'_>,
        id: &str,
        reason: &str,
    ) -> BridgeResult<Ticket> {
        if !actor.human {
            return Err(fail("humans_only", "force-release needs a human principal"));
        }
        if reason.trim().chars().count() < 3 {
            return Err(invalid("a reason of at least 3 characters is required"));
        }
        transaction!(&self.store, |connection| {
            let now = self.scope(connection).await?;
            let workflow = self.configuration(connection, "workflow").await?;
            let ticket = self.ticket(connection, id, &workflow, now).await?;
            if ticket.holder.is_none() {
                return Err(fail("not_held", format!("{id} is not held")).into());
            }
            self.clear_lease(connection,actor.id,now,&ticket,&crate::board::lease::release_state(&workflow,&ticket),
                "ticket.force_released",serde_json::json!({"previous_holder":ticket.holder,"previous_fence":ticket.fence,"reason":reason.trim()})).await?;
            self.ticket(connection, id, &workflow, now).await
        })
    }

    pub(crate) async fn reap(&self) -> BridgeResult<usize> {
        transaction!(&self.store, |connection| {
            let now = self.scope(connection).await?;
            let workflow = self.configuration(connection, "workflow").await?;
            let settings = self.configuration(connection, "settings").await?;
            let ids:Vec<String>=sqlx::query_scalar(
                "SELECT id FROM board_tickets WHERE tenant_id=$1 AND board_id=$2 AND holder IS NOT NULL AND lease_expires_at<=$3 ORDER BY id COLLATE \"C\"",
            ).bind(&self.tenant).bind(&self.project).bind(now).fetch_all(&mut *connection).await?;
            for id in &ids {
                let ticket = self.ticket(connection, id, &workflow, now).await?;
                self.expire(connection, "system", now, &ticket, &workflow, &settings)
                    .await?;
            }
            Ok(ids.len())
        })
    }

    pub(crate) async fn next_lease_expiry(&self) -> BridgeResult<Option<i64>> {
        transaction!(&self.store, |connection| {
            self.scope(connection).await?;
            Ok(sqlx::query_scalar(
                "SELECT min(lease_expires_at) FROM board_tickets WHERE tenant_id=$1 AND board_id=$2 AND holder IS NOT NULL",
            ).bind(&self.tenant).bind(&self.project).fetch_one(&mut *connection).await?)
        })
    }

    pub(crate) async fn verify_chain(&self) -> BridgeResult<ChainStatus> {
        let events = self.events().await?;
        let mut previous = GENESIS.to_string();
        let mut broken = None;
        for (index, event) in events.iter().enumerate() {
            if broken.is_none()
                && (event.seq != index as i64 + 1
                    || event.prev_hash != previous
                    || event.hash != event_hash(&previous, event))
            {
                broken = Some(event.seq);
            }
            previous = event.hash.clone();
        }
        Ok(ChainStatus {
            entries: events.len() as u64,
            ok: broken.is_none(),
            head: previous,
            first_broken: broken,
        })
    }
}
