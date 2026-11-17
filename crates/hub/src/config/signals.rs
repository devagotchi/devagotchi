//! `signals`: where the hub pushes its signals, the versioned stream of what happens on it
//! (`notify/signals/`, `docs/signals.md`), and the checks each sink passes.

use serde::Deserialize;

use super::valid_name;

/// The hub's signal sinks (`notify/signals/`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignalsConfig {
    #[serde(default)]
    pub sinks: Vec<SinkConfig>,
}

/// One receiver of the hub's signals: an HTTPS endpoint that takes them as NDJSON.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SinkConfig {
    /// What the hub calls it in `/healthz` and its log: `[a-z0-9][a-z0-9-]{0,62}`.
    pub name: String,
    /// Where batches are posted: https, or http to this machine or a service on the same
    /// private network (a name without a dot, as Compose gives one).
    pub url: String,
    /// The environment variable holding the bearer token the sink expects, if any.
    #[serde(default)]
    pub token_env: Option<String>,
    /// The environment variable holding the secret each batch is signed with
    /// (`Devagotchi-Signature: sha256=<HMAC of the body>`), if any.
    #[serde(default)]
    pub secret_env: Option<String>,
    /// `metadata` (the default): what happened, to what, by whom and how it ended, never
    /// a signal's `data`. `full`: with `data` (ticket text, notes, issue bodies).
    #[serde(default)]
    pub fields: SinkFields,
    /// Which sources it takes: `board`, `tenancy`, `audit`. Empty: all of them.
    #[serde(default)]
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SinkFields {
    #[default]
    Metadata,
    Full,
}

fn valid_env_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 128
        && (bytes[0].is_ascii_uppercase() || bytes[0] == b'_')
        && bytes
            .iter()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || *b == b'_')
}

/// A sink's URL is https, or http to a host that is this machine or a private service
/// name (no dot): a token or signals never cross a network in the clear.
fn sink_url_ok(url: &str) -> bool {
    if let Some(rest) = url.strip_prefix("https://") {
        return !rest.is_empty();
    }
    let Some(rest) = url.strip_prefix("http://") else {
        return false;
    };
    let authority = rest.split('/').next().unwrap_or_default();
    let host = match authority.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or_default(),
        None => authority
            .rsplit_once(':')
            .map_or(authority, |(host, _)| host),
    };
    matches!(host, "localhost" | "127.0.0.1" | "::1")
        || (!host.is_empty() && !host.contains('.') && valid_name(host))
}

impl SignalsConfig {
    /// Each sink's name (once each), URL, secret variables and sources.
    pub(super) fn check(&self) -> Result<(), String> {
        let mut sink_names = std::collections::BTreeSet::new();
        for sink in &self.sinks {
            if !valid_name(&sink.name) || !sink_names.insert(sink.name.clone()) {
                return Err(format!(
                    "signals.sinks: {:?} is not a sink name, or is used twice",
                    sink.name
                ));
            }
            if !sink_url_ok(&sink.url) {
                return Err(format!(
                    "signals.sinks {}: url {:?} must be https, or http to this machine or a private service name",
                    sink.name, sink.url
                ));
            }
            for name in sink.token_env.iter().chain(sink.secret_env.iter()) {
                if !valid_env_name(name) {
                    return Err(format!(
                        "signals.sinks {}: {name:?} is not an environment variable name",
                        sink.name
                    ));
                }
            }
            if let Some(source) = sink
                .sources
                .iter()
                .find(|s| !matches!(s.as_str(), "board" | "tenancy" | "audit"))
            {
                return Err(format!(
                    "signals.sinks {}: source {source:?} is board, tenancy or audit",
                    sink.name
                ));
            }
        }
        Ok(())
    }
}
