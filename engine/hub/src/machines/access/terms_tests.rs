use super::*;
use devagotchi_contracts::GrantDuration;
use serde_json::json;

fn terms() -> GrantTerms {
    serde_json::from_value(
        json!({"item":{"capability":"exec","sandbox_level":"workspace-write",
        "selector":{"paths":["/work"],"argv_prefix":[["git"]]}},"duration":"8h","uses_max":4}),
    )
    .unwrap()
}
#[test]
fn access_terms_decisions_can_only_reduce_the_requested_authority() {
    let requested = terms();
    assert_eq!(requested.narrowed(&Narrow::default()).unwrap(), requested);
    let narrower = Narrow {
        duration: Some(GrantDuration::Hours(1)),
        uses_max: Some(2),
        ..Default::default()
    };
    assert_eq!(requested.narrowed(&narrower).unwrap().uses_max, Some(2));
    for decision in [
        Narrow {
            duration: Some(GrantDuration::UntilRevoked),
            ..Default::default()
        },
        Narrow {
            duration: Some(GrantDuration::Task),
            ..Default::default()
        },
        Narrow {
            uses_max: Some(5),
            ..Default::default()
        },
        Narrow {
            uses_max: Some(0),
            duration: Some(GrantDuration::Once),
            ..Default::default()
        },
    ] {
        assert!(requested.narrowed(&decision).is_err());
    }
    let mut wider = requested.item.clone();
    wider.selector = Default::default();
    assert!(
        requested
            .narrowed(&Narrow {
                item: Some(wider),
                ..Default::default()
            })
            .is_err()
    );
    let once = requested
        .narrowed(&Narrow {
            duration: Some(GrantDuration::Once),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(once.uses_max, Some(1));
    assert!(
        once.narrowed(&Narrow {
            uses_max: Some(2),
            ..Default::default()
        })
        .is_err()
    );
}
#[test]
fn access_terms_expiry_is_bounded_and_once_is_an_admission_limit() {
    let requested = terms();
    assert_eq!(requested.expires_at(10).unwrap(), Some(28_800_010));
    assert!(requested.expires_at(i64::MAX).is_err());
    let once = requested
        .narrowed(&Narrow {
            duration: Some(GrantDuration::Once),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(once.expires_at(10).unwrap(), None);
    let mut invalid = once;
    invalid.uses_max = None;
    assert!(invalid.validate().is_err());
}
