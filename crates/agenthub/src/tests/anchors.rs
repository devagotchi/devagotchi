//! Anchors (machine, repo, environment, agent, session) and a board's repo and environments.

use super::*;

#[test]
fn anchors_round_trip_through_create_update_and_list() {
    let f = fixture();
    let jesse = Actor::human("jesse");
    let anchors = Anchors {
        machine: Some("dabo".into()),
        repo: Some("https://github.com/x/y".into()),
        environment: Some("prod".into()),
        agent: Some("claude".into()),
        session: Some("s-1".into()),
        fingerprint: Some("fp-1".into()),
    };
    let created = f
        .board
        .create(
            &jesse,
            NewTicket {
                title: "anchored".into(),
                anchors: anchors.clone(),
                occurrences: Some(3),
                last_seen_at: Some(42),
                ..NewTicket::default()
            },
        )
        .expect("create");
    assert_eq!(created.machine.as_deref(), Some("dabo"));
    assert_eq!(created.repo.as_deref(), Some("https://github.com/x/y"));
    assert_eq!(created.environment.as_deref(), Some("prod"));
    assert_eq!(created.agent.as_deref(), Some("claude"));
    assert_eq!(created.session.as_deref(), Some("s-1"));
    assert_eq!(created.fingerprint.as_deref(), Some("fp-1"));
    assert_eq!((created.occurrences, created.last_seen_at), (3, Some(42)));
    assert_eq!(f.board.get(&created.id).unwrap().ticket, created);
    let plain = f.new_ticket(&jesse, "plain");
    assert_eq!(f.board.get(&plain).unwrap().ticket.occurrences, 1);

    let updated = f
        .board
        .update(
            &jesse,
            &created.id,
            created.version,
            Patch {
                anchors: AnchorPatch {
                    machine: Some(Some("wisconsin".into())),
                    environment: Some(Some("dev".into())),
                    session: Some(None),
                    ..AnchorPatch::default()
                },
                occurrences: Some(4),
                last_seen_at: Some(Some(99)),
                ..Patch::default()
            },
        )
        .expect("update");
    assert_eq!(updated.machine.as_deref(), Some("wisconsin"));
    assert_eq!(updated.environment.as_deref(), Some("dev"));
    assert_eq!(updated.session, None, "Some(None) clears");
    assert_eq!(updated.agent.as_deref(), Some("claude"), "untouched stays");
    assert_eq!((updated.occurrences, updated.last_seen_at), (4, Some(99)));
    assert_eq!(updated.version, created.version + 1);

    let by = |anchors: Anchors| -> Vec<String> {
        f.board
            .list(&Filter {
                anchors,
                ..Filter::default()
            })
            .expect("list")
            .into_iter()
            .map(|t| t.id)
            .collect()
    };
    let only = vec![created.id.clone()];
    assert_eq!(
        by(Anchors {
            machine: Some("wisconsin".into()),
            ..Anchors::default()
        }),
        only
    );
    assert_eq!(
        by(Anchors {
            repo: Some("https://github.com/x/y".into()),
            ..Anchors::default()
        }),
        only
    );
    assert_eq!(
        by(Anchors {
            environment: Some("dev".into()),
            ..Anchors::default()
        }),
        only
    );
    assert_eq!(
        by(Anchors {
            agent: Some("claude".into()),
            ..Anchors::default()
        }),
        only
    );
    assert_eq!(
        by(Anchors {
            fingerprint: Some("fp-1".into()),
            ..Anchors::default()
        }),
        only
    );
    assert!(
        by(Anchors {
            machine: Some("dabo".into()),
            ..Anchors::default()
        })
        .is_empty()
    );
    assert!(
        by(Anchors {
            session: Some("s-1".into()),
            ..Anchors::default()
        })
        .is_empty()
    );
    assert!(
        by(Anchors {
            machine: Some("wisconsin".into()),
            environment: Some("prod".into()),
            ..Anchors::default()
        })
        .is_empty(),
        "filters combine"
    );
    assert_eq!(by(Anchors::default()).len(), 2);
}

#[test]
fn an_environment_outside_the_boards_list_is_refused_with_the_list() {
    let f = fixture();
    let jesse = Actor::human("jesse");
    let refused = f
        .board
        .create(
            &jesse,
            NewTicket {
                title: "qa".into(),
                anchors: Anchors {
                    environment: Some("qa".into()),
                    ..Anchors::default()
                },
                ..NewTicket::default()
            },
        )
        .unwrap_err();
    assert_eq!(refused.code, "unknown_environment");
    assert!(
        refused.message.contains("dev, staging, prod"),
        "{}",
        refused.message
    );

    let mut settings = f.board.settings().unwrap();
    assert_eq!(settings.environments, ["dev", "staging", "prod"]);
    settings.environments.push("qa".into());
    settings.repo = Some("https://github.com/x/y".into());
    f.board.put_settings(&jesse, &settings).unwrap();
    assert_eq!(f.board.settings().unwrap(), settings);
    let ticket = f
        .board
        .create(
            &jesse,
            NewTicket {
                title: "qa".into(),
                anchors: Anchors {
                    environment: Some("qa".into()),
                    ..Anchors::default()
                },
                ..NewTicket::default()
            },
        )
        .expect("qa is on the list now");
    let refused = f
        .board
        .update(
            &jesse,
            &ticket.id,
            ticket.version,
            Patch {
                anchors: AnchorPatch {
                    environment: Some(Some("moon".into())),
                    ..AnchorPatch::default()
                },
                ..Patch::default()
            },
        )
        .unwrap_err();
    assert_eq!(refused.code, "unknown_environment");
    assert!(refused.message.contains("qa"), "{}", refused.message);
}

