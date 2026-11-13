use super::*;
use crate::approval::{ApprovalStatus, KIND, Verdict, landing, text};

impl HostedBoard {
    pub async fn request_approval(
        &self,
        actor: &Actor<'_>,
        title: &str,
        what: &str,
        why: &str,
        undo: &str,
        labels: &[String],
    ) -> BridgeResult<ApprovalStatus> {
        let labels = serde_json::to_string(labels).expect("labels json");
        let title = text("title", title, 300, true)?;
        let what = text("what", what, 4000, true)?;
        let why = text("why", why, 4000, false)?;
        let undo = text("undo", undo, 4000, false)?;
        let description = format!("What: {what}\n\nWhy: {why}\n\nUndo: {undo}");
        transaction!(&self.store, |connection| {
            let now = self.scope(connection).await?;
            let workflow: Workflow = self.configuration(connection, "workflow").await?;
            let id = self.fresh_id(connection).await?;
            sqlx::query(
                "INSERT INTO board_tickets
                 (tenant_id,board_id,id,kind,title,description,state,priority,labels,created_by,created_at,updated_at)
                 VALUES ($1,$2,$3,'approval',$4,$5,$6,1,$9,$7,$8,$8)",
            ).bind(&self.tenant).bind(&self.project).bind(&id).bind(&title).bind(&description)
                .bind(&workflow.attention_state).bind(actor.id).bind(now).bind(&labels)
                .execute(&mut *connection).await?;
            self.event(
                connection,
                actor.id,
                now,
                "approval.requested",
                Some(&id),
                None,
                serde_json::json!({"title":title}),
            )
            .await?;
            self.approval_in(connection, &id, &workflow, now).await
        })
    }

    async fn approval_in(
        &self,
        connection: &mut AnyConnection,
        id: &str,
        workflow: &Workflow,
        now: i64,
    ) -> DbResult<ApprovalStatus> {
        let ticket = self.ticket(connection, id, workflow, now).await?;
        if ticket.kind != KIND {
            return Err(fail("not_an_approval", format!("{id} is not an approval")).into());
        }
        let decision=sqlx::query(
            "SELECT actor,at,data FROM board_events WHERE tenant_id=$1 AND board_id=$2 AND ticket=$3
             AND action='approval.decided' ORDER BY seq DESC LIMIT 1",
        ).bind(&self.tenant).bind(&self.project).bind(id).fetch_optional(&mut *connection).await?;
        let (status, decided_by, decided_at, note) = match decision {
            Some(row) => {
                let data: serde_json::Value = decode(&row.try_get::<String, _>("data")?)?;
                let verdict = data["verdict"]
                    .as_str()
                    .filter(|v| matches!(*v, "approved" | "denied"))
                    .ok_or_else(|| fail("board_damaged", "invalid approval verdict"))?;
                (
                    verdict.into(),
                    Some(row.try_get("actor")?),
                    Some(row.try_get("at")?),
                    data["note"]
                        .as_str()
                        .filter(|note| !note.is_empty())
                        .map(str::to_string),
                )
            }
            None => ("pending".into(), None, None, None),
        };
        Ok(ApprovalStatus {
            id: ticket.id,
            title: ticket.title,
            status,
            description: ticket.description,
            requested_by: ticket.created_by,
            requested_at: ticket.created_at,
            version: ticket.version,
            decided_by,
            decided_at,
            note,
            kind: crate::approval::label_value(&ticket.labels, "kind:"),
            target: crate::approval::label_value(&ticket.labels, "target:"),
        })
    }

    pub async fn edit_settings<F>(
        &self,
        actor: &Actor<'_>,
        action: &str,
        edit: &F,
    ) -> BridgeResult<Settings>
    where
        F: Fn(&mut Settings) -> BridgeResult<serde_json::Value>,
    {
        transaction!(&self.store, |connection| {
            let now = self.scope(connection).await?;
            let mut settings: Settings = self.configuration(connection, "settings").await?;
            let data = edit(&mut settings)?;
            self.save_configuration(connection, "settings", &settings)
                .await?;
            self.event(connection, actor.id, now, action, None, None, data)
                .await?;
            Ok(settings)
        })
    }

    pub async fn approval_status(&self, id: &str) -> BridgeResult<ApprovalStatus> {
        transaction!(&self.store, |connection| {
            let now = self.scope(connection).await?;
            let workflow = self.configuration(connection, "workflow").await?;
            self.approval_in(connection, id, &workflow, now).await
        })
    }

    pub async fn pending_approvals(&self) -> BridgeResult<Vec<ApprovalStatus>> {
        transaction!(&self.store, |connection| {
            let now = self.scope(connection).await?;
            let workflow: Workflow = self.configuration(connection, "workflow").await?;
            let ids:Vec<String>=sqlx::query_scalar(
                "SELECT id FROM board_tickets WHERE tenant_id=$1 AND board_id=$2 AND kind='approval'
                 AND state=$3 ORDER BY created_at,id COLLATE \"C\"",
            ).bind(&self.tenant).bind(&self.project).bind(&workflow.attention_state)
                .fetch_all(&mut *connection).await?;
            let mut approvals = Vec::with_capacity(ids.len());
            for id in ids {
                approvals.push(self.approval_in(connection, &id, &workflow, now).await?);
            }
            Ok(approvals)
        })
    }

    pub async fn decide_approval(
        &self,
        actor: &Actor<'_>,
        id: &str,
        version: i64,
        verdict: Verdict,
        note: &str,
    ) -> BridgeResult<ApprovalStatus> {
        if !actor.human {
            return Err(fail("humans_only", "only a person can approve or deny"));
        }
        let note = text("note", note, 2000, false)?;
        transaction!(&self.store, |connection| {
            let now = self.scope(connection).await?;
            let workflow: Workflow = self.configuration(connection, "workflow").await?;
            let ticket = self.ticket(connection, id, &workflow, now).await?;
            if ticket.kind != KIND {
                return Err(fail("not_an_approval", format!("{id} is not an approval")).into());
            }
            if ticket.state != workflow.attention_state {
                return Err(fail("already_decided", format!("{id} is already decided")).into());
            }
            if ticket.version != version {
                return Err(fail(
                    "approval_changed",
                    format!("{id} changed since you saw it; look again before deciding"),
                )
                .into());
            }
            let to = landing(&workflow, verdict)?;
            sqlx::query(
                "UPDATE board_tickets SET state=$4,closed_at=$5,version=version+1,updated_at=$5
                 WHERE tenant_id=$1 AND board_id=$2 AND id=$3",
            )
            .bind(&self.tenant)
            .bind(&self.project)
            .bind(id)
            .bind(&to)
            .bind(now)
            .execute(&mut *connection)
            .await?;
            if !note.is_empty() {
                self.insert_note(connection, actor.id, now, id, "decision", &note, None, None)
                    .await?;
            }
            self.event(
                connection,
                actor.id,
                now,
                "approval.decided",
                Some(id),
                None,
                serde_json::json!({"verdict":verdict,"note":note,"to":to}),
            )
            .await?;
            self.approval_in(connection, id, &workflow, now).await
        })
    }
}
