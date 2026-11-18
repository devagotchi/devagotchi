//! The hub's mail server, shared: what `/admin` → Email swaps and every sender reads.

use std::sync::Arc;

use super::email::Mailer;

/// The hub's mail server as every part of it sees it: set at start from the config file or the
/// settings page, replaced when the owner saves another, read at the moment a mail is sent.
/// A mail server the config file names (`auth.email`) is `from_config`: `/admin` shows it and
/// leaves it.
#[derive(Clone, Default)]
pub struct MailCell {
    cell: Arc<std::sync::RwLock<Option<Mailer>>>,
    from_config: bool,
}

impl MailCell {
    /// The config file's mail server, if it names one.
    pub fn from_config(mailer: Option<Mailer>) -> Self {
        Self {
            from_config: mailer.is_some(),
            cell: Arc::new(std::sync::RwLock::new(mailer)),
        }
    }

    /// Whether the config file names the mail server.
    pub fn is_from_config(&self) -> bool {
        self.from_config
    }

    /// The mail server now, if the hub has one.
    pub fn get(&self) -> Option<Mailer> {
        self.cell.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    pub fn set(&self, mailer: Option<Mailer>) {
        *self.cell.write().unwrap_or_else(|p| p.into_inner()) = mailer;
    }
}
