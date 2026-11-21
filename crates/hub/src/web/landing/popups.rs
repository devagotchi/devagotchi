//! The landing page's popups beside the comparison's evidence (`compare.rs`): how claiming
//! works and every tool the AI gets. Each is a `popover` opened by a
//! `popovertarget` button: no script, light dismiss, Escape closes it. A popup is on the
//! page only when something on it opens it.

use super::{Ctx, modal};

/// The popups something on this page opens, in the order they close the page.
pub fn all(ctx: &Ctx) -> String {
    let mut out = String::new();
    if ctx.opens("claim") {
        out.push_str(&claim(ctx));
    }
    if ctx.opens("tools") {
        out.push_str(&tools(ctx));
    }
    out
}

/// `<li><strong>Title.</strong> Text</li>` for each step.
fn steps(ctx: &Ctx, steps: &[crate::site::Item]) -> String {
    let items: String = steps
        .iter()
        .map(|step| {
            let text = step
                .text
                .pick("default")
                .map(|t| ctx.inline(t))
                .unwrap_or_default();
            let text = if text.is_empty() {
                text
            } else {
                format!(" {text}")
            };
            format!("<li><strong>{}</strong>{text}</li>", ctx.inline(&step.term))
        })
        .collect();
    format!("<ol class=\"modal-steps\">{items}</ol>")
}

/// How a hub with no owner is claimed (`identity/accounts/claim.rs`, `web/claim.rs`), in plain words.
fn claim(ctx: &Ctx) -> String {
    let copy = &ctx.site.landing.popups.claim;
    let paragraphs: String = copy
        .paragraphs
        .iter()
        .map(|p| format!("<p>{}</p>", ctx.inline(p)))
        .collect();
    let source = if copy.source.is_empty() {
        String::new()
    } else {
        format!("<p class=\"modal-source\">{}</p>", ctx.inline(&copy.source))
    };
    modal(
        "m-claim",
        &ctx.inline(&copy.title),
        &format!("{}{paragraphs}{source}", steps(ctx, &copy.steps)),
    )
}

