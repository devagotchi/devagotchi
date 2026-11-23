//! `/app/boards/<project>`: workflow columns and task cards, read with `Board::list`. Filters are
//! chips (state, assignee, label and the anchors machine, environment, repo, agent: each
//! value the tickets have; tapping the chosen one clears it) and a search box over
//! task IDs, notes, titles and descriptions. Approvals are not listed:
//! they are decided on `/approvals`.

use std::collections::BTreeSet;

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::Response,
};
use serde::Deserialize;

use super::super::{open_board, person, words};
use crate::{
    AppState,
    agenthub::{Anchors, Filter, Ticket, approval},
    web::{
        encode,
        pages::{self, Tab, escape},
    },
};

#[derive(Deserialize, Default, Clone)]
pub struct Chosen {
    state: Option<String>,
    assignee: Option<String>,
    label: Option<String>,
    machine: Option<String>,
    environment: Option<String>,
    repo: Option<String>,
    agent: Option<String>,
    q: Option<String>,
    #[serde(default)]
    mine: bool,
}

impl Chosen {
    /// This board's address with these filters, `key` set to `value` (or cleared).
    fn href(&self, project: &str, key: &str, value: Option<&str>) -> String {
        let mut next = self.clone();
        if key == "mine" {
            next.mine = value.is_some();
        } else {
            let slot = match key {
                "state" => &mut next.state,
                "assignee" => &mut next.assignee,
                "label" => &mut next.label,
                "machine" => &mut next.machine,
                "environment" => &mut next.environment,
                "repo" => &mut next.repo,
                "agent" => &mut next.agent,
                _ => &mut next.q,
            };
            *slot = value.map(str::to_string);
        }
        let mut query: Vec<String> = [
            ("state", &next.state),
            ("assignee", &next.assignee),
            ("label", &next.label),
            ("machine", &next.machine),
            ("environment", &next.environment),
            ("repo", &next.repo),
            ("agent", &next.agent),
            ("q", &next.q),
        ]
        .iter()
        .filter_map(|(k, v)| {
            v.as_deref()
                .filter(|v| !v.is_empty())
                .map(|v| format!("{k}={}", encode(v)))
        })
        .collect();
        if next.mine {
            query.push("mine=true".into());
        }
        let path = format!("/app/boards/{}", encode(project));
        if query.is_empty() {
            path
        } else {
            format!("{path}?{}", query.join("&"))
        }
    }
}

