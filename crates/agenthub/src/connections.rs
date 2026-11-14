//! Connections: the outside services agents and apps call through the hub (plan:
//! `.ai/dev/plans/connections-gateway.md`). This is the part that needs no store: the catalog
//! of services, where a request may go, how the key is attached, and keeping the key out of
//! whatever comes back. The hub keeps each tenant's connections sealed and makes the calls
//! (`crates/hub/src/connections.rs`).
//!
//! A call reaches only its service's own host: the path is checked before the URL is built,
//! and the built URL's origin is checked again after, so nothing an agent passes can send the
//! key elsewhere.

/// How a service takes its key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Auth {
    /// `Authorization: Bearer <key>`.
    Bearer,
    /// `Authorization: <key>`, as Linear takes a personal key.
    Raw,
    /// A header of the service's own, `<name>: <key>`.
    Header(&'static str),
}

/// One service a connection can be made to.
#[derive(Debug, Clone, Copy)]
pub struct Service {
    /// Its name in the catalog, and the default name of a connection to it.
    pub id: &'static str,
    pub title: &'static str,
    /// Its API's origin; every call goes here and nowhere else.
    pub base: &'static str,
    pub auth: Auth,
    /// Headers every call to it carries (an API version, an Accept, a User-Agent).
    pub headers: &'static [(&'static str, &'static str)],
    /// The call that proves a key works: method, path, and a JSON body when it takes one.
    pub check: (&'static str, &'static str, Option<&'static str>),
    /// Where a person makes the key.
    pub keys_at: &'static str,
    /// What its keys look like, to guide the person.
    pub key_hint: &'static str,
}

/// The services, in the order the page offers them. Keys first; OAuth connections for the
/// services that need a person's own account come later (plan X7).
pub const CATALOG: [Service; 11] = [
    Service {
        id: "resend",
        title: "Resend (email)",
        base: "https://api.resend.com",
        auth: Auth::Bearer,
        headers: &[],
        check: ("GET", "/domains", None),
        keys_at: "https://resend.com/api-keys",
        key_hint: "re_…",
    },
    Service {
        id: "stripe",
        title: "Stripe (payments)",
        base: "https://api.stripe.com",
        auth: Auth::Bearer,
        headers: &[],
        check: ("GET", "/v1/balance", None),
        keys_at: "https://dashboard.stripe.com/apikeys",
        key_hint: "sk_test_… or rk_… (a restricted key)",
    },
    Service {
        id: "github",
        title: "GitHub",
        base: "https://api.github.com",
        auth: Auth::Bearer,
        headers: &[
            ("accept", "application/vnd.github+json"),
            ("x-github-api-version", "2022-11-28"),
        ],
        check: ("GET", "/user", None),
        keys_at: "https://github.com/settings/personal-access-tokens",
        key_hint: "github_pat_… (fine-grained)",
    },
    Service {
        id: "slack",
        title: "Slack",
        base: "https://slack.com",
        auth: Auth::Bearer,
        headers: &[],
        check: ("POST", "/api/auth.test", None),
        keys_at: "https://api.slack.com/apps",
        key_hint: "xoxb-… (a bot token)",
    },
    Service {
        id: "vercel",
        title: "Vercel",
        base: "https://api.vercel.com",
        auth: Auth::Bearer,
        headers: &[],
        check: ("GET", "/v2/user", None),
        keys_at: "https://vercel.com/account/settings/tokens",
        key_hint: "a Vercel access token",
    },
    Service {
        id: "supabase",
        title: "Supabase (management)",
        base: "https://api.supabase.com",
        auth: Auth::Bearer,
        headers: &[],
        check: ("GET", "/v1/projects", None),
        keys_at: "https://supabase.com/dashboard/account/tokens",
        key_hint: "sbp_…",
    },
    Service {
        id: "linear",
        title: "Linear",
        base: "https://api.linear.app",
        auth: Auth::Raw,
        headers: &[],
        check: ("POST", "/graphql", Some(r#"{"query":"{ viewer { id } }"}"#)),
        keys_at: "https://linear.app/settings/account/security",
        key_hint: "lin_api_…",
    },
    Service {
        id: "openai",
        title: "OpenAI",
        base: "https://api.openai.com",
        auth: Auth::Bearer,
        headers: &[],
        check: ("GET", "/v1/models", None),
        keys_at: "https://platform.openai.com/api-keys",
        key_hint: "sk-…",
    },
    Service {
        id: "anthropic",
        title: "Anthropic",
        base: "https://api.anthropic.com",
        auth: Auth::Header("x-api-key"),
        headers: &[("anthropic-version", "2023-06-01")],
        check: ("GET", "/v1/models", None),
        keys_at: "https://console.anthropic.com/settings/keys",
        key_hint: "sk-ant-…",
    },
    Service {
        id: "postmark",
        title: "Postmark (email)",
        base: "https://api.postmarkapp.com",
        auth: Auth::Header("x-postmark-server-token"),
        headers: &[("accept", "application/json")],
        check: ("GET", "/server", None),
        keys_at: "https://account.postmarkapp.com/servers",
        key_hint: "a server API token",
    },
    Service {
        id: "notion",
        title: "Notion",
        base: "https://api.notion.com",
        auth: Auth::Bearer,
        headers: &[("notion-version", "2022-06-28")],
        check: ("GET", "/v1/users/me", None),
        keys_at: "https://www.notion.so/profile/integrations",
        key_hint: "ntn_… (an internal integration secret)",
    },
];

/// The service called `id`.
pub fn service(id: &str) -> Option<&'static Service> {
    CATALOG.iter().find(|service| service.id == id)
}

/// The methods a call may use.
pub const METHODS: [&str; 5] = ["GET", "POST", "PUT", "PATCH", "DELETE"];

/// The URL a call to `path` goes to on `base` (the service's origin, or a stand-in's in
/// tests), with `query` pairs encoded; refused unless it stays on that origin.
pub fn target(base: &str, path: &str, query: &[(String, String)]) -> Result<String, String> {
    let plain = path.starts_with('/')
        && !path.starts_with("//")
        && path.len() <= 2048
        && !path.contains("..")
        && !path.contains('\\')
        && !path.contains('?')
        && !path.contains('#')
        && !path.contains('@')
        && !path.chars().any(|c| c.is_control() || c.is_whitespace());
    if !plain {
        return Err(format!(
            "{path:?} is not a path on this service: it starts with / and has no .., ?, #, @, \\ or spaces (put query parameters in query)"
        ));
    }
    let origin = reqwest::Url::parse(base).map_err(|e| e.to_string())?;
    let mut url = reqwest::Url::parse(&format!("{}{path}", base.trim_end_matches('/')))
        .map_err(|e| e.to_string())?;
    if !query.is_empty() {
        let mut pairs = url.query_pairs_mut();
        for (key, value) in query {
            pairs.append_pair(key, value);
        }
    }
    if url.origin() != origin.origin() {
        return Err(format!("{path:?} leaves the service"));
    }
    Ok(url.to_string())
}

/// The headers a call carries: the service's own, then its key where the service takes it.
pub fn headers(service: &Service, key: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = service
        .headers
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect();
    out.push(("user-agent".into(), "devagotchi-hub".into()));
    match service.auth {
        Auth::Bearer => out.push(("authorization".into(), format!("Bearer {key}"))),
        Auth::Raw => out.push(("authorization".into(), key.to_string())),
        Auth::Header(name) => out.push((name.to_string(), key.to_string())),
    }
    out
}

/// `text` with every copy of `key` taken out: a service that echoes the request (or an
/// error that quotes it) never hands the key to the agent.
pub fn redact(text: &str, key: &str) -> String {
    if key.len() < 4 {
        return text.to_string();
    }
    text.replace(key, "[redacted]")
}

/// What a stored key is shown as: its last four characters, never more.
pub fn hint(key: &str) -> String {
    let tail: String = key
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("…{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_call_stays_on_its_services_host() {
        let base = "https://api.resend.com";
        assert_eq!(
            target(base, "/emails", &[]).unwrap(),
            "https://api.resend.com/emails"
        );
        assert_eq!(
            target(base, "/domains", &[("limit".into(), "a b&c".into())]).unwrap(),
            "https://api.resend.com/domains?limit=a+b%26c"
        );
        for bad in [
            "emails",
            "//evil.example/x",
            "/../x",
            "/x/../../y",
            "/x?key=1",
            "/x#frag",
            "/@evil.example",
            "/x y",
            "/x\\y",
            "https://evil.example/x",
        ] {
            assert!(target(base, bad, &[]).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_key_goes_where_the_service_takes_it_and_never_comes_back() {
        let resend = service("resend").unwrap();
        assert!(headers(resend, "re_x").contains(&("authorization".into(), "Bearer re_x".into())));
        let linear = service("linear").unwrap();
        assert!(headers(linear, "lin_x").contains(&("authorization".into(), "lin_x".into())));
        let anthropic = service("anthropic").unwrap();
        let sent = headers(anthropic, "sk-ant-x");
        assert!(sent.contains(&("x-api-key".into(), "sk-ant-x".into())));
        assert!(sent.contains(&("anthropic-version".into(), "2023-06-01".into())));
        assert!(!sent.iter().any(|(name, _)| name == "authorization"));
        assert_eq!(
            redact("{\"auth\":\"Bearer re_secret_1\"}", "re_secret_1"),
            "{\"auth\":\"Bearer [redacted]\"}"
        );
        assert_eq!(hint("re_secret_1234"), "…1234");
        let ids: std::collections::BTreeSet<_> = CATALOG.iter().map(|s| s.id).collect();
        assert_eq!(ids.len(), CATALOG.len(), "one entry per service");
        for service in CATALOG {
            assert!(service.base.starts_with("https://"), "{}", service.id);
            assert!(crate::valid_name(service.id));
            assert!(METHODS.contains(&service.check.0));
            assert!(target(service.base, service.check.1, &[]).is_ok());
        }
    }
}
