//! What `/admin` offers to change while the hub runs: each setting's key, card, words, kind
//! and range, and the config keys that are set at deploy instead, with why (`super`).

/// Which card of `/admin` a setting is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    SignUp,
    Passwords,
    Machines,
    Workflows,
    Analytics,
}

impl Group {
    pub const ALL: [Group; 5] = [
        Group::SignUp,
        Group::Passwords,
        Group::Machines,
        Group::Workflows,
        Group::Analytics,
    ];

    /// The card's anchor and form name.
    pub fn id(self) -> &'static str {
        match self {
            Group::SignUp => "sign-up",
            Group::Passwords => "passwords",
            Group::Machines => "machines",
            Group::Workflows => "workflows",
            Group::Analytics => "visits",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|group| group.id() == id)
    }

    pub fn title(self) -> &'static str {
        match self {
            Group::SignUp => "New accounts and their limits",
            Group::Passwords => "Passwords",
            Group::Machines => "Machine updates",
            Group::Workflows => "Workflow links",
            Group::Analytics => "Visits to the public pages",
        }
    }

    pub fn help(self) -> &'static str {
        match self {
            Group::SignUp => {
                "How fast new accounts may be made, and what an account nobody has secured yet (no password, passkey or sign-in) may hold. A hosted hub (tenancy multi) uses all of these; a single owner's hub uses the machine and client limits for accounts its owner lets in."
            }
            Group::Passwords => "What a password must be, for sign-up and for every change.",
            Group::Machines => {
                "Whether machines move to the build this hub hands out by themselves. Update one any time from Machines either way."
            }
            Group::Workflows => {
                "How long the link of a workflow an AI starts for a person (/go/…) keeps working. The link is the credential, so it is short."
            }
            Group::Analytics => {
                "A hosted hub counts visits to its public pages for its operator. Raw visits are kept this long; the daily totals stay."
            }
        }
    }
}

/// What kind of value a setting takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A whole number from `min` to `max`, in `unit`.
    Number {
        min: i64,
        max: i64,
        unit: &'static str,
    },
    /// On or off.
    Switch,
}

/// One setting of `/admin`.
#[derive(Debug, Clone, Copy)]
pub struct Setting {
    /// Its key in the config file (`tenancy.starts_per_hour`), and the row name in Postgres.
    pub key: &'static str,
    pub group: Group,
    pub label: &'static str,
    pub help: &'static str,
    pub kind: Kind,
    /// The environment variable that pins it, if one does.
    pub env: Option<&'static str>,
}

const fn number(min: i64, max: i64, unit: &'static str) -> Kind {
    Kind::Number { min, max, unit }
}

/// Every setting the owner changes on `/admin` while the hub runs.
pub const SETTINGS: &[Setting] = &[
    Setting {
        key: "tenancy.starts_per_hour",
        group: Group::SignUp,
        label: "New accounts per address, per hour",
        help: "How many accounts one client address may make in an hour.",
        kind: number(1, 10_000, "accounts"),
        env: None,
    },
    Setting {
        key: "tenancy.max_unsecured",
        group: Group::SignUp,
        label: "Unsecured accounts at once",
        help: "Accounts nobody has secured yet that may be live at the same time, on the whole hub.",
        kind: number(1, 10_000_000, "accounts"),
        env: None,
    },
    Setting {
        key: "tenancy.anonymous_connect",
        group: Group::SignUp,
        label: "Connect an AI with no sign-in",
        help: "On a hosted hub, someone with no session may connect an AI straight away, and the hub makes an account nobody has secured yet. Off: they sign in or sign up first.",
        kind: Kind::Switch,
        env: None,
    },
    Setting {
        key: "tenancy.unsecured.machines",
        group: Group::SignUp,
        label: "Machines an unsecured account may add",
        help: "0 means it adds none until it is secured.",
        kind: number(0, 100, "machines"),
        env: None,
    },
    Setting {
        key: "tenancy.unsecured.clients",
        group: Group::SignUp,
        label: "AI clients an unsecured account may connect",
        help: "Connections at once, terminals included.",
        kind: number(1, 100, "clients"),
        env: None,
    },
    Setting {
        key: "tenancy.unsecured.idle_days",
        group: Group::SignUp,
        label: "Days before an unused, unsecured account ends",
        help: "It ends with everything it connected.",
        kind: number(1, 90, "days"),
        env: None,
    },
    Setting {
        key: "passwords.min_length",
        group: Group::Passwords,
        label: "Shortest password",
        help: "In characters. 15 to 18 is recommended; the hub never allows fewer than 8.",
        kind: number(8, 128, "characters"),
        env: None,
    },
    Setting {
        key: "machines.auto_update",
        group: Group::Machines,
        label: "Machines update themselves",
        help: "Once an hour, and when one connects, each online machine on another build with no running jobs updates itself.",
        kind: Kind::Switch,
        env: None,
    },
    Setting {
        key: "workflows.link_ttl_secs",
        group: Group::Workflows,
        label: "A workflow link works for",
        help: "300 is five minutes, 86400 a day. A link also stops the moment its workflow ends.",
        kind: number(300, 86_400, "seconds"),
        env: None,
    },
    Setting {
        key: "analytics.retention_days",
        group: Group::Analytics,
        label: "Keep each visit for",
        help: "After this only the daily totals remain.",
        kind: number(1, 3650, "days"),
        env: Some(crate::config::analytics::RETENTION_ENV),
    },
];

