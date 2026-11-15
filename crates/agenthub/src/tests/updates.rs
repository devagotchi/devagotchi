//! Changing a ticket: versions and transitions, listing, append-only notes and the event chain, workflow replacement, discovered work.

use super::*;

#[test]
fn update_needs_the_current_version_and_a_declared_transition() {
    let f = fixture();
    let jesse = Actor::human("jesse");
    let id = f.new_ticket(&jesse, "edit me");
    let ticket = f.board.get(&id).expect("get").ticket;
    let stale = f
        .board
        .update(
            &jesse,
            &id,
            ticket.version + 5,
            Patch {
                title: Some("renamed".into()),
                ..Patch::default()
            },
        )
        .expect_err("version mismatch");
    assert_eq!(stale.code, "version_conflict");
    let renamed = f
        .board
        .update(
            &jesse,
            &id,
            ticket.version,
            Patch {
                title: Some("renamed".into()),
                labels: Some(vec!["Hub".into(), "hub".into(), " rust ".into()]),
                ..Patch::default()
            },
        )
        .expect("update");
    assert_eq!(renamed.title, "renamed");
    assert_eq!(renamed.labels, vec!["hub", "rust"]);
    assert_eq!(renamed.version, ticket.version + 1);
    let illegal = f
        .board
        .update(
            &jesse,
            &id,
            renamed.version,
            Patch {
                state: Some("done".into()),
                ..Patch::default()
            },
        )
        .expect_err("ready -> done is not a transition");
    assert_eq!(illegal.code, "transition_not_allowed");
    let active = f
        .board
        .update(
            &jesse,
            &id,
            renamed.version,
            Patch {
                state: Some("in_progress".into()),
                ..Patch::default()
            },
        )
        .expect_err("active states are lease operations");
    assert_eq!(active.code, "use_lease_ops");
    let blocked = f
        .board
        .update(
            &jesse,
            &id,
            renamed.version,
            Patch {
                state: Some("blocked".into()),
                ..Patch::default()
            },
        )
        .expect("* -> blocked");
    assert_eq!(blocked.state, "blocked");
    let claude = Actor::agent("claude");
    let held_err = {
        let ready = f
            .board
            .update(
                &jesse,
                &id,
                blocked.version,
                Patch {
                    state: Some("ready".into()),
                    ..Patch::default()
                },
            )
            .expect("unblock");
        f.board.claim(&claude, &id, None).expect("claim");
        let held = f.board.get(&id).expect("get").ticket;
        f.board
            .update(
                &jesse,
                &id,
                held.version,
                Patch {
                    state: Some("cancelled".into()),
                    ..Patch::default()
                },
            )
            .expect_err("held tickets move only through lease ops");
        let _ = ready;
        f.board
            .update(
                &jesse,
                &id,
                held.version,
                Patch {
                    state: Some("cancelled".into()),
                    ..Patch::default()
                },
            )
            .expect_err("still held")
    };
    assert_eq!(held_err.code, "use_lease_ops");
}

#[test]
fn list_filters_by_label_text_state_and_open_only() {
    let f = fixture();
    let jesse = Actor::human("jesse");
    let a = f
        .board
        .create(
            &jesse,
            NewTicket {
                title: "Windows installer".into(),
                labels: vec!["windows".into()],
                ..NewTicket::default()
            },
        )
        .expect("a")
        .id;
    let b = f
        .board
        .create(
            &jesse,
            NewTicket {
                title: "Linux installer".into(),
                description: "one script for any server".into(),
                labels: vec!["linux".into()],
                ..NewTicket::default()
            },
        )
        .expect("b")
        .id;
    let by_label = f
        .board
        .list(&Filter {
            label: Some("linux".into()),
            ..Filter::default()
        })
        .expect("list");
    assert_eq!(by_label.iter().map(|t| &t.id).collect::<Vec<_>>(), vec![&b]);
    let by_text = f
        .board
        .list(&Filter {
            text: Some("any server".into()),
            ..Filter::default()
        })
        .expect("list");
    assert_eq!(by_text.iter().map(|t| &t.id).collect::<Vec<_>>(), vec![&b]);
    let like_safe = f
        .board
        .list(&Filter {
            text: Some("100%".into()),
            ..Filter::default()
        })
        .expect("a literal percent sign matches nothing here");
    assert!(like_safe.is_empty());
    let ticket = f.board.get(&a).expect("get").ticket;
    f.board
        .update(
            &jesse,
            &a,
            ticket.version,
            Patch {
                state: Some("cancelled".into()),
                ..Patch::default()
            },
        )
        .expect("cancel");
    let open = f
        .board
        .list(&Filter {
            open_only: true,
            ..Filter::default()
        })
        .expect("list");
    assert_eq!(open.iter().map(|t| &t.id).collect::<Vec<_>>(), vec![&b]);
    let cancelled = f
        .board
        .list(&Filter {
            state: Some("cancelled".into()),
            ..Filter::default()
        })
        .expect("list");
    assert_eq!(
        cancelled.iter().map(|t| &t.id).collect::<Vec<_>>(),
        vec![&a]
    );
}

