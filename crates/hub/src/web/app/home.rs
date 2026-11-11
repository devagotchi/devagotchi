//! `/app`, the app's home: three cards — Needs you (approvals waiting, tickets assigned to
//! the person, machines whose update failed; newest first), Machines (each online or
//! offline, its build, "updating" while it is) and Boards (each project with how many
//! tickets are open). `/app/machines` and `/app/boards` are those two cards on their own.

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Response,
};

use super::{board_names, card, open_board, person, words};
use crate::{
    AppState,
    auth::Subject,
    board::{Filter, approval},
    hub::Machine,
    web::pages::{self, Tab, escape},
};

/// One board as the home page shows it: its open tickets, and those assigned to the person.
struct BoardSummary {
    project: String,
    open: usize,
    /// (updated at, ms; id; title)
    assigned: Vec<(i64, String, String)>,
}

pub async fn home(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let subject = match person(&state, &headers, "/app").await {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    let machines = state.hub.machine_status(&subject).await;
    let approvals = if state.hub.may_approve(&subject) {
        match super::super::approvals::pending(&state, &subject).await {
            Ok(approvals) => approvals,
            Err(error) => {
                eprintln!("devagotchi hub: home approvals: {error}");
                return crate::web::routes::unavailable();
            }
        }
    } else {
        Vec::new()
    };
    let boards = match summaries(&state, &subject).await {
        Ok(boards) => boards,
        Err(error) => {
            eprintln!("devagotchi hub: board summaries: {error}");
            return crate::web::routes::unavailable();
        }
    };
    let site = words();
    let copy = &site.messages.app;
    // Needs you, newest first: a failed update has no time of its own, so it leads.
    let mut needs: Vec<(i64, String)> = Vec::new();
    for machine in &machines {
        if machine
            .update
            .as_deref()
            .is_some_and(|u| u.starts_with("failed"))
        {
            needs.push((
                i64::MAX,
                format!(
                    "<li><span class=\"what-kind\">{}</span> <a href=\"/app/machines/{}\">{}</a></li>",
                    escape(&copy.update_failed),
                    crate::web::encode(&machine.name),
                    escape(&machine.name)
                ),
            ));
        }
    }
    for (project, waiting) in &approvals {
        needs.push((
            waiting.requested_at,
            format!(
                "<li><span class=\"what-kind\">{}</span> <a href=\"/approvals#{p}-{i}\">{}</a> \
<span class=\"meta\">{p}</span></li>",
                escape(&copy.approval_waiting),
                escape(&waiting.title),
                p = escape(project),
                i = escape(&waiting.id),
            ),
        ));
    }
    for board in &boards {
        for (at, id, title) in &board.assigned {
            needs.push((
                *at,
                format!(
                    "<li><span class=\"what-kind\">{}</span> <a href=\"/app/tickets/{}/{}\">{}</a> \
<span class=\"meta\">{} · {}</span></li>",
                    escape(&copy.assigned_to_you),
                    crate::web::encode(&board.project),
                    crate::web::encode(id),
                    escape(title),
                    escape(id),
                    escape(&board.project),
                ),
            ));
        }
    }
    needs.sort_by(|a, b| b.0.cmp(&a.0));
    let needs: Vec<String> = needs.into_iter().map(|(_, row)| row).collect();
    let trouble = machines.iter().any(|machine| {
        machine
            .update
            .as_deref()
            .is_some_and(|u| u.starts_with("failed"))
    });
    let me = super::me::identity(&state, &subject).await;
    let body = format!(
        "{}{}{}{}{}",
        hello(&me, needs.len(), trouble),
        review(approvals.len()),
        card("needs-you", &copy.needs_you, &needs, &copy.needs_you_empty),
        machines_card(&machines),
        boards_card(&boards),
    );
    pages::app_page(StatusCode::OK, &copy.home_title, &body, Tab::Home)
}

pub async fn machines(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let subject = match person(&state, &headers, "/app/machines").await {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    let machines = state.hub.machine_status(&subject).await;
    let site = words();
    let title = &site.messages.app.machines_title;
    pages::app_page(
        StatusCode::OK,
        title,
        &machines_card(&machines),
        Tab::Machines,
    )
}

pub async fn boards(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let subject = match person(&state, &headers, "/app/boards").await {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    let boards = match summaries(&state, &subject).await {
        Ok(boards) => boards,
        Err(error) => {
            eprintln!("devagotchi hub: board summaries: {error}");
            return crate::web::routes::unavailable();
        }
    };
    let site = words();
    let title = &site.messages.app.boards_title;
    // The organisations a person's boards come from, and where they make one (`web/orgs.rs`).
    let orgs = if state.hub.boards.orgs().is_some() {
        format!(
            "<p><a href=\"/orgs\" id=\"orgs-link\">{}</a></p>",
            pages::escape(&site.messages.orgs.link)
        )
    } else {
        String::new()
    };
    pages::app_page(
        StatusCode::OK,
        title,
        &format!("{}{orgs}", boards_card(&boards)),
        Tab::Boards,
    )
}

/// The top of Home: the mascot in the mood of what waits, and who it is talking to.
fn hello(me: &pages::Me, needs: usize, trouble: bool) -> String {
    let site = words();
    let copy = &site.messages.me;
    let first = me.name.split_whitespace().next().unwrap_or_default();
    let greeting = if me.name == copy.tab {
        escape(&copy.hello_nameless)
    } else {
        crate::site::Ctx::new(&site)
            .with("name", first.to_string())
            .inline(&copy.hello)
    };
    let line = match needs {
        0 => escape(&copy.clear),
        1 => escape(&copy.waiting_one),
        n => crate::site::Ctx::new(&site)
            .with("n", n.to_string())
            .inline(&copy.waiting_many),
    };
    let mood = match (needs, trouble) {
        (_, true) => "trouble",
        (0, false) => "clear",
        _ => "waiting",
    };
    let pet = site
        .named(site.info.assets.mascot.as_ref())
        .map(|pet| format!("<span class=\"pet-wrap\">{}</span>", pet.text()))
        .unwrap_or_default();
    format!(
        "<section class=\"hello\" id=\"hello\" data-mood=\"{mood}\">{pet}\
<div><h1>{greeting}</h1><p class=\"meta\">{line}</p></div></section>"
    )
}

/// The one button that matters when approvals wait.
fn review(approvals: usize) -> String {
    let site = words();
    let copy = &site.messages.me;
    let label = match approvals {
        0 => return String::new(),
        1 => escape(&copy.review_one),
        n => crate::site::Ctx::new(&site)
            .with("n", n.to_string())
            .inline(&copy.review_many),
    };
    format!("<a class=\"button primary review\" id=\"review\" href=\"/approvals\">{label}</a>")
}

/// Each of the person's boards: its open tickets and those assigned to the person.
/// Approvals are not tickets here; Needs you lists them on their own.
async fn summaries(
    state: &AppState,
    subject: &Subject,
) -> devagotchi_contracts::BridgeResult<Vec<BoardSummary>> {
    let opened: Vec<_> = board_names(state, subject)?
        .into_iter()
        .map(|project| {
            let board = open_board(state, subject, &project)?.ok_or_else(|| {
                devagotchi_contracts::BridgeError::new(
                    "unknown_board",
                    format!("board {project:?} is no longer available"),
                )
            })?;
            Ok((project, board))
        })
        .collect::<devagotchi_contracts::BridgeResult<Vec<_>>>()?;
    let me = subject.id.clone();
    tokio::task::spawn_blocking(move || {
        opened
            .into_iter()
            .map(|(project, board)| {
                let open: Vec<_> = board
                    .list(&Filter {
                        open_only: true,
                        ..Filter::default()
                    })?
                    .into_iter()
                    .filter(|ticket| ticket.kind != approval::KIND)
                    .collect();
                let assigned = open
                    .iter()
                    .filter(|ticket| ticket.assignee.as_deref() == Some(me.as_str()))
                    .map(|ticket| (ticket.updated_at, ticket.id.clone(), ticket.title.clone()))
                    .collect();
                Ok(BoardSummary {
                    project,
                    open: open.len(),
                    assigned,
                })
            })
            .collect()
    })
    .await
    .map_err(|error| devagotchi_contracts::BridgeError::new("board_store", error.to_string()))?
}

/// A machine's status as the app says it.
pub(super) fn status_word(status: &str) -> String {
    let site = words();
    let copy = &site.messages.app;
    match status {
        "online" => copy.online.clone(),
        "offline" => copy.offline.clone(),
        "revoked" => copy.revoked.clone(),
        _ => copy.not_enrolled.clone(),
    }
}

/// A machine's build, and "updating" while an update runs on it.
fn build_line(machine: &Machine) -> String {
    let site = words();
    let copy = &site.messages.app;
    let mut parts = Vec::new();
    if let Some(build) = &machine.build {
        parts.push(
            crate::site::Ctx::new(&site)
                .with("build", build.as_str())
                .inline(&copy.build),
        );
    }
    if machine.update.as_deref() == Some("updating") {
        parts.push(escape(&copy.updating));
    }
    parts.join(" · ")
}

fn machines_card(machines: &[Machine]) -> String {
    let site = words();
    let copy = &site.messages.app;
    let rows: Vec<String> = machines
        .iter()
        .map(|machine| {
            format!(
                "<li id=\"machine-{n}\"><span class=\"status {}\">{}</span> \
<a href=\"/app/machines/{}\"><strong>{n}</strong></a> <span class=\"meta\">{}</span></li>",
                machine.status,
                escape(&status_word(machine.status)),
                crate::web::encode(&machine.name),
                build_line(machine),
                n = escape(&machine.name),
            )
        })
        .collect();
    card(
        "machines",
        &copy.machines_title,
        &rows,
        &copy.machines_empty,
    )
}

fn boards_card(boards: &[BoardSummary]) -> String {
    let site = words();
    let copy = &site.messages.app;
    let rows: Vec<String> = boards
        .iter()
        .map(|board| {
            let count = crate::site::Ctx::new(&site)
                .with("n", board.open.to_string())
                .inline(&copy.open_count);
            format!(
                "<li id=\"board-{p}\"><a href=\"/app/boards/{}\"><strong>{p}</strong></a> \
<span class=\"meta\">{count}</span></li>",
                crate::web::encode(&board.project),
                p = escape(&board.project),
            )
        })
        .collect();
    card("boards", &copy.boards_title, &rows, &copy.boards_empty)
}
