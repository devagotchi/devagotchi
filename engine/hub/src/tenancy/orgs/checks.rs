//! The checks on what a caller names (a slug, a subject, a title) and the two ways a call is refused.

use devagotchi_contracts::{BridgeError, BridgeResult};

use crate::{
    config::{valid_name, valid_subject},
    store::Failed,
};

pub(super) fn refused(code: &str, message: impl Into<String>) -> Failed {
    Failed::Refused(BridgeError::new(code, message))
}

pub(super) fn invalid(message: impl Into<String>) -> BridgeError {
    BridgeError::new("invalid", message)
}

pub(super) fn check_name(what: &str, name: &str) -> BridgeResult<()> {
    if valid_name(name) {
        Ok(())
    } else {
        Err(invalid(format!(
            "{what} {name:?} is not [a-z0-9][a-z0-9-]{{0,62}}"
        )))
    }
}

pub(super) fn check_subject(subject: &str) -> BridgeResult<()> {
    valid_subject(subject).map_err(invalid)
}

pub(super) fn check_title(name: &str) -> BridgeResult<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 100 || name.chars().any(char::is_control) {
        return Err(invalid(
            "a name is 1 to 100 characters, without control characters",
        ));
    }
    Ok(name.to_string())
}
