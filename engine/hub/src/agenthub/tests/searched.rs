//! The bound CLI sends its search through MCP to the same PostgreSQL query.
use super::serving::{hub, run, signed_in};
use crate::agenthub::cli::EXIT_OK;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bound_cli_search_finds_task_ids_and_note_text() {
    let dir = tempfile::tempdir().unwrap();
    let url = or_skip!(hub(dir.path()));
    let project = dir.path().join("demo");
    std::fs::create_dir_all(&project).unwrap();
    let account = dir.path().join("account.json");
    signed_in(&account, "tok-member");
    tokio::task::spawn_blocking(move || {
        let (code, _, error) = run(&project, &account, None, &["bind", "--hub", &url]);
        assert_eq!(code, EXIT_OK, "{error}");
        let (code, made, error) = run(&project, &account, None, &["new", "Searchable work"]);
        assert_eq!(code, EXIT_OK, "{error}");
        let id = made["id"].as_str().unwrap();
        let (code, _, error) = run(
            &project,
            &account,
            None,
            &["note", id, "Hidden note evidence"],
        );
        assert_eq!(code, EXIT_OK, "{error}");
        for query in [id, "hidden note evidence", "searchable work"] {
            let (code, result, error) = run(&project, &account, None, &["ls", "--text", query]);
            assert_eq!(code, EXIT_OK, "{error}");
            let tickets = result.as_array().unwrap();
            assert_eq!(tickets.len(), 1, "{query}: {result}");
            assert_eq!(tickets[0]["id"], id);
        }
    })
    .await
    .unwrap();
}