/// Config keys set only when the hub is deployed or installed, never on `/admin`, and why.
/// With [`SETTINGS`], every key of the config file is in exactly one of the two.
pub const DEPLOY_TIME: &[(&str, &str)] = &[
    (
        "listen",
        "the socket the hub binds; a change needs a restart",
    ),
    (
        "public_url",
        "the hub's own address; install.sh sets it with the domain",
    ),
    (
        "auth",
        "how callers prove who they are and how long tokens live; sign-in providers and email are on /admin (an auth.email here pins the Email card)",
    ),
    (
        "trusted_issuers",
        "other OpenID issuers whose tokens the hub accepts; set with the deploy",
    ),
    (
        "owner",
        "who can always get in; a page that could change it could take the hub",
    ),
    (
        "brokers",
        "operator-configured machines; machines people add are on Machines",
    ),
    ("grants", "who reaches the operator-configured machines"),
    ("boards", "project boards on the hub's own disk"),
    ("github", "repositories admitted to those boards"),
    ("account_boards", "where boards for accounts are kept"),
    ("web", "the legacy Nomen pages; hub accounts have their own"),
    (
        "downloads",
        "the builds the hub hands out; laid down with the image",
    ),
    (
        "storage",
        "the hub's Postgres; the settings live in it, so it cannot be set from it",
    ),
    (
        "clickhouse",
        "set here it pins the ClickHouse card on /admin; leave it out to set it there",
    ),
    (
        "signals",
        "signal sinks; their card on /admin is the next step (hub-settings M5)",
    ),
    (
        "tenancy.mode",
        "single or multi changes who may own the hub; set at install",
    ),
    (
        "tenancy.domain",
        "tenant hostnames need DNS and TLS for the domain",
    ),
    (
        "tenancy.trusted_proxies",
        "the proxies in front of the hub are part of the deploy",
    ),
    (
        "operators",
        "who runs a hosted hub; set at deploy so no page can add one",
    ),
    ("site", "the site directory the pages are drawn from"),
    (
        "analytics.enabled",
        "whether a hosted hub counts visits at all; the counter starts with the hub",
    ),
    (
        "shippin",
        "the network's provisioning token, a deploy secret",
    ),
];

#[cfg(test)]
/// The sections of the config file some of whose keys are on `/admin`: the check walks into
/// them, key by key, instead of listing the section whole.
pub const SPLIT_SECTIONS: &[(&str, &str)] = &[
    ("tenancy", "TenancyConfig"),
    ("tenancy.unsecured", "UnsecuredLimits"),
    ("machines", "MachinesConfig"),
    ("workflows", "WorkflowsConfig"),
    ("analytics", "AnalyticsConfig"),
];

/// Settings that exist only on `/admin`, with no key in the config file.
#[cfg(test)]
pub const PAGE_ONLY: &[&str] = &["passwords.min_length"];

/// The value a setting has: a number or a switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Value {
    Number(i64),
    Switch(bool),
}

impl Value {
    /// As stored in Postgres and shown in a form.
    pub fn text(self) -> String {
        match self {
            Value::Number(n) => n.to_string(),
            Value::Switch(on) => if on { "on" } else { "off" }.to_string(),
        }
    }
}

impl Setting {
    pub fn find(key: &str) -> Option<&'static Setting> {
        SETTINGS.iter().find(|setting| setting.key == key)
    }

    /// `text` as this setting's value, inside its range; else why not, in words for the page.
    pub fn parse(&self, text: &str) -> Result<Value, String> {
        let text = text.trim();
        match self.kind {
            Kind::Switch => match text {
                "on" | "true" => Ok(Value::Switch(true)),
                "off" | "false" | "" => Ok(Value::Switch(false)),
                _ => Err(format!("{} is on or off", self.label)),
            },
            Kind::Number { min, max, unit } => match text.parse::<i64>() {
                Ok(n) if (min..=max).contains(&n) => Ok(Value::Number(n)),
                _ => Err(format!("{} is {min} to {max} {unit}", self.label)),
            },
        }
    }
}
