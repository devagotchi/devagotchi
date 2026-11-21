use super::*;

/// An explicit empty list permits nothing; missing unrestricted fields mean any.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum AllowedNames {
    Names(Vec<String>),
    Any(AnyName),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum AnyName {
    #[serde(rename = "any")]
    Any,
}
impl AllowedNames {
    pub fn any() -> Self {
        Self::Any(AnyName::Any)
    }
    pub fn contains(&self, name: &str) -> bool {
        match self {
            Self::Any(_) => true,
            Self::Names(v) => v.iter().any(|v| v == name),
        }
    }
    pub(super) fn validate(&self) -> BridgeResult<()> {
        match self {
            Self::Any(_) => Ok(()),
            Self::Names(v) => strings(v),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GrantSelector {
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "Vec<String>")]
    pub paths: Option<Vec<String>>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "Vec<Vec<String>>")]
    pub argv_prefix: Option<Vec<Vec<String>>>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "Vec<String>")]
    pub hosts: Option<Vec<String>>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "Vec<String>")]
    pub env_allow: Option<Vec<String>>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "Vec<String>")]
    pub actions: Option<Vec<String>>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "AllowedNames")]
    pub apps: Option<AllowedNames>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "AllowedNames")]
    pub windows: Option<AllowedNames>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "AllowedNames")]
    pub monitors: Option<AllowedNames>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "Vec<String>")]
    pub deny_keys: Option<Vec<String>>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "u32")]
    pub max_runtime_s: Option<u32>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "u32")]
    pub max_actions_per_minute: Option<u32>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "u32")]
    pub max_edge: Option<u32>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "u32")]
    pub tree_depth: Option<u32>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "String")]
    pub shell_profile: Option<String>,
}
pub(super) fn strings(values: &[String]) -> BridgeResult<()> {
    if values.len() > 256
        || values
            .iter()
            .any(|s| s.is_empty() || s.len() > 4096 || s.chars().any(char::is_control))
    {
        Err(invalid(
            "grant lists require at most 256 nonempty values without control characters",
        ))
    } else {
        Ok(())
    }
}
pub(super) fn argv(values: &[String]) -> BridgeResult<()> {
    if values.len() > 256
        || values.first().is_some_and(String::is_empty)
        || values.iter().any(|s| s.len() > 16_384 || s.contains('\0'))
    {
        Err(invalid("invalid command arguments"))
    } else {
        Ok(())
    }
}
pub(super) fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}
impl GrantSelector {
    pub fn validate(&self) -> BridgeResult<()> {
        for list in [
            &self.paths,
            &self.hosts,
            &self.env_allow,
            &self.actions,
            &self.deny_keys,
        ]
        .into_iter()
        .flatten()
        {
            strings(list)?;
        }
        for names in [&self.apps, &self.windows, &self.monitors]
            .into_iter()
            .flatten()
        {
            names.validate()?;
        }
        if let Some(prefixes) = &self.argv_prefix {
            if prefixes.len() > 256 {
                return Err(invalid("too many command prefixes"));
            }
            for prefix in prefixes {
                if prefix.is_empty() {
                    return Err(invalid("command prefix cannot be empty"));
                }
                argv(prefix)?;
            }
        }
        for limit in [
            self.max_runtime_s,
            self.max_actions_per_minute,
            self.max_edge,
            self.tree_depth,
        ]
        .into_iter()
        .flatten()
        {
            if limit == 0 {
                return Err(invalid("grant limits must be positive"));
            }
        }
        if let Some(profile) = &self.shell_profile {
            strings(std::slice::from_ref(profile))?;
        }
        if let Some(paths) = &self.paths {
            for path in paths {
                if lexical(path).is_none() {
                    return Err(invalid("grant paths must have an absolute or home anchor"));
                }
            }
        }
        if let Some(hosts) = &self.hosts {
            for host in hosts {
                if !valid_host(host, true) {
                    return Err(invalid("invalid granted host pattern"));
                }
            }
        }
        Ok(())
    }
}

/// Lexical comparison only. The broker must resolve actual paths and prevent link escapes.
pub(super) fn lexical(value: &str) -> Option<String> {
    let value = value.replace('\\', "/");
    if value.is_empty() || value.chars().any(char::is_control) || value.starts_with("//") {
        return None;
    }
    let (anchor, tail) = if let Some(tail) = value.strip_prefix('/') {
        ("/".to_owned(), tail)
    } else if value == "~" || value.starts_with("~/") {
        ("~".into(), value.get(2..).unwrap_or(""))
    } else if value.len() >= 3 && value.as_bytes()[0].is_ascii_alphabetic() && &value[1..3] == ":/"
    {
        (value[..3].to_owned(), &value[3..])
    } else {
        return None;
    };
    let mut parts = Vec::new();
    for part in tail.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            p if p.contains(':') => return None,
            p => parts.push(p),
        }
    }
    Some(format!(
        "{}{}{}",
        anchor,
        if anchor.ends_with('/') || parts.is_empty() {
            ""
        } else {
            "/"
        },
        parts.join("/")
    ))
}
pub(super) fn path_within(path: &str, root: &str) -> bool {
    let (Some(path), Some(root)) = (lexical(path), lexical(root)) else {
        return false;
    };
    path == root || path.starts_with(&format!("{}/", root.trim_end_matches('/')))
}
pub(super) fn valid_host(host: &str, pattern: bool) -> bool {
    if pattern && host == "*" {
        return true;
    }
    if host.parse::<std::net::IpAddr>().is_ok() {
        return true;
    }
    let wildcard = pattern && host.starts_with("*.");
    let host = if pattern {
        host.strip_prefix("*.").unwrap_or(host)
    } else {
        host
    };
    let host = host.strip_suffix('.').unwrap_or(host);
    !host.is_empty()
        && !(wildcard && host.parse::<std::net::IpAddr>().is_ok())
        && host.len() <= 253
        && host.split('.').all(|part| {
            !part.is_empty()
                && part.len() <= 63
                && !part.starts_with('-')
                && !part.ends_with('-')
                && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}
pub(super) fn host_within(host: &str, pattern: &str) -> bool {
    if !valid_host(host, true) || !valid_host(pattern, true) {
        return false;
    }
    let host = host.strip_suffix('.').unwrap_or(host).to_ascii_lowercase();
    let pattern = pattern
        .strip_suffix('.')
        .unwrap_or(pattern)
        .to_ascii_lowercase();
    if host == pattern || pattern == "*" {
        return true;
    }
    match (
        host.parse::<std::net::IpAddr>(),
        pattern.parse::<std::net::IpAddr>(),
    ) {
        (Ok(a), Ok(b)) => return a == b,
        (Ok(_), _) | (_, Ok(_)) => return false,
        _ => {}
    }
    let Some(root) = pattern.strip_prefix("*.") else {
        return false;
    };
    let host = host.strip_prefix("*.").unwrap_or(&host);
    host.ends_with(&format!(".{root}"))
}
