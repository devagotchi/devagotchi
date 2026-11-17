//! Interactive operations: the input rules on every platform; programs on a pipe and on a
//! terminal typed into for real and resized (a pseudo-terminal on Linux, a pseudo-console on
//! Windows), and on Linux driven only by their own account (the rule is the broker's, not the
//! platform's).

use super::*;

fn caller(account: &str, client: &str) -> Caller {
    Caller {
        account: account.into(),
        client: client.into(),
    }
}

fn interactive(mode: InputMode) -> Option<InteractiveInput> {
    Some(InteractiveInput {
        mode,
        cols: 80,
        rows: 24,
    })
}

fn interactive_start(
    key: &str,
    executable: &Path,
    args: Vec<String>,
    mode: InputMode,
) -> StartInput {
    StartInput {
        idempotency_key: key.into(),
        executable: executable.display().to_string(),
        args,
        command: None,
        cwd: None,
        deadline_ms: 30_000,
        interactive: interactive(mode),
        combined_output: false,
        caller: Some(caller("local", "conversation-a")),
    }
}

#[test]
fn a_terminal_gets_bytes_and_a_pipe_gets_lines_signals_and_an_end() {
    let terminal = input_steps(
        InputMode::Terminal,
        "ls",
        &[Key::Enter, Key::CtrlC, Key::Up],
    )
    .unwrap();
    assert_eq!(terminal, vec![InputStep::Bytes(b"ls\r\x03\x1b[A".to_vec())]);
    let pipe = input_steps(
        InputMode::Pipe,
        "line",
        &[Key::Enter, Key::CtrlC, Key::CtrlD],
    )
    .unwrap();
    assert_eq!(
        pipe,
        vec![
            InputStep::Bytes(b"line\n".to_vec()),
            InputStep::Interrupt,
            InputStep::Close
        ]
    );
    let stop = input_steps(InputMode::Pipe, "", &[Key::CtrlZ]).unwrap_err();
    assert_eq!(stop.code, "no_terminal");
    let after_end = input_steps(InputMode::Pipe, "", &[Key::CtrlD, Key::Enter]).unwrap_err();
    assert_eq!(after_end.code, "invalid_request");
}

#[test]
fn a_list_shows_a_caller_its_own_and_with_account_scope_its_accounts() {
    let mine = caller("hub:jesse", "chat-1");
    let other_chat = caller("hub:jesse", "chat-2");
    let stranger = caller("hub:someone", "chat-9");
    let owned = Some(&mine);
    assert!(visible(owned, Some(&mine), ListScope::Mine));
    assert!(!visible(owned, Some(&other_chat), ListScope::Mine));
    assert!(visible(owned, Some(&other_chat), ListScope::Account));
    assert!(!visible(owned, Some(&stranger), ListScope::Account));
    assert!(
        visible(owned, None, ListScope::Mine),
        "the inspector sees everything"
    );
    // Started with no owner: this machine's own account's.
    let local = caller(LOCAL_ACCOUNT, "adapter-1");
    assert!(visible(None, Some(&local), ListScope::Account));
    assert!(!visible(None, Some(&mine), ListScope::Account));
    // This machine's own user sees what a hub's accounts started here too.
    assert!(visible(owned, Some(&local), ListScope::Account));
    assert!(!visible(owned, Some(&local), ListScope::Mine));
    assert_eq!(
        ListScope::default(),
        ListScope::Account,
        "a restarted client sees its jobs"
    );
}

#[test]
fn an_interactive_start_needs_a_caller_and_a_sane_size() {
    let directory = tempfile::tempdir().unwrap();
    let executable = test_executable();
    let broker = open_broker(&directory, &[&executable]);
    let mut no_caller = interactive_start("no-caller", &executable, vec![], InputMode::Pipe);
    no_caller.caller = None;
    let refused = broker.start(no_caller).unwrap_err();
    assert_eq!(refused.code, "invalid_request", "{refused:?}");
    let mut huge = interactive_start("huge", &executable, vec![], InputMode::Terminal);
    huge.interactive.as_mut().unwrap().cols = 10_000;
    assert_eq!(broker.start(huge).unwrap_err().code, "invalid_request");
}

/// Wait (bounded) until `id`'s output holds `needle`.
async fn output_holding(broker: &Broker, id: &str, needle: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let text = output_text(broker, id);
        if text.contains(needle) {
            return text;
        }
        assert!(Instant::now() < deadline, "no {needle:?} in {text:?}");
        let _ = broker.wait(wait_for(id, None, 200)).await;
        sleep(Duration::from_millis(50)).await;
    }
}