pub async fn page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(project): Path<String>,
    Query(chosen): Query<Chosen>,
) -> Response {
    let here = format!("/app/boards/{}", encode(&project));
    let subject = match person(&state, &headers, &here).await {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    let site = words();
    let copy = &site.messages.app;
    let opened = match open_board(&state, &subject, &project) {
        Ok(board) => board,
        Err(error) => {
            eprintln!("devagotchi hub: open board: {error}");
            return crate::web::routes::unavailable();
        }
    };
    let Some(board) = opened else {
        return pages::app_page(
            StatusCode::NOT_FOUND,
            &copy.board_unknown,
            &format!(
                "<h1>{}</h1><p class=\"note\">{}</p>",
                escape(&copy.board_unknown),
                escape(&copy.board_unknown_text)
            ),
            Tab::Boards,
        );
    };
    let filter = Filter {
        state: chosen.state.clone(),
        assignee: chosen.assignee.clone(),
        label: chosen.label.clone(),
        text: chosen.q.clone().filter(|q| !q.trim().is_empty()),
        holder: chosen.mine.then(|| subject.id.clone()),
        anchors: Anchors {
            machine: chosen.machine.clone(),
            environment: chosen.environment.clone(),
            repo: chosen.repo.clone(),
            agent: chosen.agent.clone(),
            ..Anchors::default()
        },
        ..Filter::default()
    };
    let read = tokio::task::spawn_blocking(move || {
        let open = |filter: &Filter| -> devagotchi_contracts::BridgeResult<Vec<Ticket>> {
            Ok(board
                .list(filter)?
                .into_iter()
                .filter(|ticket| ticket.kind != approval::KIND)
                .collect())
        };
        let all = open(&Filter::default())?;
        let shown = open(&filter)?;
        let state_counts = if filter.state.is_some() {
            open(&Filter {
                state: None,
                ..filter
            })?
        } else {
            shown.clone()
        };
        devagotchi_contracts::BridgeResult::Ok((
            all,
            shown,
            state_counts,
            board.configuration_state()?,
        ))
    })
    .await;
    let (all, shown, state_counts, configuration) = match read {
        Ok(Ok(value)) => value,
        other => {
            eprintln!("devagotchi hub: read board page: {other:?}");
            return crate::web::routes::unavailable();
        }
    };
    let workflow = &configuration.workflow;
    let me = subject.id.as_str();
    let who = |assignee: &str| {
        if assignee == me {
            copy.you.clone()
        } else {
            assignee.to_string()
        }
    };
    let chips = |key: &str, title: &str, values: BTreeSet<String>, current: &Option<String>| {
        if values.is_empty() {
            return String::new();
        }
        let links: String = values
            .iter()
            .map(|value| {
                let on = current.as_deref() == Some(value.as_str());
                let label = if key == "assignee" {
                    who(value)
                } else if key == "state" {
                    workflow
                        .state(value)
                        .map_or_else(|| value.clone(), |s| s.label.clone())
                } else {
                    value.clone()
                };
                format!(
                    "<a class=\"chip\" href=\"{}\"{}>{}</a>",
                    escape(&chosen.href(&project, key, (!on).then_some(value.as_str()))),
                    if on { " aria-current=\"true\"" } else { "" },
                    escape(&label)
                )
            })
            .collect();
        format!(
            "<p class=\"chips\" id=\"chips-{key}\"><span class=\"chips-title\">{}</span>{links}</p>",
            escape(title)
        )
    };
    let state_links: String = workflow
        .states
        .iter()
        .map(|state| {
            let on = chosen.state.as_deref() == Some(state.key.as_str());
            let count = state_counts
                .iter()
                .filter(|ticket| ticket.state == state.key)
                .count();
            format!(
                "<a class=\"chip\" href=\"{}\"{}>{} <span class=\"chip-count\">{count}</span></a>",
                escape(&chosen.href(&project, "state", (!on).then_some(state.key.as_str()))),
                if on { " aria-current=\"true\"" } else { "" },
                escape(&state.label)
            )
        })
        .collect();
    let state_chips = format!(
        "<p class=\"chips\" id=\"chips-state\"><span class=\"chips-title\">{}</span>{state_links}</p>",
        escape(&copy.filter_state)
    );
    let assignees: BTreeSet<String> = all.iter().filter_map(|t| t.assignee.clone()).collect();
    let labels: BTreeSet<String> = all.iter().flat_map(|t| t.labels.clone()).collect();
    let anchor = |of: fn(&Ticket) -> &Option<String>| -> BTreeSet<String> {
        all.iter().filter_map(|t| of(t).clone()).collect()
    };
    let list = super::board_cards::columns(
        &project,
        &shown,
        workflow,
        &configuration.settings,
        copy,
        me,
        chosen.state.as_deref(),
    );
    let summary = copy
        .board_view
        .summary
        .replace("{n}", &shown.len().to_string())
        .replace(
            "{held}",
            &shown
                .iter()
                .filter(|t| t.holder.is_some() && !t.lease_expired)
                .count()
                .to_string(),
        )
        .replace("{workflow}", &workflow.name);
    let mine = format!(
        "<a class=\"chip\" href=\"{}\"{}>{}</a>",
        escape(&chosen.href(&project, "mine", (!chosen.mine).then_some("true"))),
        if chosen.mine {
            " aria-current=\"true\""
        } else {
            ""
        },
        escape(&copy.board_view.held_by_me)
    );
    let mut hidden: String = [
        ("state", &chosen.state),
        ("assignee", &chosen.assignee),
        ("label", &chosen.label),
        ("machine", &chosen.machine),
        ("environment", &chosen.environment),
        ("repo", &chosen.repo),
        ("agent", &chosen.agent),
    ]
    .iter()
    .filter_map(|(k, v)| {
        v.as_deref().map(|v| {
            format!(
                "<input type=\"hidden\" name=\"{k}\" value=\"{}\">",
                escape(v)
            )
        })
    })
    .collect();
    if chosen.mine {
        hidden.push_str("<input type=\"hidden\" name=\"mine\" value=\"true\">");
    }
    let search = format!(
        "<form class=\"search\" method=\"get\" action=\"{here}\" role=\"search\">{hidden}\
<label class=\"visually-hidden\" for=\"q\">{l}</label><input type=\"search\" id=\"q\" name=\"q\" value=\"{}\" \
placeholder=\"{l}\"><button type=\"submit\">{}</button></form>",
        escape(chosen.q.as_deref().unwrap_or_default()),
        escape(&copy.search_button),
        l = escape(&copy.search_label),
    );
    let configure = match state.hub.board_access(&subject, &project) {
        Ok((_, _, role)) if role.may_configure() => format!(
            "<a class=\"button\" href=\"/app/boards/{}/settings\">{}</a>",
            encode(&project),
            escape(&copy.configuration.title)
        ),
        Ok(_) => String::new(),
        Err(_) => return crate::web::routes::unavailable(),
    };
    let create = match state.hub.board_access(&subject, &project) {
        Ok((_, _, role)) if role.is_person() => format!(
            "<a class=\"button primary\" href=\"/app/boards/{}/new\">{}</a>",
            encode(&project),
            escape(&copy.creation.task)
        ),
        Ok(_) => String::new(),
        Err(_) => return crate::web::routes::unavailable(),
    };
    let audit = format!(
        "<a class=\"button\" href=\"/app/boards/{}/audit\">{}</a>",
        encode(&project),
        escape(&copy.audit.title)
    );
    let body = format!(
        "<div class=\"stack\" data-page=\"board\"><header class=\"cluster board-head\"><h1>{}</h1>{create}{audit}{configure}{search}</header>\
<p class=\"board-summary\">{}</p><p class=\"board-snapshot\">{}</p>\
<aside class=\"board-filters\">{mine}{}{}{}{}{}{}{}</aside>\
<section class=\"board-list\">{list}</section></div>",
        escape(&project),
        escape(&summary),
        escape(&copy.board_view.snapshot),
        state_chips,
        chips(
            "assignee",
            &copy.filter_assignee,
            assignees,
            &chosen.assignee
        ),
        chips("label", &copy.filter_label, labels, &chosen.label),
        chips(
            "machine",
            &copy.filter_machine,
            anchor(|t| &t.machine),
            &chosen.machine
        ),
        chips(
            "environment",
            &copy.filter_environment,
            anchor(|t| &t.environment),
            &chosen.environment
        ),
        chips("repo", &copy.filter_repo, anchor(|t| &t.repo), &chosen.repo),
        chips(
            "agent",
            &copy.filter_agent,
            anchor(|t| &t.agent),
            &chosen.agent
        ),
    );
    pages::app_page(StatusCode::OK, &project, &body, Tab::Boards)
}
