use super::*;
use crate::agenthub::issue_lifecycle::{
    ContextReceipt, ContextWrite, DeliveryReceipt, IssueContext, IssueWebhookEvent, SourceIssue,
    hash,
};

impl HostedBoard {
    pub(crate) async fn issue_source(&self, ticket: &str) -> BridgeResult<Option<SourceIssue>> {
        transaction!(&self.store, |connection| {
            self.scope(connection).await?;
            let text: Option<String> = sqlx::query_scalar(
                "SELECT row_to_json(i)::text FROM board_issues i
                 WHERE tenant_id=$1 AND board_id=$2 AND ticket=$3",
            )
            .bind(&self.tenant)
            .bind(&self.project)
            .bind(ticket)
            .fetch_optional(&mut *connection)
            .await?;
            Ok(text.map(|text| decode(&text)).transpose()?)
        })
    }

    async fn source_issue(
        &self,
        connection: &mut AnyConnection,
        repository: i64,
        number: i64,
    ) -> DbResult<Option<SourceIssue>> {
        let text: Option<String> = sqlx::query_scalar(
            "SELECT row_to_json(i)::text FROM board_issues i
             WHERE tenant_id=$1 AND board_id=$2 AND repository_id=$3 AND issue_number=$4",
        )
        .bind(&self.tenant)
        .bind(&self.project)
        .bind(repository)
        .bind(number)
        .fetch_optional(&mut *connection)
        .await?;
        text.map(|text| decode(&text).map_err(Failed::from))
            .transpose()
    }

