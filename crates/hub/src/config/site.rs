//! `site`: the site directory the hub's pages, sign-in email and MCP display read, and
//! the site it holds once that directory checks out.

use std::sync::Arc;

use serde::Deserialize;

use super::HubConfig;

/// `site` in the hub's config.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteConfig {
    /// An absolute path to the site directory.
    #[serde(default)]
    pub dir: Option<String>,
}

pub(super) fn default_site() -> Arc<crate::site::SiteCell> {
    Arc::new(crate::site::SiteCell::new(crate::site::builtin()))
}

impl HubConfig {
    /// The site the hub's pages, email and MCP display read: the one `site.dir` names, or
    /// the built-in one.
    pub fn site(&self) -> Arc<crate::site::Site> {
        self.loaded_site.get()
    }

    /// Where the site the hub serves now is kept: the site watcher replaces it there.
    pub fn site_cell(&self) -> Arc<crate::site::SiteCell> {
        self.loaded_site.clone()
    }
}
