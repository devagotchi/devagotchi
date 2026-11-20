//! The hub's stylesheet after its tokens, in cascade order. `site/style.rs` serves one sheet:
//! the layer order (`@layer reset, tokens, base, components, pages`), the tokens in `tokens`,
//! `web/hub.css` and `web/landing.css` in `base`, then the sheets listed here.
//!
//! - [`PRIMITIVES`]: the layout primitives every page composes (`stack`, `cluster`,
//!   `auto-grid`, `sidebar`, `switcher`, `query`), in `@layer components`.
//! - [`PAGES`]: each page's own sheet, kept beside its Rust and scoped to that page
//!   (`@layer pages { [data-page="<page>"] … }`). A page's markup carries `data-page`.
//!
//! Sizes come from the fluid scales in `tokens.json`: `--step-*` for type and `--space-*` for
//! spacing, each a `clamp()` between a 360px phone and a 1240px desktop. So a page has no
//! viewport breakpoint of its own; where a component, not the page, decides its layout, it
//! uses a container query. `tests.rs` holds these rules.

/// The layout primitives, `@layer components`.
pub(crate) const PRIMITIVES: &str = include_str!("primitives.css");

/// Each page's sheet, by the `data-page` it is scoped to.
pub(crate) const PAGES: [(&str, &str); 4] = [
    ("board", include_str!("../app/agenthub/board.css")),
    (
        "board-configuration",
        include_str!("../app/agenthub/configuration/page.css"),
    ),
    ("ticket", include_str!("../app/agenthub/ticket.css")),
    ("approvals", include_str!("../approvals/page.css")),
];

#[cfg(test)]
mod tests;
