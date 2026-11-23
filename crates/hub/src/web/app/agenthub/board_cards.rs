//! Source Board.tsx / TaskCard.tsx presentation over the existing board read model.
use crate::{
    agenthub::{Settings, Ticket, Workflow},
    site::app_copy::AppCopy,
    web::{encode, pages::escape},
};

pub(super) fn columns(
    project: &str,
    tickets: &[Ticket],
    workflow: &Workflow,
    settings: &Settings,
    copy: &AppCopy,
    me: &str,
    selected: Option<&str>,
) -> String {
    let selected = selected
        .or_else(|| {
            workflow
                .states
                .iter()
                .find(|state| {
                    !workflow.is_done(&state.key) && tickets.iter().any(|t| t.state == state.key)
                })
                .map(|state| state.key.as_str())
        })
        .or_else(|| {
            workflow
                .states
                .iter()
                .find(|state| tickets.iter().any(|t| t.state == state.key))
                .map(|state| state.key.as_str())
        })
        .or_else(|| workflow.states.first().map(|state| state.key.as_str()));
    let now = super::ticket::now_ms();
    let mut columns = String::new();
    for state in &workflow.states {
        let tasks: Vec<_> = tickets.iter().filter(|t| t.state == state.key).collect();
        if workflow.is_done(&state.key) && tasks.is_empty() && selected != Some(&state.key) {
            continue;
        }
        let cards: String = tasks
            .iter()
            .map(|ticket| card(project, ticket, workflow, settings, copy, me, now))
            .collect();
        let cards = if tasks.is_empty() {
            format!(
                "<p class=\"empty\">{}</p>",
                escape(&copy.board_view.column_empty)
            )
        } else {
            format!("<ul class=\"task-cards\">{cards}</ul>")
        };
        columns.push_str(&format!(
            "<section class=\"board-column{}\" data-state=\"{}\" aria-labelledby=\"state-{}\">\
<header class=\"column-head\"><h2 id=\"state-{}\">{}</h2><span class=\"column-count\">{}</span></header>{cards}</section>",
            if selected == Some(state.key.as_str()) { " is-selected" } else { "" },
            escape(&state.key), escape(&state.key), escape(&state.key), escape(&state.label), tasks.len(),
        ));
    }
    let empty = if tickets.is_empty() {
        format!("<p class=\"empty\">{}</p>", escape(&copy.tickets_empty))
    } else {
        String::new()
    };
    format!("{empty}<div class=\"board-columns\" id=\"tickets\">{columns}</div>")
}

fn card(
    project: &str,
    ticket: &Ticket,
    workflow: &Workflow,
    settings: &Settings,
    copy: &AppCopy,
    me: &str,
    now: i64,
) -> String {
    let words = &copy.board_view;
    let who = |id: &str| escape(if id == me { &copy.you } else { id });
    let labels: String = ticket
        .labels
        .iter()
        .map(|label| format!("<span class=\"task-label\">{}</span>", escape(label)))
        .collect();
    let status = if let Some(holder) = &ticket.holder {
        let lease = if ticket.lease_expired || ticket.lease_expires_at.is_some_and(|at| at <= now) {
            escape(&words.lease_expired)
        } else if let Some(at) = ticket.lease_expires_at {
            escape(
                &words
                    .lease_remaining
                    .replace("{minutes}", &((at - now + 59_999) / 60_000).to_string()),
            )
        } else {
            String::new()
        };
        format!(
            "<span>{}: {}</span><span class=\"lease\">{lease}</span>",
            escape(&copy.ticket_holder),
            who(holder)
        )
    } else if !ticket.blocked_by.is_empty() {
        let links: Vec<_> = ticket
            .blocked_by
            .iter()
            .map(|id| {
                format!(
                    "<a href=\"/app/tickets/{}/{}\">{}</a>",
                    encode(project),
                    encode(id),
                    escape(id)
                )
            })
            .collect();
        format!(
            "<span>{}: {}</span>",
            escape(&words.blocked_by),
            links.join(", ")
        )
    } else if let Some(assignee) = &ticket.assignee {
        format!(
            "<span>{}: {}</span>",
            escape(&words.reserved_for),
            who(assignee)
        )
    } else if workflow.lane_by_queue(&ticket.state).is_some() {
        format!("<span class=\"task-ready\">{}</span>", escape(&words.ready))
    } else if let Some(worker) = &ticket.last_worker {
        format!(
            "<span>{}: {}</span>",
            escape(&words.last_worker),
            who(worker)
        )
    } else {
        String::new()
    };
    let expiries = if ticket.expiry_count > 0 {
        format!(
            "<span class=\"task-expiries\">{}</span>",
            escape(
                &words
                    .expiry_count
                    .replace("{n}", &ticket.expiry_count.to_string())
            )
        )
    } else {
        String::new()
    };
    format!(
        "<li id=\"ticket-{}\" class=\"task-card\"><div class=\"task-top\"><span class=\"task-id\">{}</span>\
<span class=\"task-priority\">{}</span></div><h3><a href=\"/app/tickets/{}/{}\">{}</a></h3>\
<div class=\"task-labels\">{labels}</div><div class=\"task-status\">{status}{expiries}</div></li>",
        escape(&ticket.id),
        escape(&ticket.id),
        escape(&settings.priority_label(ticket.priority)),
        encode(project),
        encode(&ticket.id),
        escape(&ticket.title),
    )
}
