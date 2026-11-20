//! The hub's site: every word, name, link, colour and picture its pages, its sign-in email
//! and its MCP display show, from a site directory instead of the code. Design tokens and
//! content are kept apart from the code that lays them out, so one hub binary serves any
//! brand (`crates/hub/README.md`, "The site").
//!
//! ```text
//! site/
//!   site.json              product, operator, links, sign-in labels, landing order, MCP,
//!                          plans, services, early access, which assets are used
//!   tokens.json            colours, type, radii and sizes the stylesheet reads, and fonts
//!   content/landing.json   every landing section's copy
//!   content/messages.json  the sign-in email, the board's getting-started ticket, what
//!                          early access and plans say (waiting, a limit, "you're in"),
//!                          the workflow page's words
//!   content/workflows.json the step-by-step setups an AI hands its person as one link
//!   content/docs.md, privacy.md, terms.md, support.md   the public pages
//!   content/llms.md        what an AI reads at /llms.txt (`discovery.rs`)
//!   assets/                logo, icon, diagram, preview, fonts (served from 'self')
//! ```
//!
//! `site.dir` in the hub's config (or `DEVAGOTCHI_SITE_DIR`) names the directory. Only
//! `site.json` is required there: any other file left out is the built-in default's
//! (`crates/hub/site/default/`), and `tokens.json` and `messages.json` may give just the
//! values they change. The directory is read and checked once, when the hub starts; one
//! that does not check out stops the start with what is wrong and where. With no directory
//! the built-in default serves, compiled into the binary.

mod analytics_copy;
mod assets;
mod check;
#[cfg(test)]
mod assets_tests;
#[cfg(test)]
mod brand_tests;
pub mod discovery;
mod load;
mod markdown;
pub(crate) mod model;
pub mod offers;
mod plans;
mod pwa;
mod style;
#[cfg(test)]
pub(crate) mod tests;
pub(crate) mod text;
pub mod walkthrough;
mod watch;
pub(crate) mod workflows;

use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, LazyLock},
};

pub use assets::Asset;
use load::builtin_files;
pub use markdown::Page;
pub use model::{
    Action, Band, Front, Item, Landing, Limit, Messages, Plan, SignUp, SiteFile, Step, Variants,
    safe_href,
};
pub use plans::Cap;
pub use text::Ctx;
pub use watch::{EVERY as WATCH_EVERY, SiteCell, watch};
pub use workflows::{Workflow, WorkflowsFile};

/// The environment variable that names the site directory, over `site.dir` in the config.
pub const SITE_ENV: &str = "DEVAGOTCHI_SITE_DIR";

/// The pages a site has, by the path they are served at.
pub const PAGES: [&str; 4] = ["docs", "privacy", "terms", "support"];

/// The text files a site directory may hold, besides `assets/`.
const TEXT_FILES: [&str; 10] = [
    "site.json",
    "tokens.json",
    "content/landing.json",
    "content/messages.json",
    "content/workflows.json",
    "content/docs.md",
    "content/privacy.md",
    "content/terms.md",
    "content/support.md",
    "content/llms.md",
];
const MAX_TEXT_BYTES: u64 = 1024 * 1024;

/// A loaded, checked site.
pub struct Site {
    /// Where it was read from; `None` for the built-in default.
    pub dir: Option<PathBuf>,
    pub info: SiteFile,
    pub landing: Landing,
    pub messages: Messages,
    /// The workflows an AI can start for its person (`content/workflows.json`).
    pub workflows: WorkflowsFile,
    pub pages: BTreeMap<String, Page>,
    /// What an AI reads at `/llms.txt` (`content/llms.md`, `discovery.rs`).
    pub llms: Page,
    pub assets: BTreeMap<String, Asset>,
    style: style::Style,
}

impl std::fmt::Debug for Site {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.dir {
            Some(dir) => write!(f, "Site({})", dir.display()),
            None => write!(f, "Site(built in)"),
        }
    }
}

static BUILTIN: LazyLock<Arc<Site>> = LazyLock::new(|| {
    Arc::new(Site::from_files(builtin_files(), None).expect("the built-in site checks out"))
});

