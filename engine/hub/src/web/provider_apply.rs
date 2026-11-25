//! Settings-page rows over the config file's sign-in providers.
//!
//! A row turned off hides that name. A row with no secret of its own keeps the file's
//! secret when the client id is the same, so the Shippin switch does not wipe the client
//! `install.sh --nomen` installed. A saved secret replaces the file. A public client (no
//! secret anywhere) is offered as itself.

use std::sync::Arc;

use super::providers::Provider;
use crate::{
    config::ProviderConfig,
    identity::{Sealer, accounts::sign_in_settings::StoredSignIn},
};

pub(super) fn overlay(
    from_config: &[Arc<Provider>],
    stored: &[StoredSignIn],
    sealer: &Sealer,
) -> Vec<Arc<Provider>> {
    let mut list: Vec<Arc<Provider>> = from_config
        .iter()
        .filter(|provider| !stored.iter().any(|row| row.name == provider.config.name()))
        .cloned()
        .collect();
    for setting in stored.iter().filter(|row| row.enabled) {
        let file = from_config
            .iter()
            .find(|provider| provider.config.name() == setting.name);
        let secret = if !setting.sealed_secret.is_empty() {
            match sealer.open_value(&setting.name, &setting.sealed_secret) {
                Some(secret) => Some(secret),
                None => {
                    eprintln!(
                        "devagotchi-hub: sign-in {}: its secret does not open with this hub's key; save it again",
                        setting.name
                    );
                    continue;
                }
            }
        } else if file.is_some_and(|provider| provider.client_id() == setting.client_id) {
            file.and_then(|provider| provider.secret().map(str::to_string))
        } else {
            None
        };
        let Some(provider) = from_stored(setting, secret) else {
            continue;
        };
        list.push(provider);
    }
    list
}

fn from_stored(setting: &StoredSignIn, secret: Option<String>) -> Option<Arc<Provider>> {
    let mut described = serde_json::json!({ "kind": setting.kind, "name": setting.name });
    if !setting.label.is_empty() {
        described["label"] = setting.label.clone().into();
    }
    if !setting.issuer.is_empty() {
        described["issuer"] = setting.issuer.clone().into();
    }
    let config = match serde_json::from_value::<ProviderConfig>(described) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("devagotchi-hub: sign-in {}: {error}", setting.name);
            return None;
        }
    };
    Some(Provider::from_parts(
        config,
        setting.client_id.clone(),
        secret,
    ))
}
