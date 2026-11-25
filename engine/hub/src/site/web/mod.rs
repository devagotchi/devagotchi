//! The site's public pages as the hub serves them: the landing page (`/`, `landing`), the
//! pages the AI directories link to (`/docs`, `/privacy`, `/terms`, `/support`, `public`),
//! what search engines and AI agents read (`/llms.txt`, `/llms-full.txt`, `/robots.txt`,
//! `/sitemap.xml`, `discovery`) and the site's files under `/assets/` (`media`). Their words
//! and files are the site's (`crate::site`).

use axum::{Router, routing::get};

use crate::AppState;

pub(crate) mod discovery;
pub(crate) mod landing;
pub(crate) mod media;
pub(crate) mod public;

#[cfg(test)]
pub(crate) mod tests;

/// The public pages, what search engines and AI agents read, and the site's files.
pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/assets/landing.js", get(landing::script))
        .route("/docs", get(public::docs))
        .route("/privacy", get(public::privacy))
        .route("/terms", get(public::terms))
        .route("/support", get(public::support))
        .route("/llms.txt", get(discovery::llms))
        .route("/llms-full.txt", get(discovery::llms_full))
        .route("/robots.txt", get(discovery::robots))
        .route("/sitemap.xml", get(discovery::sitemap))
        // The site's own files: its logo, icon, pictures and fonts.
        .route("/assets/{file}", get(media::asset))
}
