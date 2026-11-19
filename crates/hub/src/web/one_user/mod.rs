//! The one-user hub (`tenancy.mode single`, the standalone build) manages no other people
//! (operator ruling, 2026-11-18; increment 225). A hosted hub (`tenancy.mode multi`) keeps
//! every multi-user surface; a one-user hub leaves them out:
//!
//! - `/orgs*`, `/invite` and `/assets/invite.js` are not mounted, so they get the empty 404
//!   an unknown path gets (`orgs`, called where `routes.rs` merges the organisation pages);
//! - `/admin` has no People section and no chip for it (`hub_settings.rs`);
//! - You (`/app/me`) has no Organisations card, and Boards no link to them (`app/`).
//!
//! # Owns
//! No tables.
//!
//! # Serves
//! No routes of its own: it decides whether `web/orgs.rs`'s routes are mounted.
//!
//! # Settings
//! `tenancy.mode` (deploy-time).
//!
//! # Depends on
//! `web/orgs.rs`, `config` (`HubConfig::multi_tenant`).

use axum::Router;

use crate::AppState;

/// The organisation pages (`web/orgs.rs`) on a hosted hub; nothing on a one-user hub, so
/// `/orgs*`, `/invite` and `/assets/invite.js` answer as an unknown path does.
pub(super) fn orgs(multi_tenant: bool) -> Router<AppState> {
    if multi_tenant {
        super::orgs::routes()
    } else {
        Router::new()
    }
}

#[cfg(test)]
mod tests;
