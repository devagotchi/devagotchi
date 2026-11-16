use devagotchi_contracts::{Caller, GetInput, LOCAL_ACCOUNT, SendInput};

use super::*;

fn input_from(caller: Option<Caller>) -> RequestBody {
    RequestBody::OperationInput(SendInput {
        operation_id: "op".into(),
        text: Some("x".into()),
        keys: Vec::new(),
        take_over: true,
        caller,
    })
}

#[test]
fn a_hub_speaks_only_for_its_own_accounts_never_for_this_machines_user() {
    let local = Caller {
        account: LOCAL_ACCOUNT.into(),
        client: "adapter-1".into(),
    };
    let refused = from_a_hub(&input_from(Some(local))).unwrap_err();
    assert_eq!(refused.code, "not_your_operation");
    let other = Caller {
        account: "someone".into(),
        client: "c".into(),
    };
    assert!(from_a_hub(&input_from(Some(other))).is_err());
    assert!(from_a_hub(&input_from(Some(Caller::hub("acct-1", "session-1")))).is_ok());
    // No caller: the broker itself refuses what needs one (input, an interactive start).
    assert!(from_a_hub(&input_from(None)).is_ok());
    let get = RequestBody::OperationGet(GetInput {
        operation_id: "op".into(),
    });
    assert!(from_a_hub(&get).is_ok());
}
