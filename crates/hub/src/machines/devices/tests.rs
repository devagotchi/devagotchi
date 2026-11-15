use super::*;

#[tokio::test]
async fn a_code_binds_one_key_once_and_only_for_its_device() {
    let devices = or_skip!(test_registry());
    let code = devices.issue_code("gus", [7; 16]).await.unwrap();
    assert!(code.starts_with("dvg-") && code.len() == 36);
    assert_eq!(
        devices
            .enroll("dabo", &code, "aa", true)
            .await
            .unwrap_err()
            .code,
        "enroll_refused"
    );
    devices.enroll("gus", &code, "aa", true).await.unwrap();
    assert_eq!(devices.get("gus").await.unwrap().unwrap().public_key, "aa");
    assert_eq!(
        devices
            .enroll("gus", &code, "bb", true)
            .await
            .unwrap_err()
            .code,
        "enroll_refused"
    );
    assert_eq!(devices.get("gus").await.unwrap().unwrap().public_key, "aa");
}

#[tokio::test]
async fn only_the_newest_code_lives_and_binding_or_revoking_ends_it() {
    let devices = or_skip!(test_registry());
    let older = devices.issue_code("gus", [10; 16]).await.unwrap();
    let newer = devices.issue_code("gus", [11; 16]).await.unwrap();
    assert!(
        devices.enroll("gus", &older, "aa", true).await.is_err(),
        "a superseded code must not work"
    );
    let spare = devices.issue_code("dabo", [12; 16]).await.unwrap();
    devices.enroll("gus", &newer, "aa", true).await.unwrap();
    // A code for another device is untouched by gus's binding.
    let pending = devices.issue_code("gus", [13; 16]).await.unwrap();
    assert_eq!(devices.revoke("gus").await.unwrap(), (true, 1));
    assert!(
        devices.enroll("gus", &pending, "bb", true).await.is_err(),
        "revoke must cancel pending codes"
    );
    assert_eq!(devices.get("gus").await.unwrap().unwrap().public_key, "aa");
    devices.enroll("dabo", &spare, "cc", true).await.unwrap();
    // Revoking a name that never enrolled still cancels its codes.
    let orphan = devices.issue_code("solo", [14; 16]).await.unwrap();
    assert_eq!(devices.revoke("solo").await.unwrap(), (false, 1));
    assert!(devices.enroll("solo", &orphan, "dd", true).await.is_err());
}

#[tokio::test]
async fn expired_and_unknown_codes_are_refused() {
    let devices = or_skip!(test_registry());
    let code = devices.issue_code("gus", [1; 16]).await.unwrap();
    devices.expire_codes().await;
    assert!(devices.enroll("gus", &code, "aa", true).await.is_err());
    assert!(devices.enroll("gus", "dvg-nope", "aa", true).await.is_err());
    assert!(devices.get("gus").await.unwrap().is_none());
}

#[tokio::test]
async fn revoke_sticks_until_a_new_code_re_enrolls() {
    let devices = or_skip!(test_registry());
    let first = devices.issue_code("gus", [2; 16]).await.unwrap();
    devices.enroll("gus", &first, "aa", true).await.unwrap();
    assert!(!devices.is_revoked("gus").await.unwrap());
    assert_eq!(devices.revoke("gus").await.unwrap(), (true, 0));
    assert!(devices.is_revoked("gus").await.unwrap());
    assert_eq!(devices.revoke("gus").await.unwrap(), (false, 0));
    let second = devices.issue_code("gus", [3; 16]).await.unwrap();
    devices.enroll("gus", &second, "cc", true).await.unwrap();
    assert!(!devices.is_revoked("gus").await.unwrap());
    assert_eq!(devices.list().await.unwrap().len(), 1);
    assert!(devices.is_revoked("never-enrolled").await.unwrap());
}

#[tokio::test]
async fn an_owned_code_binds_a_machine_to_its_account_only() {
    let devices = or_skip!(test_registry());
    let code = devices
        .issue_owned_code("acct-a", "platinum", [20; 16], OWNED_LIMIT)
        .await
        .unwrap();
    // Not in the hub config, yet it joins: the code says whose it is.
    let device = devices
        .enroll("platinum", &code, "aa", false)
        .await
        .unwrap();
    assert_eq!(device.owner.as_deref(), Some("acct-a"));
    assert_eq!(
        devices
            .get("platinum")
            .await
            .unwrap()
            .unwrap()
            .owner
            .as_deref(),
        Some("acct-a")
    );
    assert_eq!(devices.owned_by("acct-a").await.unwrap().len(), 1);
    assert!(devices.owned_by("acct-b").await.unwrap().is_empty());
    // Another account cannot take the name, nor revoke it, nor learn more than "not yours".
    assert_eq!(
        devices
            .issue_owned_code("acct-b", "platinum", [21; 16], OWNED_LIMIT)
            .await
            .unwrap_err()
            .code,
        "name_taken"
    );
    assert_eq!(
        devices
            .revoke_owned("acct-b", "platinum")
            .await
            .unwrap_err()
            .code,
        "not_found"
    );
    // The owner re-adds it with a new key after revoking it.
    assert_eq!(
        devices.revoke_owned("acct-a", "platinum").await.unwrap(),
        (true, 0)
    );
    assert!(devices.is_revoked("platinum").await.unwrap());
    let again = devices
        .issue_owned_code("acct-a", "platinum", [22; 16], OWNED_LIMIT)
        .await
        .unwrap();
    devices
        .enroll("platinum", &again, "bb", false)
        .await
        .unwrap();
    let record = devices.get("platinum").await.unwrap().unwrap();
    assert_eq!(
        (record.public_key.as_str(), record.revoked_at),
        ("bb", None)
    );
}

