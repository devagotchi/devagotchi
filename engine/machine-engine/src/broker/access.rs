//! Who reaches which operation. Every request but a list names its operation, and only its
//! own account reaches it: this machine's own account and the inspector (no caller) reach all
//! of them; another account's answers as not found, so even its existence stays hidden. Typing
//! into one takes more: the keyboard (`claim_writer`).

use super::*;

/// The operation `id` names, as `caller` may reach it.
pub(super) fn reach<'a>(
    operations: &'a HashMap<String, Operation>,
    id: &str,
    caller: Option<&Caller>,
) -> BridgeResult<&'a Operation> {
    operations
        .get(id)
        .filter(|operation| visible(operation.record.owner.as_ref(), caller, ListScope::Account))
        .ok_or_else(not_found)
}

/// As `reach`, to change it.
pub(super) fn reach_mut<'a>(
    operations: &'a mut HashMap<String, Operation>,
    id: &str,
    caller: Option<&Caller>,
) -> BridgeResult<&'a mut Operation> {
    operations
        .get_mut(id)
        .filter(|operation| visible(operation.record.owner.as_ref(), caller, ListScope::Account))
        .ok_or_else(not_found)
}

/// Whether a list for `caller` in `scope` shows an operation `owner` started. Without a
/// caller (the inspector, an older client) everything shows. This machine's own account
/// owns everything its broker runs, an operation started without an owner included.
pub(super) fn visible(owner: Option<&Caller>, caller: Option<&Caller>, scope: ListScope) -> bool {
    let Some(caller) = caller else {
        return true;
    };
    match (owner, scope) {
        (Some(owner), ListScope::Mine) => owner == caller,
        (_, ListScope::Account) if caller.account == LOCAL_ACCOUNT => true,
        (Some(owner), ListScope::Account) => owner.account == caller.account,
        (None, _) => false,
    }
}

/// Give `caller` the keyboard of an interactive operation, or say why not. Only its own
/// account ever drives it; another conversation or client of that account asks explicitly
/// (`take_over`); and while one client holds the keyboard (its last input within the lease)
/// no other types into it unless it takes over, so keystrokes never interleave.
pub(super) fn claim_writer(
    operation: &mut Operation,
    caller: &Caller,
    take_over: bool,
) -> BridgeResult<InputMode> {
    let Some(interactive) = &operation.record.view.interactive else {
        return Err(BridgeError::new(
            "not_interactive",
            "This operation takes no input; start it with interactive to type into it",
        ));
    };
    let Some(owner) = &operation.record.owner else {
        return Err(input_closed());
    };
    if owner.account != caller.account {
        return Err(BridgeError::new(
            "not_your_operation",
            "This operation belongs to another account; only that account can drive it",
        ));
    }
    if !interactive.input_open || operation.input.is_none() {
        return Err(input_closed());
    }
    if owner.client != caller.client && !take_over {
        return Err(BridgeError::new(
            "not_your_session",
            "Another conversation or client of your account started this operation. To drive it anyway, call again with take_over: true",
        ));
    }
    let now = now_ms();
    if let Some((holder, until)) = &operation.writer
        && holder != &caller.client
        && *until > now
        && !take_over
    {
        return Err(BridgeError::new(
            "writer_busy",
            format!(
                "Another client of your account is typing into this operation; its hold ends in {} s. Wait, or call again with take_over: true",
                (until - now).div_ceil(1000)
            ),
        ));
    }
    let mode = interactive.mode;
    operation.writer = Some((caller.client.clone(), now + WRITER_LEASE_MS));
    if let Some(interactive) = &mut operation.record.view.interactive {
        interactive.writer_until_ms = Some(now + WRITER_LEASE_MS);
    }
    Ok(mode)
}
