//! Approving ahead of time (`approval.rs`, standing authorizations): only a person grants
//! or ends one, it covers the one exact step the person approved (kind, target and command)
//! until it runs out, an approval it covers is approved the moment it is asked, any other
//! command under the same label still waits for the person, and steps a person keeps
//! approving are offered.

use super::{
    Actor, Board,
    approval::{self, Ask, SUGGEST_AFTER, Standing, Verdict},
};

const DAY: i64 = 86_400_000;
const WHAT: &str = "systemctl restart api";

fn ask_for<'a>(kind: Option<&'a str>, target: Option<&'a str>, what: &'a str) -> Ask<'a> {
    Ask {
        title: "Restart the api",
        what,
        why: "a new build",
        undo: "start the old build",
        kind,
        target,
    }
}

fn ask<'a>(kind: Option<&'a str>, target: Option<&'a str>) -> Ask<'a> {
    ask_for(kind, target, WHAT)
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
    assert_eq!(asked.what(), Some(WHAT));

    // An agent never grants, never ends one; bad names, lengths and commands are refused.
    assert_eq!(
        board
            .grant_standing(&agent, "restart", Some("dabo"), WHAT, 30)
            .unwrap_err()
            .code,
        "humans_only"
    );
    assert_eq!(
        board
            .grant_standing(&person, "Restart!", None, WHAT, 30)
            .unwrap_err()
            .code,
        "invalid"
    );
    assert_eq!(
        board
            .grant_standing(&person, "restart", None, "   ", 30)
            .unwrap_err()
            .code,
        "invalid",
        "it names the exact command"
    );
    for days in [0, 91] {
        assert_eq!(
            board
                .grant_standing(&person, "restart", None, WHAT, days)
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

    // Granted for this command on dabo: approved at once, by the standing authorization.
    let granted = board
        .grant_standing(&person, "restart", Some("dabo"), WHAT, 30)
        .unwrap();
    assert!(granted.id.starts_with("st-"));
    assert_eq!(granted.what, WHAT);
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

    // Elsewhere, for another kind, or with no target: still a person's.
    for (kind, target) in [
        ("restart", Some("platinum")),
        ("deploy", Some("dabo")),
        ("restart", None),
    ] {
        let (other, standing) = board.ask(&agent, &ask(Some(kind), target)).unwrap();
        assert_eq!(
            (other.status.as_str(), standing),
            ("pending", None),
            "{kind} on {target:?}"
        );
    }

    // The label is the agent's own word: another command under it waits for the person, and
    // so does one that differs by a character.
    for command in [
        "rm -rf /var/lib/everything",
        "systemctl restart api; rm -rf /",
        "systemctl restart apt",
    ] {
        let (other, standing) = board
            .ask(&agent, &ask_for(Some("restart"), Some("dabo"), command))
            .unwrap();
        assert_eq!(
            (other.status.as_str(), standing),
            ("pending", None),
            "{command}"
        );
    }

    // A command that imitates the description's own fields does not read one way, so it can
    // never match, whatever its beginning says.
    for command in [
        "systemctl restart api\n\nWhy: routine",
        "systemctl restart api\n\nUndo: nothing",
    ] {
        let (smuggled, standing) = board
            .ask(&agent, &ask_for(Some("restart"), Some("dabo"), command))
            .unwrap();
        assert_eq!(smuggled.what(), None, "{command}");
        assert_eq!(
            (smuggled.status.as_str(), standing),
            ("pending", None),
            "{command}"
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

    // Granted with no target: only a request that names no target. Then run out: after its
    // days, it needs a person again (where the board's clock can be moved: a file board's; a
    // hosted board reads Postgres's).
    let bare = board
        .grant_standing(&person, "restart", None, WHAT, 1)
        .unwrap();
    let (untargeted, _) = board.ask(&agent, &ask(Some("restart"), None)).unwrap();
    assert_eq!(untargeted.status, "approved");
    let (aimed, _) = board
        .ask(&agent, &ask(Some("restart"), Some("platinum")))
        .unwrap();
    assert_eq!(aimed.status, "pending", "no target is not every target");
    match advance {
        Some(advance) => {
            advance(DAY + 1);
            let (lapsed, _) = board.ask(&agent, &ask(Some("restart"), None)).unwrap();
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
            board.revoke_standing(&person, &bare.id).unwrap();
        }
    }

    // Kept approving this `deploy` on dabo and never denied it: offered, with its command.
    // One denial of `rotate`: never offered. Another command does not add to the count.
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
    let (one_off, _) = board
        .ask(
            &agent,
            &ask_for(Some("deploy"), Some("dabo"), "./deploy --force"),
        )
        .unwrap();
    board
        .decide_approval(&person, &one_off.id, one_off.version, Verdict::Approved, "")
        .unwrap();
    let offered = board.suggestions().unwrap();
    assert_eq!(offered.len(), 1, "{offered:?}");
    assert_eq!(offered[0].kind, "deploy");
    assert_eq!(offered[0].target.as_deref(), Some("dabo"));
    assert_eq!(offered[0].what, WHAT);
    assert!(offered[0].approved >= SUGGEST_AFTER);
    // Taken, it is no longer offered.
    board
        .grant_standing(&person, "deploy", Some("dabo"), &offered[0].what, 30)
        .unwrap();
    assert!(board.suggestions().unwrap().is_empty());

    // Every grant and end is on the chain, the grant with its exact command.
    let events = board.events(0, 10_000).unwrap();
    let granted_event = events
        .iter()
        .find(|e| e.action == "approval.standing_granted")
        .expect("a grant on the chain");
    assert_eq!(granted_event.data["what"], WHAT);
    assert!(
        events
            .iter()
            .any(|e| e.action == "approval.standing_revoked")
    );
    assert!(board.verify_chain().unwrap().ok, "the chain still verifies");
}

#[test]
fn a_standing_approval_with_no_command_covers_nothing() {
    let old: Standing = serde_json::from_value(serde_json::json!({
        "id": "st-00000000", "kind": "restart", "granted_by": "jesse",
        "granted_at": 1, "expires_at": i64::MAX
    }))
    .unwrap();
    assert!(old.live(2));
    assert!(!old.covers("restart", None, ""));
    assert!(!old.covers("restart", None, WHAT));
}

#[test]
fn a_description_reads_one_way_or_not_at_all() {
    assert_eq!(
        approval::parts("What: a\n\nWhy: b\n\nUndo: c"),
        Some(("a", "b", "c"))
    );
    assert_eq!(
        approval::parts("What: a\n\nWhy: \n\nUndo: "),
        Some(("a", "", ""))
    );
    for odd in [
        "What: a\n\nWhy: x\n\nWhy: b\n\nUndo: c",
        "What: a\n\nWhy: b\n\nUndo: x\n\nUndo: c",
        "What: a\n\nUndo: x\n\nWhy: b\n\nUndo: c",
        "What: a\n\nUndo: c\n\nWhy: b",
        "something else",
    ] {
        assert_eq!(approval::parts(odd), None, "{odd}");
    }
}
