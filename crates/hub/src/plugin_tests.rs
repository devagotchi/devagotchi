//! The plugins in `plugins/` (Claude Code, Codex, and Grok through Claude's format), held to
//! the code they front: the hooks are the handler's own, every tool a skill names is one the
//! hub serves, the connector points at the hosted hub, and Codex's version is this release's.
//! The clients themselves are checked by hand with `claude plugin validate` and a throwaway
//! `CODEX_HOME` (`.ai/dev/plans/plugins.md`, receipts).

use std::path::{Path, PathBuf};

use serde_json::Value;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn json(path: &str) -> Value {
    let full = root().join(path);
    let text = std::fs::read_to_string(&full)
        .unwrap_or_else(|error| panic!("{}: {error}", full.display()));
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("{path}: {error}"))
}

/// A file's text with Windows line endings made plain (a Windows checkout has CRLF).
fn text(path: &str) -> String {
    std::fs::read_to_string(root().join(path))
        .unwrap_or_else(|error| panic!("{path}: {error}"))
        .replace("\r\n", "\n")
}

#[test]
fn the_agenthub_plugin_ships_exactly_the_hooks_the_handler_prints() {
    let shipped = json("plugins/devagotchi-agenthub/hooks/hooks.json");
    let printed: Value =
        serde_json::from_str(&crate::agenthub::cli::plugin_hooks()).expect("plugin hooks");
    assert_eq!(shipped, printed, "regenerate from plugin_hooks()");
    for command in shipped["hooks"]
        .as_object()
        .unwrap()
        .values()
        .flat_map(|groups| groups.as_array().unwrap())
        .flat_map(|group| group["hooks"].as_array().unwrap())
        .map(|hook| hook["command"].as_str().unwrap())
    {
        assert!(command.ends_with("--as auto --if-board"), "{command}");
    }
}

#[test]
fn every_tool_a_skill_names_is_one_the_hub_serves() {
    let tools = crate::hub::tool_names();
    let prefixes = [
        "board_",
        "approval_",
        "operation_",
        "machine_",
        "workflow_",
        "read_",
        "write_",
        "edit_",
        "list_",
        "get_",
        "create_",
        "move_",
    ];
    let mut named = 0;
    for skill in ["agenthub", "machines"] {
        let body = text(&format!("plugins/devagotchi/skills/{skill}/SKILL.md"));
        assert!(
            body.starts_with(&format!("---\nname: {skill}\ndescription: ")),
            "{skill}: frontmatter names it and says when to use it"
        );
        for token in body.split('`').skip(1).step_by(2) {
            let tool_like = ["run", "machines", "whoami"].contains(&token)
                || prefixes.iter().any(|p| token.starts_with(p))
                    && token.bytes().all(|b| b.is_ascii_lowercase() || b == b'_');
            if tool_like {
                named += 1;
                assert!(
                    tools.iter().any(|tool| tool == token),
                    "{skill} names `{token}`, which the hub does not serve"
                );
            }
        }
    }
    assert!(named > 20, "the skills name the tools they teach ({named})");
}

#[test]
fn the_connector_points_at_the_hosted_hub_and_claude_names_its_seat() {
    let hub = crate::account::client::default_hub();
    assert!(hub.starts_with("https://"), "{hub}");
    // The shared default: Codex and Grok read it as is.
    let shared = json("plugins/devagotchi/.mcp.json");
    assert_eq!(shared["mcpServers"]["devagotchi"]["type"], "http");
    assert_eq!(
        shared["mcpServers"]["devagotchi"]["url"],
        format!("{hub}/mcp")
    );
    // Claude Code's manifest replaces it by name: the hub the person set, as Claude's seat.
    let claude = json("plugins/devagotchi/.claude-plugin/plugin.json");
    assert_eq!(claude["name"], "devagotchi");
    assert_eq!(claude["userConfig"]["hub_url"]["default"], hub.as_str());
    let server = &claude["mcpServers"]["devagotchi"];
    assert_eq!(server["url"], "${user_config.hub_url}/mcp");
    let seat = server["headers"]["X-Devagotchi-Seat"].as_str().unwrap();
    assert_eq!(seat, "claude");
    assert_eq!(
        crate::agenthub::cli::plugin_host(|name| (name == "CLAUDE_PLUGIN_ROOT")
            .then(|| std::ffi::OsString::from("/plugins/devagotchi"))),
        Some(seat.to_string()),
        "the hooks' --as auto names the same seat as the connector"
    );
}

#[test]
fn codex_gets_this_release_and_both_marketplaces_list_real_plugins() {
    let codex = json("plugins/devagotchi/.codex-plugin/plugin.json");
    assert_eq!(codex["name"], "devagotchi");
    assert_eq!(codex["version"], env!("CARGO_PKG_VERSION"));
    let claude_market = json(".claude-plugin/marketplace.json");
    let mut listed = Vec::new();
    for entry in claude_market["plugins"].as_array().unwrap() {
        let source = entry["source"].as_str().unwrap();
        let manifest = json(&format!(
            "{}/.claude-plugin/plugin.json",
            source.trim_start_matches("./")
        ));
        assert_eq!(manifest["name"], entry["name"], "{source}");
        assert_eq!(
            manifest["privacyPolicyUrl"],
            "https://app.devagotchi.com/privacy"
        );
        listed.push(entry["name"].as_str().unwrap().to_string());
    }
    assert_eq!(listed, ["devagotchi", "devagotchi-agenthub"]);
    // Codex lists only what has a Codex manifest: the hooks plugin is Claude Code's for now.
    let codex_market = json(".agents/plugins/marketplace.json");
    for entry in codex_market["plugins"].as_array().unwrap() {
        assert_eq!(entry["source"]["source"], "local");
        let path = entry["source"]["path"].as_str().unwrap();
        let manifest = json(&format!(
            "{}/.codex-plugin/plugin.json",
            path.trim_start_matches("./")
        ));
        assert_eq!(manifest["name"], entry["name"]);
    }
}

#[test]
fn auto_names_the_client_whose_plugin_runs_the_hook() {
    use crate::agenthub::cli::plugin_host;
    let host = |set: &[&str]| {
        plugin_host(|name| {
            set.contains(&name)
                .then(|| std::ffi::OsString::from("/some/plugin"))
        })
    };
    assert_eq!(host(&["CLAUDE_PLUGIN_ROOT"]), Some("claude".into()));
    // Codex sets Claude's variable too, for compatibility.
    assert_eq!(
        host(&["PLUGIN_ROOT", "CLAUDE_PLUGIN_ROOT"]),
        Some("codex".into())
    );
    assert_eq!(
        host(&["GROK_PLUGIN_ROOT", "CLAUDE_PLUGIN_ROOT"]),
        Some("grok".into())
    );
    assert_eq!(host(&[]), None);
    assert_eq!(plugin_host(|_| Some(std::ffi::OsString::new())), None);
}