#[tokio::test]
async fn operator_codes_and_configured_names_stay_the_operators() {
    let devices = or_skip!(test_registry());
    // An operator's code joins only a device the hub config names.
    let code = devices.issue_code("gus", [30; 16]).await.unwrap();
    assert_eq!(
        devices
            .enroll("gus", &code, "aa", false)
            .await
            .unwrap_err()
            .code,
        "enroll_refused"
    );
    let code = devices.issue_code("gus", [31; 16]).await.unwrap();
    assert_eq!(
        devices
            .enroll("gus", &code, "aa", true)
            .await
            .unwrap()
            .owner,
        None
    );
    // An account cannot claim an operator's machine by name.
    assert_eq!(
        devices
            .issue_owned_code("acct-a", "gus", [32; 16], OWNED_LIMIT)
            .await
            .unwrap_err()
            .code,
        "name_taken"
    );
    assert_eq!(
        devices
            .revoke_owned("acct-a", "gus")
            .await
            .unwrap_err()
            .code,
        "not_found"
    );
}

#[tokio::test]
async fn a_pending_owned_code_holds_the_name_until_it_is_spent_or_expires() {
    let devices = or_skip!(test_registry());
    let pending = devices
        .issue_owned_code("acct-a", "spare", [40; 16], OWNED_LIMIT)
        .await
        .unwrap();
    assert_eq!(
        devices
            .issue_owned_code("acct-b", "spare", [41; 16], OWNED_LIMIT)
            .await
            .unwrap_err()
            .code,
        "name_taken"
    );
    // The owner's new code supersedes its own unused one.
    let newer = devices
        .issue_owned_code("acct-a", "spare", [42; 16], OWNED_LIMIT)
        .await
        .unwrap();
    assert!(
        devices
            .enroll("spare", &pending, "aa", false)
            .await
            .is_err()
    );
    // Revoking a name that never joined cancels the owner's pending code.
    assert_eq!(
        devices.revoke_owned("acct-a", "spare").await.unwrap(),
        (false, 1)
    );
    assert!(devices.enroll("spare", &newer, "aa", false).await.is_err());
    // Once codes expire unspent, the name is free for anyone.
    devices
        .issue_owned_code("acct-a", "spare", [43; 16], OWNED_LIMIT)
        .await
        .unwrap();
    devices.expire_codes().await;
    devices
        .issue_owned_code("acct-b", "spare", [44; 16], OWNED_LIMIT)
        .await
        .unwrap();
}

#[tokio::test]
async fn an_account_holds_at_most_its_limit_and_revoking_frees_a_place() {
    let devices = or_skip!(test_registry());
    let one = devices
        .issue_owned_code("acct-a", "one", [50; 16], 2)
        .await
        .unwrap();
    devices.enroll("one", &one, "aa", false).await.unwrap();
    devices
        .issue_owned_code("acct-a", "two", [51; 16], 2)
        .await
        .unwrap();
    // A new code for a name it already holds is not a new machine.
    devices
        .issue_owned_code("acct-a", "two", [52; 16], 2)
        .await
        .unwrap();
    devices
        .issue_owned_code("acct-a", "one", [53; 16], 2)
        .await
        .unwrap();
    assert_eq!(
        devices
            .issue_owned_code("acct-a", "three", [54; 16], 2)
            .await
            .unwrap_err()
            .code,
        "machine_limit"
    );
    devices.revoke_owned("acct-a", "one").await.unwrap();
    devices
        .issue_owned_code("acct-a", "three", [55; 16], 2)
        .await
        .unwrap();
}

#[tokio::test]
async fn a_live_code_names_its_machine_until_it_is_spent_or_expires() {
    let devices = or_skip!(test_registry());
    let code = devices
        .issue_owned_code("acct-a", "platinum", [60; 16], OWNED_LIMIT)
        .await
        .unwrap();
    assert_eq!(
        devices.live_code(&code).await.unwrap().as_deref(),
        Some("platinum")
    );
    // Looking does not spend it.
    assert_eq!(
        devices.live_code(&code).await.unwrap().as_deref(),
        Some("platinum")
    );
    devices
        .enroll("platinum", &code, "aa", false)
        .await
        .unwrap();
    assert_eq!(devices.live_code(&code).await.unwrap(), None);
    let other = devices
        .issue_owned_code("acct-a", "spare", [61; 16], OWNED_LIMIT)
        .await
        .unwrap();
    devices.expire_codes().await;
    assert_eq!(devices.live_code(&other).await.unwrap(), None);
    assert_eq!(devices.live_code("dvg-0000").await.unwrap(), None);
}

#[tokio::test]
async fn reassign_owner_moves_a_machine_onto_a_devagotchi_account() {
    let devices = or_skip!(test_registry());
    let code = devices
        .issue_owned_code("300000000000000001", "gus", [9; 16], OWNED_LIMIT)
        .await
        .unwrap();
    devices.enroll("gus", &code, "aa", false).await.unwrap();
    let account = format!("dg_{}", "ab".repeat(16));
    let moved = devices
        .reassign_owner("300000000000000001", &account)
        .await
        .unwrap();
    assert!(moved >= 1);
    assert_eq!(
        devices.get("gus").await.unwrap().unwrap().owner.as_deref(),
        Some(account.as_str())
    );
    assert_eq!(
        devices
            .reassign_owner("300000000000000001", &account)
            .await
            .unwrap(),
        0
    );
}
