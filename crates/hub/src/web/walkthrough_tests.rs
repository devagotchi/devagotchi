//! The setup walkthrough beside the hero's words (`landing/walkthrough.rs`): it works without
//! its script, a screen reader reads its steps, it draws this hub's own address, a new hub
//! shows none, and its script is the hub's own and reaches nothing.

use std::collections::BTreeSet;

use super::{
    landing::page,
    pages::{CSS, Front},
};

const URL: &str = "https://hub.example.org";

fn body(front: Front) -> String {
    page(&front, URL).1
}

#[test]
fn the_walkthrough_works_without_its_script_and_every_id_is_unique() {
    for front in [Front::Passkeys, Front::Nomen, Front::Hosted] {
        let html = body(front);
        let walk = html
            .split("<figure class=\"walkthrough\" data-walkthrough>")
            .nth(1)
            .and_then(|rest| rest.split("</figure>").next())
            .expect("the walkthrough in the hero");
        // A step is a radio button and a scene: choosing one shows its scene with no script.
        let steps = walk.matches("<label class=\"walk-tab\">").count();
        assert!(
            (2..=crate::site::walkthrough::MAX_STEPS).contains(&steps),
            "{steps}"
        );
        assert_eq!(walk.matches("<div class=\"walk-scene").count(), steps);
        assert_eq!(
            walk.matches("<input type=\"radio\" name=\"walk\"").count(),
            steps
        );
        assert_eq!(walk.matches(" checked>").count(), 1, "the first is chosen");
        assert!(walk.contains("data-seconds=\""));
        // The scenes are pictures; a screen reader reads each step's name and caption.
        assert!(walk.contains("<div class=\"walk-stage\" aria-hidden=\"true\">"));
        assert!(walk.contains("<legend class=\"visually-hidden\">"));
        // Pause shows only when the script runs, and says Play once pressed.
        assert!(walk.contains(
            "<button type=\"button\" class=\"walk-pause\" hidden data-pause=\"Pause\" data-play=\"Play\">"
        ));
        // The connector it draws is this hub's own address.
        assert!(walk.contains(&format!("{URL}/mcp")), "{walk}");
        // The diagram is on the page twice (the hero on a phone, How it works on a wide
        // screen), so its ids are prefixed: no id is used twice.
        let ids: Vec<&str> = html
            .split(" id=\"")
            .skip(1)
            .map(|rest| rest.split('"').next().unwrap())
            .collect();
        let unique: BTreeSet<&str> = ids.iter().copied().collect();
        assert_eq!(unique.len(), ids.len(), "{ids:?}");
        assert!(html.split("url(#").skip(1).all(|rest| {
            let id = rest.split(')').next().unwrap();
            unique.contains(id)
        }));
    }
    // A new hub offers only its claim.
    assert!(!body(Front::Unclaimed).contains("data-walkthrough"));
    // The stylesheet shows the chosen step of as many as a walkthrough may have.
    let last = crate::site::walkthrough::MAX_STEPS;
    assert!(CSS.contains(&format!(
        ".walk-tab:nth-of-type({last}) input:checked) .walk-scene:nth-child({last})"
    )));
    assert!(!CSS.contains(&format!("nth-of-type({})", last + 1)));
}

#[tokio::test]
async fn the_walkthrough_script_is_the_hubs_own() {
    use axum::http::header;
    let response = super::landing::script().await;
    let headers = response.headers().clone();
    assert_eq!(
        headers.get(header::CONTENT_TYPE).unwrap(),
        "text/javascript; charset=utf-8"
    );
    assert_eq!(
        headers.get(header::X_CONTENT_TYPE_OPTIONS).unwrap(),
        "nosniff"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let script = String::from_utf8(body.to_vec()).unwrap();
    // It finds the walkthrough, holds still for someone who asks for less motion, and
    // reaches nothing: no request, no storage, no other origin.
    assert!(script.contains("[data-walkthrough]"));
    assert!(script.contains("prefers-reduced-motion: reduce"));
    for never in [
        "fetch(",
        "XMLHttpRequest",
        "localStorage",
        "http",
        "innerHTML",
        "eval(",
    ] {
        assert!(!script.contains(never), "{never}");
    }
}
