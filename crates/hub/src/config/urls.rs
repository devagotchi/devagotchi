//! The URL checks the config's sections share: an absolute URL's host, a passkey relying
//! party against the hub's host, and plain http allowed only on this machine's loopback.

/// The host of an absolute URL, lowercased, without port or brackets.
pub(crate) fn url_host(url: &str) -> Option<String> {
    let uri = url.parse::<http::Uri>().ok()?;
    let host = uri.host()?.trim_start_matches('[').trim_end_matches(']');
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

/// A passkey relying party is the hub's host or a parent domain of it (WebAuthn §5.1.3):
/// lowercase DNS labels, and never a bare top-level domain.
pub(super) fn check_rp_id(rp_id: &str, host: &str) -> Result<(), String> {
    let well_formed = !rp_id.is_empty()
        && rp_id.len() <= 253
        && !rp_id.starts_with('.')
        && !rp_id.ends_with('.')
        && !rp_id.contains("..")
        && rp_id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'.');
    let covers = host == rp_id || host.ends_with(&format!(".{rp_id}"));
    if well_formed && covers && (rp_id.contains('.') || rp_id == host) {
        Ok(())
    } else {
        Err(format!(
            "auth.rp_id {rp_id:?} must be the hub's host {host} or a parent domain of it, in lowercase"
        ))
    }
}

/// `http://` to this machine's loopback only, where no network carries the traffic.
pub(super) fn loopback_http(url: &str) -> bool {
    let Ok(uri) = url.parse::<http::Uri>() else {
        return false;
    };
    uri.scheme_str() == Some("http")
        && uri.authority().is_some_and(|a| !a.as_str().contains('@'))
        && matches!(
            uri.host(),
            Some("127.0.0.1" | "localhost" | "[::1]" | "::1")
        )
}