/// The built-in default site, compiled into the binary.
pub fn builtin() -> Arc<Site> {
    BUILTIN.clone()
}

impl Site {
    /// The product's name, as every page and message says it.
    pub fn name(&self) -> &str {
        &self.info.product.name
    }

    /// What a sign-in provider is called on this site's pages: its `sign_in.labels` entry,
    /// else `fallback` (the provider's own label from the hub's config).
    pub fn label<'a>(&'a self, provider: &str, fallback: &'a str) -> &'a str {
        self.info
            .sign_in
            .labels
            .get(provider)
            .map(String::as_str)
            .unwrap_or(fallback)
    }

    /// Where a provider's button goes among the others: its place in `sign_in.order`;
    /// providers it leaves out keep the config's order, after the ones it names.
    pub fn sign_in_rank(&self, provider: &str) -> usize {
        self.info
            .sign_in
            .order
            .iter()
            .position(|name| name == provider)
            .unwrap_or(usize::MAX)
    }

    /// The asset `site.json` names for `what`, if it names one.
    pub fn named(&self, what: Option<&String>) -> Option<&Asset> {
        what.and_then(|name| self.assets.get(name))
    }

    /// The whole stylesheet: the tokens, the fonts, the hub's own rules and the landing
    /// page's rail for this site's sections.
    pub fn stylesheet(&self) -> &str {
        &self.style.css
    }

    /// The stylesheet's strong ETag: it is revalidated, so a site read again restyles
    /// every page at once.
    pub fn stylesheet_etag(&self) -> &str {
        &self.style.etag
    }

    /// Whether the pages load fonts (their CSP then allows `font-src 'self'`).
    pub fn has_fonts(&self) -> bool {
        self.style.fonts
    }

    /// The colour a phone paints its bar in for the installed app.
    pub fn theme_color(&self) -> &str {
        &self.style.theme_color
    }

    /// The MCP server's name and instructions as AI clients see them.
    pub fn mcp_info(&self) -> rmcp::model::Implementation {
        let mcp = &self.info.mcp;
        let mut info =
            rmcp::model::Implementation::new(mcp.name.clone(), env!("CARGO_PKG_VERSION"));
        if let Some(title) = &mcp.title {
            info = info.with_title(Ctx::new(self).plain(title));
        }
        info
    }

    /// What every AI client is told when it connects to the hub at `public_url`:
    /// `{hub}`, `{docs}` and `{llms}` are its address, its docs and its `/llms.txt`.
    pub fn mcp_instructions(&self, public_url: &str) -> String {
        discovery::instructions_ctx(Ctx::new(self), public_url).plain(&self.info.mcp.instructions)
    }

    /// What the MCP instructions add while the site's offers show (`site/offers.rs`).
    pub fn offers_instructions(&self) -> Option<String> {
        self.info
            .offers()
            .map(|offers| Ctx::new(self).plain(&offers.instructions))
    }

    /// The sign-in email: subject and body, with the link in it.
    pub fn sign_in_email(&self, link: &str) -> (String, String) {
        let ctx = Ctx::new(self).with("link", link);
        (
            ctx.plain(&self.messages.email.subject),
            ctx.plain(&self.messages.email.body),
        )
    }

    /// The getting-started ticket a new board shows: title and steps, with this hub's
    /// address in them.
    pub fn welcome(&self, public_url: &str) -> (String, String) {
        let ctx = Ctx::new(self).with("hub", public_url.trim_end_matches('/'));
        (
            ctx.plain(&self.messages.welcome.title),
            ctx.plain(&self.messages.welcome.body),
        )
    }

    /// A site may name `links.hosted_hub` for its own pages. The command line does not
    /// read it: a login names its hub with `--hub` or `DEVAGOTCHI_HUB`.
    #[cfg(test)]
    pub fn hosted_hub(&self) -> Option<&str> {
        self.info
            .links
            .get("hosted_hub")
            .map(String::as_str)
            .filter(|url| !url.is_empty())
    }
}
