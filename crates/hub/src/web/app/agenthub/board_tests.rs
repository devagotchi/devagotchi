//! Source board behavior over actual PostgreSQL and authenticated HTTP.
use crate::{
    agenthub::{Actor, NewTicket},
    web::app::tests::{hosted, page},
};
use serde_json::json;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn workflow_board_exposes_leases_dependencies_and_completed_work_without_crossing_accounts() {
    let hub = or_skip!(hosted());
    let mut alice = hub.first_connect("Claude").await;
    let bob = hub.first_connect("Codex").await;
    let owner = hub.call(&mut alice, "whoami", json!({})).await["structuredContent"]["subject"]
        .as_str()
        .unwrap()
        .to_owned();
    let boards = hub.hub.boards.clone();
    let account = owner.clone();
    let board = tokio::task::spawn_blocking(move || {
        boards.bind(&account, "columns", &[]).unwrap();
        boards.open_account(&account, "columns").unwrap().unwrap()
    })
    .await
    .unwrap();
    let db = board.hosted();
    let actor = Actor::human(&owner);
    let held = db
        .create(
            &actor,
            &NewTicket {
                title: "Work <script>not markup</script>".into(),
                labels: vec!["release".into()],
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let completed = db
        .create(
            &actor,
            &NewTicket {
                title: "Completed source work".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let work = db.claim(&actor, &completed.id, None).await.unwrap();
    db.complete(&actor, &completed.id, work.fence, "Implemented")
        .await
        .unwrap();
    let reviewer = Actor::agent("reviewer");
    let review = db.claim(&reviewer, &completed.id, None).await.unwrap();
    db.complete(&reviewer, &completed.id, review.fence, "Reviewed")
        .await
        .unwrap();
    db.claim(&actor, &held.id, Some(60_000)).await.unwrap();
    let blocked = db
        .create(
            &actor,
            &NewTicket {
                title: "Waiting for dependency".into(),
                depends_on: vec![held.id.clone()],
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let mut config = db.configuration_state().await.unwrap();
    config
        .workflow
        .states
        .iter_mut()
        .find(|s| s.key == "ready")
        .unwrap()
        .label = "Ready <review>".into();
    db.edit_workflow(&actor, config.workflow_version, &config.workflow)
        .await
        .unwrap();
    let events = db.events().await.unwrap();

    let (status, html) = page(&hub, "/app/boards/columns", &alice).await;
    assert_eq!(status, 200, "{html}");
    for value in [
        "class=\"board-columns\"",
        "data-state=\"done\"",
        "Completed source work",
        "Ready &lt;review&gt;",
        "Work &lt;script&gt;not markup&lt;/script&gt;",
        "Held by: You",
        "Lease:",
        "Blocked by",
        "Held by me",
        "class=\"task-label\">release",
    ] {
        assert!(html.contains(value), "missing {value}: {html}");
    }
    assert!(!html.contains("<script>not markup</script>"));
    assert!(html.contains(&format!("/app/tickets/columns/{}", blocked.id)));
    let (_, mine) = page(&hub, "/app/boards/columns?mine=true&label=release", &alice).await;
    assert!(mine.contains(&format!("id=\"ticket-{}\"", held.id)));
    assert!(!mine.contains("Completed source work") && !mine.contains("Waiting for dependency"));
    assert!(mine.contains("name=\"mine\" value=\"true\""));
    assert!(mine.contains("label=release&amp;mine=true"));
    let (_, done) = page(&hub, "/app/boards/columns?state=done", &alice).await;
    assert!(done.contains("Completed source work"));
    assert!(!done.contains(&format!("id=\"ticket-{}\"", held.id)));
    assert!(done.contains("board-column is-selected\" data-state=\"done\""));
    assert!(done.contains("In progress <span class=\"chip-count\">1</span>"));
    assert!(mine.contains("Done <span class=\"chip-count\">0</span>"));
    let (status, foreign) = page(&hub, "/app/boards/columns", &bob).await;
    assert_eq!(status, 404);
    assert!(!foreign.contains("Completed source work"));
    assert_eq!(
        db.events().await.unwrap(),
        events,
        "reading must not mutate the board"
    );

    // A selected state has its own bounded read. Do not report zero when the general
    // count sample is filled by higher-priority work in another state.
    for index in 0..201 {
        db.create(
            &actor,
            &NewTicket {
                title: format!("Higher priority work {index}"),
                priority: Some(0),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    }
    let (_, done) = page(&hub, "/app/boards/columns?state=done", &alice).await;
    assert!(done.contains("Completed source work"));
    assert!(done.contains("Done <span class=\"chip-count\">1</span>"));
}
