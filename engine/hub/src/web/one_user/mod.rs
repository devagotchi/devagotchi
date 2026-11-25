//! A single environment has local users managed by its owner, as requested 2026-11-20.
//! Organization/invite/tenant management remains hosted-only. This replaces the earlier
//! one-user-only interpretation; it does not change existing project permissions.
//! Owns no tables. Serves the mode's people routes. Settings: tenancy.mode, deploy-time.
//! Depends on web::orgs, web::people and config.

use axum::Router;

use crate::AppState;

/// Hosted organizations or local user administration, chosen once at router assembly.
pub(super) fn routes(multi_tenant: bool) -> Router<AppState> {
    if multi_tenant {
        super::orgs::routes()
    } else {
        super::people::routes()
    }
}

#[cfg(test)]
mod tests;
