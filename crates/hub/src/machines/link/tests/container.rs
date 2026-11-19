//! A machine in a container moves with its image (increment 209): the hub never sends it an
//! update, by request or in the hourly round, and says how to move it instead.

use super::*;

/// A live session for `device` whose hello said `build`, and whether it runs in a container;
/// the receiver gets every frame the hub sends it on its own.
fn connect_as(
    links: &Links,
    device: &str,
    build: &str,
    container: bool,
) -> mpsc::Receiver<HubFrame> {
    let (calls, _) = mpsc::channel(1);
    let (control, frames) = mpsc::channel(4);
    links.sessions.lock().unwrap().insert(
        device.to_string(),
        Session {
            id: 7,
            calls,
            pending: Arc::default(),
            build: Some(build.into()),
            container,
            control,
        },
    );
    frames
}

#[tokio::test]
async fn a_container_is_never_asked_to_update_and_is_told_how_to_move() {
    let dir = tempfile::tempdir().unwrap();
    let links = links!(dir.path());
    let links = links.offering(Some("main-bbbbbbb".into()));
    let mut docked = connect_as(&links, "docked", "main-aaaaaaa", true);
    let mut plain = connect_as(&links, "plain", "main-aaaaaaa", false);

    // Asked by a person: refused with the image to move to, and nothing is sent.
    let refused = links
        .request_update("docked", false, "jesse", "hub")
        .await
        .unwrap_err();
    assert_eq!(refused.code, "update_unsupported");
    assert!(
        refused
            .message
            .starts_with("container: docked runs in Docker"),
        "{}",
        refused.message
    );
    assert!(
        refused
            .message
            .contains("ghcr.io/devagotchi/devagotchi:sha-bbbbbbb")
            && refused.message.contains("docker compose up -d --build"),
        "{}",
        refused.message
    );
    assert!(docked.try_recv().is_err(), "no update frame reached it");
    assert!(
        links.update_of("docked").is_none(),
        "no update is under way"
    );
    // Even forced, and even on the hub's own build: a container is never asked.
    let on_it = connect_as(&links, "docked", "main-bbbbbbb", true);
    drop(on_it);
    assert_eq!(
        links
            .request_update("docked", true, "jesse", "hub")
            .await
            .unwrap_err()
            .code,
        "update_unsupported"
    );
    let mut docked = connect_as(&links, "docked", "main-aaaaaaa", true);

    // The hourly round asks the ordinary machine and passes the container by.
    links.auto_update_now().await;
    assert!(matches!(
        plain.try_recv(),
        Ok(HubFrame::Update { ref version, force: false }) if version == "main-bbbbbbb"
    ));
    assert!(
        docked.try_recv().is_err(),
        "the round passed the container by"
    );
    assert!(links.update_of("docked").is_none());
    assert!(links.update_of("plain").is_some());
}