#[test]
fn the_anchor_line_names_what_is_known_in_order() {
    let f = fixture();
    let jesse = Actor::human("jesse");
    let line = |anchors: Anchors| {
        f.board
            .create(
                &jesse,
                NewTicket {
                    title: "t".into(),
                    anchors,
                    ..NewTicket::default()
                },
            )
            .unwrap()
            .anchor_line()
    };
    assert_eq!(line(Anchors::default()), None);
    assert_eq!(
        line(Anchors {
            machine: Some("dabo".into()),
            environment: Some("prod".into()),
            repo: Some("https://github.com/x/y.git".into()),
            agent: Some("claude".into()),
            session: Some("abc".into()),
            fingerprint: Some("never shown".into()),
        })
        .as_deref(),
        Some("dabo · prod · github.com/x/y · claude (session abc)")
    );
    assert_eq!(
        line(Anchors {
            session: Some("0123456789".into()),
            ..Anchors::default()
        })
        .as_deref(),
        Some("session 01234567…")
    );
}

#[test]
fn a_boards_repo_and_environments_are_set_checked_and_on_the_chain() {
    let f = fixture();
    let jesse = Actor::human("jesse");
    let before = f.board.events(0, 1000).unwrap().len();
    let read = f
        .board
        .set_project(&jesse, &crate::ProjectSettings::default())
        .expect("nothing given reads");
    assert_eq!(read, Settings::default());
    assert_eq!(
        f.board.events(0, 1000).unwrap().len(),
        before,
        "a read writes nothing"
    );

    let set = f
        .board
        .set_project(
            &jesse,
            &crate::ProjectSettings {
                repo: Some(" https://github.com/x/y ".into()),
                environments: Some(vec!["dev".into(), "prod".into()]),
            },
        )
        .expect("set");
    assert_eq!(set.repo.as_deref(), Some("https://github.com/x/y"));
    assert_eq!(set.environments, ["dev", "prod"]);
    assert_eq!(f.board.settings().unwrap(), set, "read back");
    assert_eq!(
        set.lease_ttl_ms,
        Settings::default().lease_ttl_ms,
        "the rest stays"
    );
    let changed = f.board.events(0, 1000).unwrap().pop().unwrap();
    assert_eq!(changed.action, "board.settings_changed");
    assert_eq!(changed.actor, "jesse");
    assert_eq!(
        changed.data,
        serde_json::json!({ "repo": "https://github.com/x/y", "environments": ["dev", "prod"] })
    );
    // Only what is given changes.
    let set = f
        .board
        .set_project(
            &jesse,
            &crate::ProjectSettings {
                environments: Some(vec!["staging".into()]),
                ..Default::default()
            },
        )
        .expect("environments only");
    assert_eq!(set.repo.as_deref(), Some("https://github.com/x/y"));
    assert_eq!(set.environments, ["staging"]);
    assert!(f.board.verify_chain().unwrap().ok);

    let refused =
        |patch: crate::ProjectSettings| f.board.set_project(&jesse, &patch).expect_err("refused");
    for repo in [
        "http://github.com/x/y",
        "github.com/x/y",
        "https://",
        "https://exa mple.com/x",
        "ftp://x.y/z",
    ] {
        let error = refused(crate::ProjectSettings {
            repo: Some(repo.into()),
            ..Default::default()
        });
        assert_eq!(error.code, "invalid_settings", "{repo}");
        assert!(
            error.message.contains("is not an https URL"),
            "{repo}: {}",
            error.message
        );
    }
    let many: Vec<String> = (0..21).map(|n| format!("e{n}")).collect();
    for (environments, says) in [
        (vec![], "1 to 20 names"),
        (many, "1 to 20 names"),
        (vec!["Prod".to_string()], "not a lowercase name"),
        (vec!["dev".to_string(), "dev".to_string()], "named twice"),
        (vec!["has space".to_string()], "not a lowercase name"),
    ] {
        let error = refused(crate::ProjectSettings {
            environments: Some(environments.clone()),
            ..Default::default()
        });
        assert_eq!(error.code, "invalid_settings", "{environments:?}");
        assert!(
            error.message.contains(says),
            "{environments:?}: {}",
            error.message
        );
    }
    let after = f.board.settings().unwrap();
    assert_eq!(after.environments, ["staging"], "a refusal changes nothing");
}
