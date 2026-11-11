//! Approving ahead of time (`approval.rs`, standing authorizations): only a person grants
//! or ends one, it covers its kind (and target) until it runs out, an approval it covers is
//! approved the moment it is asked, and kinds a person keeps approving are offered.

use super::{
    Actor, Board,
    approval::{Ask, SUGGEST_AFTER, Verdict},
};

const DAY: i64 = 86_400_000;

fn ask<'a>(kind: Option<&'a str>, target: Option<&'a str>) -> Ask<'a> {
    Ask {
        title: "Restart the api",
        what: "systemctl restart api",
        why: "a new build",
        undo: "start the old build",
        kind,
        target,
    }
}

/// The whole of it on one board; `advance` moves the board's clock.
pub(crate) fn standing_approvals_hold(board: &Board, advance: Option<&dyn Fn(i64)>) {
    let agent = Actor::agent("claude");
    let person = Actor::human("jesse");

    // Nothing granted: it waits for a person, and its kind and target show.
    let (asked, standing) = board
        .ask(&agent, &ask(Some("restart"), Some("dabo")))
        .unwrap();
    assert_eq!(asked.status, "pending");
    assert!(standing.is_none());
    assert_eq!(asked.kind.as_deref(), Some("restart"));
    assert_eq!(asked.target.as_deref(), Some("dabo"));

    // An agent never grants, never ends one; bad names and lengths are refused.
    assert_eq!(
        board
            .grant_standing(&agent, "restart", Some("dabo"), 30)
            .unwrap_err()
            .code,
        "humans_only"
    );
    assert_eq!(
        board
            .grant_standing(&person, "Restart!", None, 30)
            .unwrap_err()
            .code,
        "invalid"
    );
    for days in [0, 91] {
        assert_eq!(
            board
                .grant_standing(&person, "restart", None, days)
                .unwrap_err()
                .code,
            "invalid"
        );
    }
    assert_eq!(
        board
            .ask(&agent, &ask(None, Some("dabo")))
            .unwrap_err()
            .code,
        "invalid",
        "a target goes with a kind"
    );

    // Granted for dabo: approved at once there, by the standing authorization; elsewhere and
    // for another kind, still a person's.
    let granted = board
        .grant_standing(&person, "restart", Some("dabo"), 30)
        .unwrap();
    assert!(granted.id.starts_with("st-"));
    let (at_once, standing) = board
        .ask(&agent, &ask(Some("restart"), Some("dabo")))
        .unwrap();
    assert_eq!(at_once.status, "approved");
    assert_eq!(
        standing.as_ref().map(|s| s.id.as_str()),
        Some(granted.id.as_str())
    );
    assert_eq!(
        at_once.decided_by.as_deref(),
        Some(format!("standing:{}", granted.id).as_str())
    );
    assert!(at_once.note.as_deref().unwrap().contains("jesse"));
    for (kind, target) in [("restart", Some("platinum")), ("deploy", Some("dabo"))] {
        let (other, standing) = board.ask(&agent, &ask(Some(kind), target)).unwrap();
        assert_eq!(
            (other.status.as_str(), standing),
            ("pending", None),
            "{kind}"
        );
    }
    assert!(
        board
            .pending_approvals()
            .unwrap()
            .iter()
            .all(|a| a.id != at_once.id),
        "approved ahead of time, it never waits"
    );

    // Ended: it needs a person again. Ending it twice is refused.
    assert_eq!(
        board.revoke_standing(&agent, &granted.id).unwrap_err().code,
        "humans_only"
    );
    let ended = board.revoke_standing(&person, &granted.id).unwrap();
    assert_eq!(ended.revoked_by.as_deref(), Some("jesse"));
    assert_eq!(
        board
            .revoke_standing(&person, &granted.id)
            .unwrap_err()
            .code,
        "not_found"
    );
    let (again, _) = board
        .ask(&agent, &ask(Some("restart"), Some("dabo")))
        .unwrap();
    assert_eq!(again.status, "pending");

    // Any target, then run out: after its days, it needs a person again (where the
    // board's clock can be moved: a file board's; a hosted board reads Postgres's).
    let any = board.grant_standing(&person, "restart", None, 1).unwrap();
    let (anywhere, _) = board
        .ask(&agent, &ask(Some("restart"), Some("platinum")))
        .unwrap();
    assert_eq!(anywhere.status, "approved");
    match advance {
        Some(advance) => {
            advance(DAY + 1);
            let (lapsed, _) = board
                .ask(&agent, &ask(Some("restart"), Some("platinum")))
                .unwrap();
            assert_eq!(lapsed.status, "pending");
            assert!(
                board
                    .standing()
                    .unwrap()
                    .iter()
                    .all(|s| !s.live(board.now()))
            );
        }
        None => {
            board.revoke_standing(&person, &any.id).unwrap();
        }
    }

    // Kept approving `deploy` on dabo and never denied it: offered. One denial of `rotate`:
    // never offered.
    for _ in 0..SUGGEST_AFTER {
        let (a, _) = board
            .ask(&agent, &ask(Some("deploy"), Some("dabo")))
            .unwrap();
        board
            .decide_approval(&person, &a.id, a.version, Verdict::Approved, "")
            .unwrap();
        let (r, _) = board.ask(&agent, &ask(Some("rotate"), None)).unwrap();
        board
            .decide_approval(&person, &r.id, r.version, Verdict::Approved, "")
            .unwrap();
    }
    let (r, _) = board.ask(&agent, &ask(Some("rotate"), None)).unwrap();
    board
        .decide_approval(&person, &r.id, r.version, Verdict::Denied, "not now")
        .unwrap();
    let offered = board.suggestions().unwrap();
    assert_eq!(offered.len(), 1, "{offered:?}");
    assert_eq!(offered[0].kind, "deploy");
    assert_eq!(offered[0].target.as_deref(), Some("dabo"));
    assert!(offered[0].approved >= SUGGEST_AFTER);
    // Taken, it is no longer offered.
    board
        .grant_standing(&person, "deploy", Some("dabo"), 30)
        .unwrap();
    assert!(board.suggestions().unwrap().is_empty());

    // Every grant and end is on the chain.
    let actions: Vec<String> = board
        .events(0, 10_000)
        .unwrap()
        .into_iter()
        .map(|e| e.action)
        .collect();
    assert!(actions.contains(&"approval.standing_granted".to_string()));
    assert!(actions.contains(&"approval.standing_revoked".to_string()));
    assert!(board.verify_chain().unwrap().ok, "the chain still verifies");
}