    pub(crate) async fn ingest_issue(
        &self,
        event: &IssueWebhookEvent,
        delivery: &str,
        payload_hash: &str,
    ) -> BridgeResult<DeliveryReceipt> {
        event.validate()?;
        let repository = event.repository.id as i64;
        let number = event.issue.number as i64;
        let source_hash = hash(
            serde_json::to_string(&event.issue)
                .expect("issue json")
                .as_bytes(),
        );
        transaction!(&self.store, |connection| {
            let now = self.scope(connection).await?;
            let previous = sqlx::query(
                "SELECT payload_hash,outcome,issue_number FROM board_issue_deliveries
                 WHERE tenant_id=$1 AND board_id=$2 AND repository_id=$3 AND delivery_id=$4",
            )
            .bind(&self.tenant)
            .bind(&self.project)
            .bind(repository)
            .bind(delivery)
            .fetch_optional(&mut *connection)
            .await?;
            if let Some(previous) = previous {
                if previous.try_get::<String, _>("payload_hash")? != payload_hash {
                    return Err(fail(
                        "delivery_conflict",
                        "delivery ID was already used with different bytes",
                    )
                    .into());
                }
                let issue = self
                    .source_issue(connection, repository, previous.try_get("issue_number")?)
                    .await?
                    .ok_or_else(|| fail("board_damaged", "delivery has no issue mapping"))?;
                return Ok(DeliveryReceipt {
                    status: previous.try_get("outcome")?,
                    duplicate: true,
                    issue,
                });
            }
            let valid: bool =
                sqlx::query_scalar("SELECT pg_input_is_valid($1,'timestamp with time zone')")
                    .bind(&event.issue.updated_at)
                    .fetch_one(&mut *connection)
                    .await?;
            if !valid {
                return Err(invalid("invalid issue.updated_at calendar value").into());
            }
            let source_time: i64 =
                sqlx::query_scalar("SELECT (extract(epoch FROM $1::timestamptz)*1000)::bigint")
                    .bind(&event.issue.updated_at)
                    .fetch_one(&mut *connection)
                    .await?;
            let existing = self.source_issue(connection, repository, number).await?;
            let workflow: Workflow = self.configuration(connection, "workflow").await?;
            let (ticket, outcome, revision, epoch) = if let Some(existing) = &existing {
                let ticket = self
                    .ticket(connection, &existing.ticket, &workflow, now)
                    .await?;
                let outcome = if source_time < existing.source_time {
                    "stale"
                } else if source_time == existing.source_time {
                    if source_hash == existing.source_hash {
                        "unchanged"
                    } else {
                        "needs_reconciliation"
                    }
                } else {
                    "accepted"
                };
                let epoch = existing.epoch
                    + i64::from(
                        existing.source_state == "closed"
                            && event.issue.state == "open"
                            && outcome == "accepted",
                    );
                (ticket, outcome, existing.source_revision + 1, epoch)
            } else {
                let new = NewTicket {
                    title: event.issue.title.clone(),
                    description: format!(
                        "Source: {}\n\n{}",
                        event.issue.html_url,
                        event.issue.body.as_deref().unwrap_or("")
                    ),
                    labels: vec!["github-issue".into()],
                    anchors: super::super::Anchors {
                        repo: Some(format!("https://github.com/{}", event.repository.full_name)),
                        ..Default::default()
                    },
                    ..Default::default()
                };
                let ticket = self
                    .create_in(connection, &Actor::agent("github"), &new, now)
                    .await?;
                (ticket, "accepted", 1, 1)
            };
            if outcome == "accepted" {
                sqlx::query(
                    "INSERT INTO board_issues
                     (tenant_id,board_id,repository_id,issue_number,ticket,repository,issue_title,issue_url,
                      source_state,source_action,source_actor,source_updated_at,source_time,source_hash,source_revision,epoch)
                     VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)
                     ON CONFLICT (tenant_id,board_id,repository_id,issue_number)
                     DO UPDATE SET repository=excluded.repository,issue_title=excluded.issue_title,issue_url=excluded.issue_url,
                       source_state=excluded.source_state,source_action=excluded.source_action,source_actor=excluded.source_actor,
                       source_updated_at=excluded.source_updated_at,source_time=excluded.source_time,source_hash=excluded.source_hash,
                       source_revision=excluded.source_revision,epoch=excluded.epoch,needs_reconciliation=0",
                ).bind(&self.tenant).bind(&self.project).bind(repository).bind(number).bind(&ticket.id)
                    .bind(&event.repository.full_name).bind(&event.issue.title).bind(&event.issue.html_url)
                    .bind(&event.issue.state).bind(&event.action).bind(&event.sender.login).bind(&event.issue.updated_at)
                    .bind(source_time).bind(&source_hash).bind(revision).bind(epoch).execute(&mut *connection).await?;
            } else if outcome == "needs_reconciliation" {
                sqlx::query(
                    "UPDATE board_issues SET needs_reconciliation=1
                     WHERE tenant_id=$1 AND board_id=$2 AND repository_id=$3 AND issue_number=$4",
                )
                .bind(&self.tenant)
                .bind(&self.project)
                .bind(repository)
                .bind(number)
                .execute(&mut *connection)
                .await?;
            }
            if outcome == "accepted" || outcome == "needs_reconciliation" {
                let snapshot = serde_json::json!({
                    "repository_id":repository,"issue_number":number,"url":event.issue.html_url,
                    "action":event.action,"source_state":event.issue.state,"source_updated_at":event.issue.updated_at,
                    "delivery_id":delivery,"outcome":outcome,"epoch":epoch,"sender":event.sender.login,
                    "board_state":ticket.state,"board_version":ticket.version,
                    "holder":ticket.holder,"fence":ticket.fence,
                    "finalization":if event.issue.state=="closed" {"pending"} else {"not_requested"}
                });
                self.insert_note(
                    connection,
                    "github",
                    now,
                    &ticket.id,
                    "issue.source",
                    &snapshot.to_string(),
                    None,
                    None,
                )
                .await?;
                sqlx::query("UPDATE board_tickets SET version=version+1,updated_at=$4 WHERE tenant_id=$1 AND board_id=$2 AND id=$3")
                    .bind(&self.tenant).bind(&self.project).bind(&ticket.id).bind(now).execute(&mut *connection).await?;
                self.event(
                    connection,
                    "github",
                    now,
                    "issue.projected",
                    Some(&ticket.id),
                    None,
                    snapshot,
                )
                .await?;
            }
            sqlx::query(
                "INSERT INTO board_issue_deliveries
                 (tenant_id,board_id,repository_id,delivery_id,payload_hash,issue_number,outcome,received_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
            ).bind(&self.tenant).bind(&self.project).bind(repository).bind(delivery).bind(payload_hash)
                .bind(number).bind(outcome).bind(now).execute(&mut *connection).await?;
            let issue = self
                .source_issue(connection, repository, number)
                .await?
                .ok_or_else(|| fail("board_damaged", "issue mapping was not saved"))?;
            Ok(DeliveryReceipt {
                status: outcome.into(),
                duplicate: false,
                issue,
            })
        })
    }

