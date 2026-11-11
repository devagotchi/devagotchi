//! `/connect`: how to connect each AI client to this hub, with this hub's own address filled
//! in, and `GET /api/v1/connectors`, the same thing for an agent to read. Both come from the
//! catalog (`devagotchi_connectors`), so a client added there appears here.

use axum::{
    Json,
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use devagotchi_connectors::{self as connectors, Proof, Rendered, Surface};

use super::pages::{self, escape};
use crate::{AppState, site::discovery};

fn mcp_url(state: &AppState) -> String {
    format!("{}/mcp", state.hub.config.public_url.trim_end_matches('/'))
}

fn card(connector: &Rendered) -> String {
    let mut body = String::new();
    if connector.remote_mcp == Some(false) {
        body.push_str(
            "<p class=\"note\">Its documentation does not offer a remote MCP connection, so it \
reaches a hub through what you build around it.</p>",
        );
    }
    if let Some(limits) = &connector.plan_limits {
        body.push_str(&format!("<p class=\"note\">{}</p>", escape(limits)));
    }
    if let Some(command) = &connector.command {
        body.push_str(&format!(
            "<div class=\"command\"><pre><code>{}</code></pre></div>",
            escape(command)
        ));
    }
    if let Some(paths) = &connector.config_path {
        let rows: String = [
            ("macOS", &paths.macos),
            ("Linux", &paths.linux),
            ("Windows", &paths.windows),
        ]
        .iter()
        .filter_map(|(os, path)| {
            path.as_ref()
                .map(|path| format!("<dt>{os}</dt><dd><code>{}</code></dd>", escape(path)))
        })
        .collect();
        if !rows.is_empty() {
            body.push_str(&format!("<dl class=\"facts\">{rows}</dl>"));
        }
    }
    if let Some(path) = &connector.project_config_path {
        body.push_str(&format!(
            "<p class=\"meta\">For one project: <code>{}</code></p>",
            escape(path)
        ));
    }
    if let Some(snippet) = &connector.snippet {
        body.push_str(&format!(
            "<div class=\"command\"><pre><code>{}</code></pre></div>",
            escape(snippet)
        ));
    }
    if let Some(with_key) = &connector.key_snippet {
        body.push_str(&format!(
            "<details class=\"gaps\"><summary>With an API key</summary>\
<div class=\"command\"><pre><code>{}</code></pre></div></details>",
            escape(with_key)
        ));
    }
    if !connector.steps.is_empty() {
        let steps: String = connector
            .steps
            .iter()
            .map(|step| format!("<li>{}</li>", escape(step)))
            .collect();
        body.push_str(&format!("<ol class=\"connect-steps\">{steps}</ol>"));
    }
    if let Some(file) = &connector.instructions_file {
        body.push_str(&format!(
            "<p class=\"meta\">Project instructions: <code>{}</code></p>",
            escape(file)
        ));
    }
    let proof = match connector.proof {
        Proof::Ci => format!(
            "A real client connects in this project's tests. Checked {}.",
            escape(&connector.checked_on)
        ),
        Proof::Docs => format!(
            "From the vendor's documentation, checked {}.",
            escape(&connector.checked_on)
        ),
    };
    let sources: String = connector
        .docs
        .iter()
        .take(1)
        .map(|url| {
            format!(
                " <a href=\"{u}\" rel=\"noreferrer\">Source</a>",
                u = escape(url)
            )
        })
        .collect();
    body.push_str(&format!("<p class=\"meta\">{proof}{sources}</p>"));
    if let Some(gaps) = &connector.gaps {
        body.push_str(&format!(
            "<details class=\"gaps\"><summary>What the documentation did not confirm</summary><p class=\"meta\">{}</p></details>",
            escape(gaps)
        ));
    }
    let tag = if connector.proof == Proof::Ci {
        "<span class=\"tag kind\">Tested</span>"
    } else {
        ""
    };
    format!(
        "<details class=\"connector\" id=\"connector-{id}\"><summary>{name} {tag}</summary>{body}</details>",
        id = escape(&connector.id),
        name = escape(&connector.name),
    )
}

/// `/connect`
pub async fn page(State(state): State<AppState>) -> Response {
    let url = mcp_url(&state);
    let rendered: Vec<Rendered> = connectors::all().iter().map(|c| c.render(&url)).collect();
    let groups: String = Surface::ALL
        .iter()
        .filter_map(|surface| {
            let cards: String = rendered
                .iter()
                .filter(|connector| connector.surface == *surface)
                .map(card)
                .collect();
            (!cards.is_empty()).then(|| format!("<h2>{}</h2>{cards}", escape(surface.title())))
        })
        .collect();
    let title = "Connect your AI to your own servers";
    let body = format!(
        "<h1>{title}</h1><p>Every client below connects to this hub's one address. Pick yours for \
the exact steps. Most sign in with OAuth. For an agent that cannot, make an \
<a href=\"/app/keys\">API key</a> limited to what it needs, for as long as it needs it.</p>\
<div class=\"command\"><pre><code>{}</code></pre></div>{groups}",
        escape(&url)
    );
    let site = state.hub.config.site();
    let head = discovery::head(
        &site,
        &state.hub.config.public_url,
        &discovery::Meta {
            path: "/connect",
            title,
            description: "Exact setup for connecting Claude, ChatGPT, Codex, Cursor, VS Code, Gemini, Grok and your own agents to your servers through one remote MCP address.",
            noindex: false,
            json_ld: Vec::new(),
        },
    );
    pages::public(StatusCode::OK, title, &body, &head)
}

/// `GET /api/v1/connectors`
pub async fn api(State(state): State<AppState>) -> Response {
    let url = mcp_url(&state);
    let body = serde_json::json!({
        "schema": connectors::SCHEMA,
        "mcp_url": url,
        "connectors": connectors::all().iter().map(|c| c.render(&url)).collect::<Vec<_>>(),
    });
    ([(header::CACHE_CONTROL, "public, max-age=300")], Json(body)).into_response()
}

#[cfg(test)]
mod tests {
    use devagotchi_connectors as connectors;

    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_page_and_the_api_give_each_client_this_hubs_address() {
        let hub = crate::tenant_tests::hosted_with(None, |_, _| serde_json::json!({})).await;
        let address = format!("{}/mcp", hub.url);
        let (status, html) =
            crate::tenant_tests::Hosted::text(hub.get("/connect", None).await).await;
        assert_eq!(status, 200);
        for connector in connectors::all() {
            assert!(
                html.contains(&format!("id=\"connector-{}\"", connector.id)),
                "{} is missing",
                connector.id
            );
        }
        assert!(html.contains(&address), "{html}");
        assert!(
            !html.contains("{url}") && !html.contains("{name}"),
            "{html}"
        );
        assert!(html.contains("rel=\"canonical\""), "{html}");

        let answer: serde_json::Value = hub
            .get("/api/v1/connectors", None)
            .await
            .json()
            .await
            .unwrap();
        assert_eq!(answer["schema"], connectors::SCHEMA);
        assert_eq!(answer["mcp_url"], address);
        let list = answer["connectors"].as_array().unwrap();
        assert_eq!(list.len(), connectors::all().len());
        let codex = list.iter().find(|c| c["id"] == "codex-cli").unwrap();
        assert_eq!(codex["proof"], "ci");
        assert!(
            codex["command"].as_str().unwrap().contains(&address),
            "{codex}"
        );
    }

    #[test]
    fn a_card_escapes_and_says_how_it_is_known() {
        let connector = connectors::all().first().unwrap();
        let mut rendered = connector.render("https://hub.test/mcp?a=<b>");
        rendered.command = Some("run <this> & that".into());
        let html = card(&rendered);
        assert!(html.contains("run &lt;this&gt; &amp; that"), "{html}");
        assert!(!html.contains("<this>"), "{html}");
        assert!(html.contains("checked"), "{html}");
    }
}