#[test]
fn notes_are_append_only_and_the_chain_notices_tampering() {
    let f = fixture();
    let jesse = Actor::human("jesse");
    let id = f.new_ticket(&jesse, "audited");
    f.board
        .add_note(&jesse, &id, "note", "first", None, None)
        .expect("note");
    {
        let connection = f.board.local().expect("local board");
        let refused = connection.execute("DELETE FROM notes", []);
        assert!(refused.is_err(), "notes are append-only");
        let refused = connection.execute("UPDATE events SET actor = 'mallory' WHERE seq = 1", []);
        assert!(refused.is_err(), "events are append-only");
        // Only a raw edit outside the triggers could break the chain; simulate one
        // by dropping the trigger first, as an attacker with the file would.
        connection
            .execute_batch(
                "DROP TRIGGER events_no_update; UPDATE events SET actor = 'mallory' WHERE seq = 1;",
            )
            .expect("tamper");
    }
    let status = f.board.verify_chain().expect("verify");
    assert!(!status.ok);
    assert_eq!(status.first_broken, Some(1));
}

#[test]
fn workflow_replacement_is_validated_against_live_tickets() {
    let f = fixture();
    let jesse = Actor::human("jesse");
    let id = f.new_ticket(&jesse, "in ready");
    let mut workflow = Workflow::default_workflow();
    workflow.states.retain(|s| s.key != "ready");
    workflow.initial_state = "backlog".into();
    workflow.lanes[0].queue = "backlog".into();
    workflow.lanes[0].release_to = "backlog".into();
    workflow
        .transitions
        .retain(|t| t.from != "ready" && t.to != "ready");
    assert!(workflow.validate().is_ok());
    let refused = f
        .board
        .put_workflow(&jesse, &workflow)
        .expect_err("a ticket sits in ready");
    assert_eq!(refused.code, "invalid");
    let claude = Actor::agent("claude");
    let ticket = f.board.get(&id).expect("get").ticket;
    f.board
        .update(
            &jesse,
            &id,
            ticket.version,
            Patch {
                state: Some("backlog".into()),
                ..Patch::default()
            },
        )
        .expect("move it");
    f.board
        .put_workflow(&jesse, &workflow)
        .expect("now it fits");
    let lease = f
        .board
        .claim(&claude, &id, None)
        .expect("backlog is the lane queue now");
    assert_eq!(lease.ticket.state, "in_progress");
    assert_eq!(
        f.board.workflow().expect("workflow").kind("in_progress"),
        Some(StateKind::Active)
    );
}

#[test]
fn discovered_work_links_back_to_where_it_was_found() {
    let f = fixture();
    let claude = Actor::agent("claude");
    let jesse = Actor::human("jesse");
    let parent = f.new_ticket(&jesse, "port the store");
    let found = f
        .board
        .create(
            &claude,
            NewTicket {
                title: "rusqlite RETURNING needs 3.35".into(),
                discovered_from: Some(parent.clone()),
                ..NewTicket::default()
            },
        )
        .expect("create");
    let view = f.board.get(&found.id).expect("view");
    assert_eq!(view.deps.len(), 1);
    assert_eq!(view.deps[0].kind, DepKind::DiscoveredFrom);
    assert_eq!(view.deps[0].depends_on, parent);
    assert!(found.blocked_by.is_empty(), "discovered-from never blocks");
}