fn typed(id: &str, who: &Caller, text: &str, keys: Vec<Key>, take_over: bool) -> SendInput {
    SendInput {
        operation_id: id.into(),
        text: (!text.is_empty()).then(|| text.into()),
        keys,
        take_over,
        caller: Some(who.clone()),
    }
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn a_program_on_a_pipe_reads_the_line_typed_into_it() {
    let directory = tempfile::tempdir().unwrap();
    let (sh, args) = shell(&["read line; echo got:$line"]);
    let broker = open_broker(&directory, &[&sh]);
    let started = broker
        .start(interactive_start("pipe", &sh, args, InputMode::Pipe))
        .unwrap()
        .operation;
    let id = started.operation_id;
    let view = started.interactive.expect("an interactive view");
    assert_eq!((view.mode, view.input_open), (InputMode::Pipe, true));
    let owner = caller("local", "conversation-a");
    broker
        .input(typed(&id, &owner, "hello", vec![Key::Enter], false))
        .await
        .unwrap();
    let settled = settle(&broker, &id).await;
    assert!(
        matches!(settled.state, OperationState::Succeeded),
        "{settled:?}"
    );
    assert!(output_text(&broker, &id).contains("got:hello"));
    let closed = settled.interactive.expect("still described");
    assert!(!closed.input_open, "a settled operation takes no input");
    let late = broker
        .input(typed(&id, &owner, "late", vec![Key::Enter], false))
        .await
        .unwrap_err();
    assert_eq!(late.code, "input_closed");
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn a_shell_on_a_terminal_answers_what_is_typed_and_ends_on_ctrl_d() {
    let directory = tempfile::tempdir().unwrap();
    let sh = dunce::canonicalize("/bin/sh").unwrap();
    let broker = open_broker(&directory, &[&sh]);
    let id = broker
        .start(interactive_start(
            "terminal",
            &sh,
            vec![],
            InputMode::Terminal,
        ))
        .unwrap()
        .operation
        .operation_id;
    let owner = caller("local", "conversation-a");
    broker
        .input(typed(&id, &owner, "echo $((6*7))", vec![Key::Enter], false))
        .await
        .unwrap();
    output_holding(&broker, &id, "42").await;
    let resized = broker
        .resize(ResizeInput {
            operation_id: id.clone(),
            cols: 120,
            rows: 40,
            take_over: false,
            caller: Some(owner.clone()),
        })
        .await
        .unwrap();
    let size = resized.interactive.unwrap();
    assert_eq!((size.cols, size.rows), (120, 40));
    broker
        .input(typed(&id, &owner, "stty size", vec![Key::Enter], false))
        .await
        .unwrap();
    output_holding(&broker, &id, "40 120").await;
    broker
        .input(typed(&id, &owner, "", vec![Key::CtrlD], false))
        .await
        .unwrap();
    let settled = settle(&broker, &id).await;
    assert!(settled.state.terminal(), "{settled:?}");
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn only_its_account_drives_it_and_two_clients_never_type_at_once() {
    let directory = tempfile::tempdir().unwrap();
    let (sh, args) = shell(&["cat"]);
    let broker = open_broker(&directory, &[&sh]);
    let id = broker
        .start(interactive_start("owned", &sh, args, InputMode::Pipe))
        .unwrap()
        .operation
        .operation_id;
    let owner = caller("local", "conversation-a");
    let same_account = caller("local", "conversation-b");
    let stranger = caller("hub:someone", "chat-9");
    let refused = |error: BridgeError| error.code;
    assert_eq!(
        refused(
            broker
                .input(typed(&id, &stranger, "x", vec![], true))
                .await
                .unwrap_err()
        ),
        "not_found",
        "take_over never reaches another account, which does not learn it exists"
    );
    assert_eq!(
        refused(
            broker
                .input(typed(&id, &same_account, "x", vec![], false))
                .await
                .unwrap_err()
        ),
        "not_your_session"
    );
    // The other conversation takes over and holds the keyboard; the owner then waits.
    broker
        .input(typed(&id, &same_account, "from b", vec![Key::Enter], true))
        .await
        .unwrap();
    assert_eq!(
        refused(
            broker
                .input(typed(&id, &owner, "x", vec![], false))
                .await
                .unwrap_err()
        ),
        "writer_busy"
    );
    output_holding(&broker, &id, "from b").await;
    // Lists: mine for the owner, the account's for the other conversation, nothing else.
    let list = |who: &Caller, scope| {
        broker
            .list(ListInput {
                offset: 0,
                limit: 10,
                scope: Some(scope),
                caller: Some(who.clone()),
            })
            .unwrap()
            .operations
            .len()
    };
    assert_eq!(list(&owner, ListScope::Mine), 1);
    assert_eq!(list(&same_account, ListScope::Mine), 0);
    assert_eq!(list(&same_account, ListScope::Account), 1);
    assert_eq!(list(&stranger, ListScope::Account), 0);
    broker
        .input(typed(&id, &owner, "", vec![Key::CtrlD], true))
        .await
        .unwrap();
    let settled = settle(&broker, &id).await;
    assert!(
        matches!(settled.state, OperationState::Succeeded),
        "{settled:?}"
    );
}

/// `text` without the escape sequences a console writes (ConPTY moves the cursor rather than
/// write runs of spaces).
#[cfg(windows)]
fn plain(text: &str) -> String {
    let mut plain = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            plain.push(c);
            continue;
        }
        match chars.next() {
            Some('[') => {
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
            Some(']') => {
                for c in chars.by_ref() {
                    if c == '\u{7}' {
                        break;
                    }
                }
            }
            _ => {}
        }
        plain.push(' ');
    }
    plain
}

#[cfg(windows)]
#[tokio::test]
async fn on_windows_a_pipe_takes_lines_and_an_end_but_no_ctrl_c() {
    let directory = tempfile::tempdir().unwrap();
    let sort = system32("sort.exe");
    let broker = open_broker(&directory, &[&sort]);
    let id = broker
        .start(interactive_start("pipe", &sort, vec![], InputMode::Pipe))
        .unwrap()
        .operation
        .operation_id;
    let owner = caller("local", "conversation-a");
    broker
        .input(typed(&id, &owner, "pear\nabove", vec![Key::Enter], false))
        .await
        .unwrap();
    let refused = broker
        .input(typed(&id, &owner, "", vec![Key::CtrlC], false))
        .await
        .unwrap_err();
    assert_eq!(refused.code, "no_terminal", "{refused:?}");
    broker
        .input(typed(&id, &owner, "", vec![Key::CtrlD], false))
        .await
        .unwrap();
    let settled = settle_within(&broker, &id, 60).await;
    assert!(
        matches!(settled.state, OperationState::Succeeded),
        "{settled:?}"
    );
    let text = output_text(&broker, &id);
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    assert_eq!(lines, ["above", "pear"], "{text:?}");
    let late = broker
        .input(typed(&id, &owner, "late", vec![Key::Enter], false))
        .await
        .unwrap_err();
    assert_eq!(late.code, "input_closed");
}

#[cfg(windows)]
#[tokio::test]
async fn on_windows_a_console_runs_what_is_typed_takes_ctrl_c_and_is_resized() {
    let directory = tempfile::tempdir().unwrap();
    let cmd = system32("cmd.exe");
    let ping = system32("PING.EXE");
    // Named in full: the environment is cleared, so there is no PATH.
    let mode = format!("{} con", system32("mode.com").display());
    let broker = open_broker(&directory, &[&cmd]);
    let started = broker
        .start(interactive_start(
            "console",
            &cmd,
            vec!["/d".into()],
            InputMode::Terminal,
        ))
        .unwrap()
        .operation;
    let id = started.operation_id;
    let owner = caller("local", "conversation-a");
    broker
        .input(typed(&id, &owner, "set /a 6*7", vec![Key::Enter], false))
        .await
        .unwrap();
    output_holding(&broker, &id, "42").await;
    let running = broker.get(&id, None).unwrap();
    assert!(
        running
            .detail
            .as_deref()
            .unwrap_or_default()
            .contains("pseudo-console"),
        "{:?}",
        running.detail
    );
    // Ctrl-C reaches the console's foreground program, as typed at a real one.
    let line = format!("{} -t 127.0.0.1", ping.display());
    broker
        .input(typed(&id, &owner, &line, vec![Key::Enter], false))
        .await
        .unwrap();
    output_holding(&broker, &id, "127.0.0.1:").await;
    broker
        .input(typed(&id, &owner, "", vec![Key::CtrlC], false))
        .await
        .unwrap();
    output_holding(&broker, &id, "Control-C").await;
    let resized = broker
        .resize(ResizeInput {
            operation_id: id.clone(),
            cols: 120,
            rows: 40,
            take_over: false,
            caller: Some(owner.clone()),
        })
        .await
        .unwrap();
    let size = resized.interactive.unwrap();
    assert_eq!((size.cols, size.rows), (120, 40));
    broker
        .input(typed(&id, &owner, &mode, vec![Key::Enter], false))
        .await
        .unwrap();
    let shown = output_holding(&broker, &id, "Columns:").await;
    let after = plain(&shown);
    let columns = after.rsplit("Columns:").next().unwrap().trim_start();
    let digits: String = columns.chars().take_while(char::is_ascii_digit).collect();
    assert_eq!(digits, "120", "{after:?}");
    broker
        .input(typed(&id, &owner, "exit", vec![Key::Enter], false))
        .await
        .unwrap();
    let settled = settle_within(&broker, &id, 60).await;
    assert!(
        matches!(settled.state, OperationState::Succeeded),
        "{settled:?}"
    );
    assert!(!settled.interactive.unwrap().input_open);
}
