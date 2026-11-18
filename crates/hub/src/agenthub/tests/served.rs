//! AgentHub as the hub serves it: the board command line against a real hub (`mod bound`), a
//! new board's getting-started steps from the hub's site. The protocol itself is AgentHub's
//! own tests (`crates/agenthub/src/tests/`).

use crate::agenthub::{Actor, Board, Filter, NewTicket, open_board, welcome};

/// A new board in a Postgres database of its own, for the getting-started tests.
pub(super) struct Fixture {
    pub(super) board: Board,
    _db: devagotchi_store::testing::TestStore,
}

pub(super) async fn fixture() -> Option<Fixture> {
    let test = crate::store::testing::postgres().await?;
    let board = open_board(&test.url, "devagotchi", true).await.unwrap();
    Some(Fixture { board, _db: test })
}

impl Fixture {
    fn new_ticket(&self, actor: &Actor<'_>, title: &str) -> String {
        self.board
            .create(
                actor,
                NewTicket {
                    title: title.into(),
                    ..NewTicket::default()
                },
            )
            .expect("create")
            .id
    }
}

/// A bound project (B11): the command line against the real hub, in process, as one
/// account from two machines; the hub arbitrates, the agents are seats.
pub(super) mod bound {
    use serde_json::{Value, json};

    use crate::agenthub::cli::{EXIT_OK, EXIT_REFUSED, EXIT_UNAVAILABLE};
    use crate::agenthub::tests::serving::{hub, hub_with, run, signed_in, tool};

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn two_machines_one_account_one_board() {
        let dir = tempfile::tempdir().unwrap();
        let url = or_skip!(hub(dir.path()));
        let machine_a = dir.path().join("a").join("demo");
        let machine_b = dir.path().join("b").join("demo");
        std::fs::create_dir_all(&machine_a).unwrap();
        std::fs::create_dir_all(&machine_b).unwrap();
        let account_a = dir.path().join("a").join("account.json");
        let account_b = dir.path().join("b").join("account.json");

        // Not signed in: bind says so, with the command to run, and writes nothing.
        let (code, _, err) = tokio::task::spawn_blocking({
            let (machine_a, account_a, url) = (machine_a.clone(), account_a.clone(), url.clone());
            move || run(&machine_a, &account_a, None, &["bind", "--hub", &url])
        })
        .await
        .unwrap();
        assert_eq!(code, EXIT_REFUSED, "{err}");
        assert!(err.contains("devagotchi login"), "{err}");
        assert!(!machine_a.join(".devagotchi").exists());

        signed_in(&account_a, "tok-member");
        signed_in(&account_b, "tok-member");

        // Machine A binds (the hub makes the board), creates, and claims as seat claude.
        let a = {
            let (machine_a, account_a, url) = (machine_a.clone(), account_a.clone(), url.clone());
            tokio::task::spawn_blocking(move || {
                let (code, bound, err) =
                    run(&machine_a, &account_a, None, &["bind", "--hub", &url]);
                assert_eq!(code, EXIT_OK, "{err}");
                assert_eq!(bound["project"], "demo");
                assert_eq!(bound["created"], true);
                let config: Value = serde_json::from_str(
                    &std::fs::read_to_string(machine_a.join(".devagotchi").join("config.json"))
                        .unwrap(),
                )
                .unwrap();
                assert_eq!(config["hub"], url);
                assert!(config.get("storage").is_none(), "{config}");
                let (code, ticket, err) = run(
                    &machine_a,
                    &account_a,
                    Some("claude"),
                    &["new", "Prove the bound board"],
                );
                assert_eq!(code, EXIT_OK, "{err}");
                let id = ticket["id"].as_str().unwrap().to_string();
                let (code, lease, err) =
                    run(&machine_a, &account_a, Some("claude"), &["claim", &id]);
                assert_eq!(code, EXIT_OK, "{err}");
                assert_eq!(
                    lease["ticket"]["holder"], "claude",
                    "the seat is the holder on the account's board"
                );
                (id, lease["fence"].as_i64().unwrap())
            })
            .await
            .unwrap()
        };
        let (id, fence) = a;

        // Machine B binds the same project (found, not made) and sees the claim at once.
        {
            let (machine_b, account_b, url, id) = (
                machine_b.clone(),
                account_b.clone(),
                url.clone(),
                id.clone(),
            );
            tokio::task::spawn_blocking(move || {
                let (code, bound, err) = run(
                    &machine_b,
                    &account_b,
                    None,
                    &["bind", "--hub", &url, "--project", "demo"],
                );
                assert_eq!(code, EXIT_OK, "{err}");
                assert_eq!(bound["created"], false, "{bound}");
                let (code, prime, err) = run(&machine_b, &account_b, Some("codex"), &["prime"]);
                assert_eq!(code, EXIT_OK, "{err}");
                assert_eq!(prime["actor"], "codex");
                assert_eq!(prime["ready"].as_array().unwrap().len(), 0, "{prime}");
                let (code, _, err) = run(
                    &machine_b,
                    &account_b,
                    Some("codex"),
                    &["beat", &id, "--fence", &fence.to_string()],
                );
                assert_eq!(code, 2, "the other seat does not hold it: {err}");
                assert!(err.contains("not_holder"), "{err}");
                // `held` and `beat` with no id resolve `@me` on the hub.
                let (code, held, err) = run(&machine_b, &account_b, Some("codex"), &["held"]);
                assert_eq!(code, EXIT_OK, "{err}");
                assert_eq!(held.as_array().map(Vec::len), Some(0));
                // The person (no seat) sees the ticket and may force it free.
                let (code, open, _) = run(&machine_b, &account_b, None, &["ls", "--open"]);
                assert_eq!(code, EXIT_OK);
                let open = open.as_array().unwrap();
                let claimed = open.iter().find(|t| t["id"] == id.as_str()).unwrap();
                assert_eq!(claimed["holder"], "claude");
                assert!(
                    open.iter()
                        .any(|t| t["title"] == crate::site::builtin().welcome("").0
                            && t["state"] == "needs_attention"),
                    "a new account board starts with the getting-started steps (B12): {open:?}"
                );
                let (code, freed, err) = run(
                    &machine_b,
                    &account_b,
                    None,
                    &["force", &id, "the demo moves on"],
                );
                assert_eq!(code, EXIT_OK, "{err}");
                assert_eq!(freed["state"], "ready", "{freed}");
            })
            .await
            .unwrap();
        }

        // Machine A: the stale fence is refused by the hub, as a standalone board would.
        {
            let (machine_a, account_a, id) = (machine_a.clone(), account_a.clone(), id.clone());
            tokio::task::spawn_blocking(move || {
                let (code, _, err) = run(&machine_a, &account_a, Some("claude"), &["beat", &id]);
                assert_eq!(code, 2, "{err}");
                // The hub's own chain verifies through the remote face.
                let (code, status, err) = run(&machine_a, &account_a, Some("claude"), &["verify"]);
                assert_eq!(code, EXIT_OK, "{err}");
                assert_eq!(status["ok"], true);
                // Unbind: the standalone board is the board again, and no Postgres names one.
                let (code, _, err) = run(&machine_a, &account_a, None, &["unbind"]);
                assert_eq!(code, EXIT_OK, "{err}");
                let (code, _, err) = run(&machine_a, &account_a, None, &["ls"]);
                assert_eq!(code, EXIT_UNAVAILABLE, "{err}");
                assert!(err.contains("no_storage"), "{err}");
            })
            .await
            .unwrap();
        }

        // Signed out underneath a bound project: every command says what to run.
        std::fs::remove_file(&account_b).unwrap();
        let (code, _, err) = tokio::task::spawn_blocking({
            let (machine_b, account_b) = (machine_b.clone(), account_b.clone());
            move || run(&machine_b, &account_b, Some("codex"), &["prime"])
        })
        .await
        .unwrap();
        assert_eq!(code, EXIT_REFUSED, "{err}");
        assert!(
            err.contains("not_signed_in") && err.contains("devagotchi login"),
            "{err}"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_bound_project_carries_anchors_to_the_hub_and_back() {
        let dir = tempfile::tempdir().unwrap();
        let url = or_skip!(hub(dir.path()));
        let project = dir.path().join("a").join("demo");
        std::fs::create_dir_all(&project).unwrap();
        let account = dir.path().join("a").join("account.json");
        signed_in(&account, "tok-member");
        tokio::task::spawn_blocking(move || {
            let (code, _, err) = run(&project, &account, None, &["bind", "--hub", &url]);
            assert_eq!(code, EXIT_OK, "{err}");
            let (code, ticket, err) = run(
                &project,
                &account,
                Some("claude"),
                &[
                    "new",
                    "anchored",
                    "--machine",
                    "dabo",
                    "--env",
                    "prod",
                    "--session",
                    "s-1",
                ],
            );
            assert_eq!(code, EXIT_OK, "{err}");
            assert_eq!(ticket["machine"], "dabo", "{ticket}");
            assert_eq!(ticket["environment"], "prod");
            assert_eq!(ticket["session"], "s-1");
            let id = ticket["id"].as_str().unwrap().to_string();
            let (code, _, err) = run(&project, &account, Some("claude"), &["new", "plain"]);
            assert_eq!(code, EXIT_OK, "{err}");
            let (code, listed, err) = run(
                &project,
                &account,
                Some("claude"),
                &["ls", "--machine", "dabo"],
            );
            assert_eq!(code, EXIT_OK, "{err}");
            let listed: Vec<&str> = listed
                .as_array()
                .unwrap()
                .iter()
                .map(|t| t["id"].as_str().unwrap())
                .collect();
            assert_eq!(listed, [id.as_str()]);
            let (code, set, err) = run(
                &project,
                &account,
                Some("claude"),
                &[
                    "set",
                    &id,
                    "--version",
                    "1",
                    "--machine",
                    "",
                    "--env",
                    "dev",
                ],
            );
            assert_eq!(code, EXIT_OK, "{err}");
            assert!(set["machine"].is_null(), "cleared through the hub: {set}");
            assert_eq!(set["environment"], "dev");
            let (code, refused, _) = run(
                &project,
                &account,
                Some("claude"),
                &["new", "nowhere", "--env", "moon"],
            );
            assert_eq!(code, EXIT_REFUSED);
            assert_eq!(refused["error"]["code"], "unknown_environment", "{refused}");
        })
        .await
        .unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn board_settings_from_the_command_line_on_the_hub_and_who_may_change_them() {
        const AGENT: &str = "300000000000000007";
        const PERSON: &str = "300000000000000008";
        let dir = tempfile::tempdir().unwrap();
        // A configured board the agent holds a grant for (not a person's), and a person too.
        let url = or_skip!(hub_with(
            dir.path(),
            json!({
                "boards": [{ "project": "shared" }],
                "grants": [{ "subject": AGENT, "boards": ["shared"] },
                           { "subject": PERSON, "boards": ["shared"], "human": true }]
            }),
            &[("tok-agent", AGENT), ("tok-person", PERSON)],
        ));
        let project = dir.path().join("m").join("demo");
        std::fs::create_dir_all(&project).unwrap();
        let account = dir.path().join("m").join("account.json");
        signed_in(&account, "tok-member");
        {
            let (project, account, url) = (project.clone(), account.clone(), url.clone());
            tokio::task::spawn_blocking(move || {
                let (code, _, err) = run(&project, &account, None, &["bind", "--hub", &url]);
                assert_eq!(code, EXIT_OK, "{err}");
                let (code, set, err) = run(
                    &project,
                    &account,
                    Some("claude"),
                    &[
                        "settings",
                        "--repo",
                        "https://github.com/x/demo",
                        "--env",
                        "dev, prod",
                    ],
                );
                assert_eq!(code, EXIT_OK, "{err}");
                assert_eq!(set["repo"], "https://github.com/x/demo", "{set}");
                assert_eq!(set["environments"], json!(["dev", "prod"]));
                let (code, read, err) = run(&project, &account, None, &["settings"]);
                assert_eq!(code, EXIT_OK, "{err}");
                assert_eq!(read, set, "read back as set");
                let (code, ticket, err) = run(
                    &project,
                    &account,
                    Some("claude"),
                    &["new", "after the settings"],
                );
                assert_eq!(code, EXIT_OK, "{err}");
                assert_eq!(ticket["repo"], "https://github.com/x/demo", "P2c: {ticket}");
                let (code, refused, all) = run(
                    &project,
                    &account,
                    None,
                    &["settings", "--repo", "ftp://x/demo"],
                );
                assert_eq!(code, EXIT_REFUSED, "{all}");
                assert_eq!(refused["error"]["code"], "invalid_settings", "{refused}");
                let next = refused["error"]["next"].as_str().unwrap();
                assert!(
                    next.contains("is not an https URL") && next.contains("board settings again"),
                    "{next}"
                );
                let (code, refused, all) =
                    run(&project, &account, None, &["settings", "--env", "Prod"]);
                assert_eq!(code, EXIT_REFUSED, "{all}");
                assert!(
                    refused["error"]["next"]
                        .as_str()
                        .unwrap()
                        .contains("not a lowercase name"),
                    "{refused}"
                );
            })
            .await
            .unwrap();
        }
        // The agent's grant is not a person's: it reads nothing and changes nothing there.
        let refused = tool(
            &url,
            "tok-agent",
            "board_settings",
            json!({ "project": "shared", "repo": "https://github.com/x/shared" }),
        )
        .await;
        assert_eq!(refused["isError"], true, "{refused}");
        assert!(refused.to_string().contains("not_owner"), "{refused}");
        let ask = json!({ "project": "shared" });
        let read = tool(&url, "tok-person", "board_settings", ask).await;
        assert_eq!(read["isError"], false, "{read}");
        assert!(read["structuredContent"]["repo"].is_null(), "{read}");
        // And the member's own board is not the agent's to reach at all.
        let refused = tool(
            &url,
            "tok-agent",
            "board_settings",
            json!({ "project": "demo" }),
        )
        .await;
        assert_eq!(refused["isError"], true, "{refused}");
        assert!(refused.to_string().contains("unknown_board"), "{refused}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_board_starts_with_the_getting_started_steps_once() {
    let f = or_skip!(fixture());
    let hub = Actor::agent("devagotchi");
    let (title, steps) = crate::site::builtin().welcome("https://app.devagotchi.com/");
    let id = f
        .board
        .welcome_if_new(&hub, &title, &steps)
        .expect("welcome")
        .expect("a new board is welcomed");
    let tickets = f.board.list(&Filter::default()).expect("list");
    assert_eq!(tickets.len(), 1);
    let welcome = &tickets[0];
    assert_eq!(welcome.id, id);
    assert_eq!(welcome.title, title);
    assert_eq!(welcome.state, "needs_attention");
    assert_eq!(welcome.labels, vec![welcome::LABEL.to_string()]);
    assert!(
        welcome
            .description
            .contains("https://app.devagotchi.com/approvals")
            && welcome.description.contains("Send me a test approval"),
        "{}",
        welcome.description
    );

    // An agent priming the board sees it under attention, never as work to claim.
    let claude = Actor::agent("claude");
    let prime = f.board.prime(&claude, 10).expect("prime");
    assert!(prime.ready.is_empty() && prime.held.is_empty());
    assert_eq!(
        prime
            .attention
            .iter()
            .map(|t| t.id.as_str())
            .collect::<Vec<_>>(),
        [id.as_str()]
    );

    // Once only, and never on a board that has tickets.
    assert_eq!(
        f.board.welcome_if_new(&hub, &title, &steps).expect("again"),
        None
    );
    let used = or_skip!(fixture());
    used.new_ticket(&claude, "real work");
    assert_eq!(
        used.board
            .welcome_if_new(&hub, &title, &steps)
            .expect("used"),
        None
    );
    assert_eq!(used.board.list(&Filter::default()).expect("list").len(), 1);
    assert!(f.board.verify_chain().expect("chain").ok);
}