/// The hub's MCP tools, grouped by what they are for. They are the hub's own, so they stay
/// with its code: a test holds this to the tools the hub serves (`hub::tool_names`).
pub const TOOLS: [(&str, &[(&[&str], &str)]); 8] = [
    (
        "Find your machines",
        &[
            (
                &["machines"],
                "Every machine your AI can work on, and whether its link is connected. It starts here.",
            ),
            (
                &["machine_inspect"],
                "Asks a machine's broker itself whether it answers and what it serves, within 3 seconds. No paths or secrets, and nothing started.",
            ),
            (
                &["machine_add"],
                "Adds a machine to your account: a one-time code and the line that installs it, for Linux, Windows, Docker Compose, Proxmox or Incus.",
            ),
            (
                &["machine_update"],
                "Updates a machine to the hub's current build now, after its running jobs finish. Machines also update themselves within the hour.",
            ),
            (
                &["whoami"],
                "Who it is signed in as, and the boards it may use.",
            ),
            (
                &["workflow_list", "workflow_start"],
                "Offers a setup (a server, a PC, your phone) and starts it as one link you open: no sign-in, one button, one line to paste.",
            ),
            (
                &["workflow_status", "workflow_wait", "workflow_cancel"],
                "Follows that link's steps and carries on the moment each one is done, or cancels the link.",
            ),
        ],
    ),
    (
        "Request machine access",
        &[
            (
                &["machines_list", "capabilities_list"],
                "Lists your enrolled machines and the kinds of access you can request, including which restrictions a connected broker supports.",
            ),
            (
                &["request_access", "wait_for_access"],
                "Asks the environment owner to review a specific scope and duration, with your reason, and waits for the decision. A pending request grants no access.",
            ),
            (
                &["my_grants"],
                "Shows the signed-in person's or agent's machine grants, their limits, and whether they have ended.",
            ),
        ],
    ),
    (
        "Run commands",
        &[
            (
                &["run"],
                "Runs a command and returns its output in one call. Still running after the wait, it keeps running as a job.",
            ),
            (
                &["operation_start"],
                "Starts a job the machine's broker owns: for long work, or on a machine that runs only the programs you granted.",
            ),
            (
                &["operation_wait"],
                "Waits until a job finishes or prints something new.",
            ),
            (
                &["operation_list", "operation_get", "operation_read"],
                "Your jobs on a machine, also from before a restart; a job's state, and its output after a cursor.",
            ),
            (
                &["operation_input", "operation_resize"],
                "Types into a job that asks for input (a prompt, a REPL, a sign-in) and resizes its terminal. Only the conversation that started it types there.",
            ),
            (
                &["operation_cancel"],
                "Stops a job. Disconnecting never does.",
            ),
            (
                &["list_processes", "kill_process"],
                "Sees every process on the machine, busiest first, and stops one. Never the broker itself.",
            ),
            (
                &["read_url"],
                "Reads a web address as the machine reaches it, its own localhost included.",
            ),
            (
                &["get_recent_tool_calls", "get_usage_stats"],
                "Your own recent calls and how they went, so an AI picks up where it was. Never anyone else's.",
            ),
            (
                &["get_config", "set_config_value"],
                "Reads the machine's settings, and changes how much output and how many jobs it keeps. What it may run stays yours to set.",
            ),
        ],
    ),
    (
        "Work with files",
        &[
            (
                &["read_file", "read_multiple_files"],
                "Reads text files by lines, up to 20 in one call.",
            ),
            (&["read_image"], "Looks at a PNG, JPEG, GIF or WebP image."),
            (
                &["list_directory", "get_file_info"],
                "Lists a folder, or says what a path is without reading it.",
            ),
            (
                &["search_files"],
                "Finds files by name, or lines inside them, as ripgrep would.",
            ),
            (
                &["write_file", "edit_file"],
                "Writes a file whole, never half-written, or replaces exact text in one.",
            ),
            (
                &["create_directory", "move_file"],
                "Makes folders, and moves or renames on the same filesystem.",
            ),
        ],
    ),
    (
        "Ask you first",
        &[(
            &["approval_request", "approval_wait"],
            "Asks you to approve a step, with what will change, why and how to undo it, then waits for your answer.",
        )],
    ),
    (
        "Share a board with other agents",
        &[(
            &[
                "board_prime",
                "board_ready",
                "board_list",
                "board_get",
                "board_create",
                "board_claim",
                "board_heartbeat",
                "board_update",
                "board_note",
                "board_handoff",
                "board_release",
                "board_complete",
                "board_dep",
                "board_force_release",
                "board_events",
                "board_wait",
                "board_verify",
                "board_bind",
                "board_attach",
                "board_issue_context",
                "board_issue_checkpoint",
                "board_settings",
                "board_inbox",
                "board_wake",
            ],
            "A project's ticket board: attach shared work, read issue context, claim and keep the claim alive, checkpoint advice and evidence, hand off, finish.",
        )],
    ),
    (
        "Call your services",
        &[(
            &["connections", "connection_call"],
            "Calls the services you connected (Resend, Stripe, GitHub, Slack and more) through the hub: your agent never sees their keys.",
        )],
    ),
    (
        "Keep your words",
        &[
            (
                &["lexicon_resolve", "dictionary_get"],
                "Says what a phrase means to you and your project, before the dictionary of product boundaries and stacks it ships with.",
            ),
            (
                &["lexicon_propose", "lexicon_review"],
                "Adds a phrase as suggested, unclear or yours, and lets you confirm, retire, reword or delete it.",
            ),
            (
                &["lexicon_handoff"],
                "Hands your words to Clyffy and takes them back, one keeper at a time, with any two meanings you confirmed left for you to pick.",
            ),
        ],
    ),
];

/// The offers' tools, listed under the site's network while its offers show
/// (`crate::offers`).
pub const OFFER_TOOLS: &[(&[&str], &str)] = &[(
    &["offers", "offers_accept"],
    "Says what the network offers, asked once after your first machine works, and links your account when you say yes.",
)];

fn tools(ctx: &Ctx) -> String {
    let copy = &ctx.site.landing.popups.tools;
    let network = ctx
        .site
        .info
        .offers()
        .map(|offers| (ctx.plain(&offers.brand), OFFER_TOOLS));
    let groups: String = TOOLS
        .iter()
        .map(|(group, tools)| (group.to_string(), *tools))
        .chain(network)
        .map(|(group, tools)| {
            let rows: String = tools
                .iter()
                .map(|(names, what)| {
                    let names: String = names
                        .iter()
                        .map(|name| format!("<code>{name}</code>"))
                        .collect::<Vec<_>>()
                        .join(" ");
                    format!("<dt>{names}</dt><dd>{what}</dd>")
                })
                .collect();
            format!(
                "<h3>{}</h3><dl class=\"tools\">{rows}</dl>",
                crate::site::text::escape_text(&group)
            )
        })
        .collect();
    modal(
        "m-tools",
        &ctx.inline(&copy.title),
        &format!("<p>{}</p>{groups}", ctx.inline(&copy.intro)),
    )
}
