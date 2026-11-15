//! A project bound to a hub (B11): `board bind` makes or finds the account's board there and
//! `.devagotchi/config.json` remembers the hub; every later command on the project runs on the
//! hub's board through the [`Hub`] this command line was given; `board unbind` forgets it.

use super::*;

/// The hub client this command line was given, or why a bound project cannot be reached.
fn signed_in(invocation: &Invocation) -> BridgeResult<&dyn Hub> {
    invocation.hub.as_deref().ok_or_else(|| {
        BridgeError::new(
            "not_signed_in",
            "this command line reaches no hub; run it as `devagotchi board`, signed in with `devagotchi login --hub URL`",
        )
    })
}

/// The hub's board as the account on file; the agent's name is its seat there, a person
/// (the bare OS user) is the account itself.
pub(super) fn remote_face(
    hub: &str,
    config: &ProjectConfig,
    invocation: &Invocation,
    actor: &Actor<'_>,
) -> BridgeResult<Remote> {
    let Some(account_file) = &invocation.account_file else {
        return Err(BridgeError::new(
            "not_signed_in",
            "this project is bound to a hub, but there is no home directory to hold an account; set DEVAGOTCHI_CONFIG_DIR",
        ));
    };
    let seat = (!actor.human).then_some(actor.id);
    let client = signed_in(invocation)?.connect(hub, account_file, seat)?;
    Ok(Remote::new(client, &config.project, &config.prefix))
}

/// `board bind [--hub URL] [--project NAME]`: the hub makes (or finds) the account's board
/// for this project, and `.devagotchi/config.json` remembers the hub. A project without a
/// local board gets its `.devagotchi/` here; one with a local board keeps it, unread from
/// now on (the hub is the board of record; `unbind` goes back).
pub(super) fn bind(
    parsed: &Parsed,
    invocation: &Invocation,
    actor: &Actor<'_>,
    output: &mut Output<'_>,
) -> u8 {
    let project_dir = parsed
        .flag("--dir")
        .map(|d| invocation.cwd.join(d))
        .or_else(|| find_project_dir(&invocation.cwd))
        .unwrap_or_else(|| invocation.cwd.clone());
    let board_dir = project_dir.join(BOARD_DIR);
    let existing = read_project_config(&board_dir).ok();
    let project = parsed
        .flag("--project")
        .map(str::to_string)
        .or_else(|| existing.as_ref().map(|c| c.project.clone()))
        .or_else(|| {
            project_dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "project".into());
    let Some(account_file) = &invocation.account_file else {
        return output.error(
            &BridgeError::new(
                "not_signed_in",
                "no home directory to hold an account; set DEVAGOTCHI_CONFIG_DIR",
            ),
            None,
        );
    };
    let Some(hub) = parsed
        .flag("--hub")
        .map(str::to_string)
        .or_else(|| existing.as_ref().and_then(|c| c.hub.clone()))
        .or_else(|| invocation.hub.as_ref()?.default_hub(account_file))
    else {
        return output.error(
            &BridgeError::new(
                "invalid_cli",
                "pass --hub https://hub.example.com, or set DEVAGOTCHI_HUB, or sign in with `devagotchi login --hub URL`",
            ),
            None,
        );
    };
    // Binding is the account's act, never a seat's: no seat on this session.
    let mut client = match signed_in(invocation).and_then(|h| h.connect(&hub, account_file, None)) {
        Ok(client) => client,
        Err(error) => return output.error(&error, None),
    };
    let shared = parsed.switch("--shared");
    let bound: serde_json::Value = match client.call_tool(
        if shared { "board_attach" } else { "board_bind" },
        serde_json::json!({ "project": project }),
    ) {
        Ok(bound) => bound,
        Err(error) => return output.error(&error, None),
    };
    let shared_prefix = if shared {
        match serde_json::from_value::<Attached>(bound.clone()) {
            Ok(attached) if attached.project == project => Some(attached.prefix),
            _ => {
                return output.error(
                    &BridgeError::new(
                        "hub_error",
                        "shared board attachment returned an invalid project/prefix",
                    ),
                    None,
                );
            }
        }
    } else {
        None
    };
    if let Err(error) = std::fs::create_dir_all(&board_dir) {
        return output.error(&BridgeError::new("board_io", error.to_string()), None);
    }
    let config = ProjectConfig {
        prefix: shared_prefix.unwrap_or_else(|| {
            existing
                .as_ref()
                .map(|c| c.prefix.clone())
                .unwrap_or_else(|| default_prefix(&project))
        }),
        created_at: existing
            .as_ref()
            .map(|c| c.created_at)
            .unwrap_or_else(now_ms),
        project: project.clone(),
        hub: Some(hub.clone()),
    };
    if let Err(error) = write_json(&board_dir.join("config.json"), &config) {
        return output.error(&error, None);
    }
    let gitignore = board_dir.join(".gitignore");
    if !gitignore.exists() {
        let _ = std::fs::write(&gitignore, LOCAL_GITIGNORE);
    }
    let _ = actor;
    output.value(
        &serde_json::json!({ "project": project, "hub": hub, "shared": shared, "created": !shared && bound["created"] == true, "boards": if shared { serde_json::json!([project]) } else { bound["boards"].clone() }, "limit": bound["limit"], "config": board_dir.join("config.json") }),
        || {
            format!(
                "{} is bound to {hub} ({}); every `devagotchi board` command here now runs there as your account, your agents as seats. Local board file, if any, is no longer read. `board unbind` goes back.",
                project,
                if shared { "shared board attached" } else if bound["created"].as_bool() == Some(true) { "board made" } else { "board found" }
            )
        },
    );
    EXIT_OK
}

/// `board unbind`: forget the hub; the local store (if any) is the board again.
pub(super) fn unbind(board_dir: &Path, output: &mut Output<'_>) -> u8 {
    let mut config = match read_project_config(board_dir) {
        Ok(config) => config,
        Err(error) => return output.error(&error, None),
    };
    let Some(hub) = config.hub.take() else {
        output.lines("not bound: this project's board is the local store");
        return EXIT_OK;
    };
    if let Err(error) = write_json(&board_dir.join("config.json"), &config) {
        return output.error(&error, None);
    }
    output.value(&serde_json::json!({ "project": config.project, "unbound_from": hub }), || {
        format!(
            "{} is no longer bound to {hub}; the local store is the board here (run `board init` if there is none). The hub keeps its board.",
            config.project
        )
    });
    EXIT_OK
}
