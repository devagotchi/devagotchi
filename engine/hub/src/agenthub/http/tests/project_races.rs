//! Destructive lifecycle races must have one serializable outcome, not orphaned work.
use super::{
    principals::Admin,
    projects::{create, remove},
    *,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn project_removal_racing_task_creation_never_orphans_a_committed_task() {
    let admin = or_skip!(Admin::new());
    let first = create(&admin, "race").await;
    let (retired, task) = tokio::join!(
        remove(&admin, "race", 1, "race-remove"),
        response(
            admin
                .request(Method::POST, "/api/v1/tasks")
                .json(&json!({"project":"race","title":"Concurrent work"}))
        )
    );
    if retired.0 == StatusCode::OK {
        assert_eq!(task.0, StatusCode::NOT_FOUND, "{:?}", task.1);
        let (_, history, _) = response(admin.request(
            Method::GET,
            &format!(
                "/api/v1/projects/archive/{}/events",
                first["board_id"].as_str().unwrap()
            ),
        ))
        .await;
        assert_eq!(history["events"].as_array().unwrap().len(), 2);
    } else {
        assert_eq!(retired.0, StatusCode::CONFLICT, "{:?}", retired.1);
        assert_eq!(task.0, StatusCode::OK, "{:?}", task.1);
        let (status, found, _) = response(admin.request(
            Method::GET,
            &format!(
                "/api/v1/tasks/{}?project=race",
                task.1["id"].as_str().unwrap()
            ),
        ))
        .await;
        assert_eq!(status, StatusCode::OK, "{found}");
        assert_eq!(
            remove(&admin, "race", 1, "later-remove").await.0,
            StatusCode::CONFLICT
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn project_removal_racing_creation_keeps_generations_and_receipts_distinct() {
    let admin = or_skip!(Admin::new());
    let first = create(&admin, "race").await;
    let input = json!({"id":"race","name":"Replacement","version":0});
    let request = || {
        admin
            .request(Method::POST, "/api/v1/projects")
            .header("idempotency-key", "replacement")
            .json(&input)
    };
    let (retired, created) = tokio::join!(
        remove(&admin, "race", 1, "retire-generation"),
        response(request())
    );
    assert_eq!(retired.0, StatusCode::OK, "{:?}", retired.1);
    assert!(
        [StatusCode::OK, StatusCode::CONFLICT].contains(&created.0),
        "{:?}",
        created.1
    );
    let (status, replacement, _) = response(request()).await;
    assert_eq!(status, StatusCode::OK, "{replacement}");
    assert_ne!(replacement["result"]["board_id"], first["board_id"]);
    assert_eq!(
        remove(&admin, "race", 1, "retire-generation").await.0,
        StatusCode::CONFLICT
    );
    let (status, current, _) = response(admin.request(Method::GET, "/api/v1/projects/race")).await;
    assert_eq!(status, StatusCode::OK, "{current}");
    assert_eq!(current, replacement["result"]);
}