    pub(crate) async fn issue_context(
        &self,
        repository: u64,
        number: u64,
        after: i64,
        limit: usize,
    ) -> BridgeResult<IssueContext> {
        let repository = i64::try_from(repository).map_err(|_| invalid("invalid repository ID"))?;
        let number = i64::try_from(number).map_err(|_| invalid("invalid issue number"))?;
        if number <= 0 || after < 0 || !(1..=100).contains(&limit) {
            return Err(invalid("invalid context page"));
        }
        transaction!(&self.store, |connection| {
            let now = self.scope(connection).await?;
            let issue = self
                .source_issue(connection, repository, number)
                .await?
                .ok_or_else(|| fail("not_found", "issue has not been projected"))?;
            let workflow: Workflow = self.configuration(connection, "workflow").await?;
            let ticket = self
                .ticket(connection, &issue.ticket, &workflow, now)
                .await?;
            let rows: Vec<String> = sqlx::query_scalar(
                "SELECT row_to_json(n)::text FROM board_notes n
                 WHERE tenant_id=$1 AND board_id=$2 AND ticket=$3 AND seq>$4 ORDER BY seq LIMIT $5",
            )
            .bind(&self.tenant)
            .bind(&self.project)
            .bind(&ticket.id)
            .bind(after)
            .bind(limit as i64 + 1)
            .fetch_all(&mut *connection)
            .await?;
            let mut notes: Vec<Note> = rows
                .into_iter()
                .map(|text| decode(&text))
                .collect::<BridgeResult<_>>()?;
            let more = notes.len() > limit;
            notes.truncate(limit);
            let next_after = if more {
                notes.last().map(|note| note.seq)
            } else {
                None
            };
            let finalization = issue.finalization();
            Ok(IssueContext {
                project: self.name.clone(),
                issue,
                ticket,
                notes,
                next_after,
                finalization: finalization.into(),
            })
        })
    }

    pub(crate) async fn write_issue_context(
        &self,
        actor: &str,
        repository: u64,
        payload: &ContextWrite,
    ) -> BridgeResult<ContextReceipt> {
        payload.validate()?;
        let repository = i64::try_from(repository).map_err(|_| invalid("invalid repository ID"))?;
        let number =
            i64::try_from(payload.issue_number).map_err(|_| invalid("invalid issue number"))?;
        let body = serde_json::to_string(payload).expect("context json");
        let payload_hash = hash(body.as_bytes());
        transaction!(&self.store, |connection| {
            let now = self.scope(connection).await?;
            let existing = sqlx::query(
                "SELECT payload_hash,ticket,note_seq,revision FROM board_context_receipts
                 WHERE tenant_id=$1 AND board_id=$2 AND actor=$3 AND request_id=$4",
            )
            .bind(&self.tenant)
            .bind(&self.project)
            .bind(actor)
            .bind(&payload.request_id)
            .fetch_optional(&mut *connection)
            .await?;
            if let Some(existing) = existing {
                if existing.try_get::<String, _>("payload_hash")? != payload_hash {
                    return Err(fail(
                        "request_conflict",
                        "request ID was already used with different content",
                    )
                    .into());
                }
                return Ok(ContextReceipt {
                    request_id: payload.request_id.clone(),
                    ticket: existing.try_get("ticket")?,
                    note_seq: existing.try_get("note_seq")?,
                    revision: existing.try_get("revision")?,
                });
            }
            let issue = self
                .source_issue(connection, repository, number)
                .await?
                .ok_or_else(|| fail("not_found", "issue has not been projected"))?;
            let workflow = self.configuration(connection, "workflow").await?;
            let ticket = self
                .ticket(connection, &issue.ticket, &workflow, now)
                .await?;
            if ticket.version != payload.expected_version {
                return Err(fail(
                    "version_conflict",
                    format!(
                        "current ticket version is {}; reread context before retrying",
                        ticket.version
                    ),
                )
                .into());
            }
            if ticket.holder.is_some() {
                crate::agenthub::lease::check_held(&ticket, actor, payload.fence)?;
            }
            let note = self
                .insert_note(
                    connection,
                    actor,
                    now,
                    &ticket.id,
                    "issue.context",
                    &body,
                    None,
                    payload.fence,
                )
                .await?;
            let revision: i64 = sqlx::query_scalar(
                "UPDATE board_tickets SET version=version+1,updated_at=$4
                 WHERE tenant_id=$1 AND board_id=$2 AND id=$3 RETURNING version",
            )
            .bind(&self.tenant)
            .bind(&self.project)
            .bind(&ticket.id)
            .bind(now)
            .fetch_one(&mut *connection)
            .await?;
            self.event(connection,actor,now,"issue.context_added",Some(&ticket.id),payload.fence,
                serde_json::json!({"note_seq":note.seq,"request_id":payload.request_id,"revision":revision})).await?;
            sqlx::query(
                "INSERT INTO board_context_receipts (tenant_id,board_id,actor,request_id,payload_hash,ticket,note_seq,revision)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
            ).bind(&self.tenant).bind(&self.project).bind(actor).bind(&payload.request_id).bind(&payload_hash)
                .bind(&ticket.id).bind(note.seq).bind(revision).execute(&mut *connection).await?;
            Ok(ContextReceipt {
                request_id: payload.request_id.clone(),
                ticket: ticket.id,
                note_seq: note.seq,
                revision,
            })
        })
    }
}
