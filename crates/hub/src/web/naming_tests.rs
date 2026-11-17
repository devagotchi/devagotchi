//! No other product is named anywhere a person or an AI reads, but in the landing page's
//! comparison (`#compare`, `landing/compare.rs`), which sets one beside this one and cites its
//! source: not in the site's files, any page, `llms.txt`, the MCP instructions or a tool.

use super::discovery_tests::{get, hosted};
use crate::site::{self, Ctx, Front};

/// The ways another product's name could be written.
const OTHERS: [&str; 3] = ["desktop commander", "desktopcommander", "desktop-commander"];

/// A page without its comparison (`#compare`, its evidence popups inside it): the one place
/// the landing page names another product, side by side with this one and citing its source.
fn without_comparison(html: &str) -> String {
    match html.find("<section class=\"band\" id=\"compare\"") {
        Some(at) => {
            let end = at + html[at..].find("</section>").expect("the comparison ends") + 10;
            format!("{}{}", &html[..at], &html[end..])
        }
        None => html.to_string(),
    }
}

fn assert_names_no_other_product(what: &str, text: &str) {
    let lower = text.to_lowercase();
    for name in OTHERS {
        assert!(!lower.contains(name), "{what} names {name}");
    }
}

#[tokio::test]
async fn no_other_product_is_named_in_the_site_any_page_or_the_mcp_surface() {
    // Every file of the built-in site, as written.
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("site/default");
    let mut files = vec![dir];
    while let Some(path) = files.pop() {
        if path.is_dir() {
            for entry in std::fs::read_dir(&path).unwrap() {
                files.push(entry.unwrap().path());
            }
        } else if let Ok(text) = std::fs::read_to_string(&path) {
            // The landing page's comparison names the other product; nothing else does.
            let text = if path.ends_with("landing.json") {
                let mut landing: serde_json::Value = serde_json::from_str(&text).unwrap();
                landing.as_object_mut().unwrap().remove("compare");
                landing.to_string()
            } else {
                text
            };
            assert_names_no_other_product(&path.display().to_string(), &text);
        }
    }
    // Every landing page the hub can draw.
    let site = site::builtin();
    for front in [
        Front::Unclaimed,
        Front::Passkeys,
        Front::Nomen,
        Front::Hosted,
    ] {
        let ctx = Ctx::new(&site);
        let (title, body) = super::landing_for(&site, &ctx, front, "https://hub.example.org");
        assert_names_no_other_product(
            front.key(),
            &format!("{title}{}", without_comparison(&body)),
        );
    }
    // Every page and file a hosted hub serves without sign-in.
    let hub = or_skip!(hosted());
    for path in [
        "/",
        "/docs",
        "/privacy",
        "/terms",
        "/support",
        "/llms.txt",
        "/llms-full.txt",
        "/robots.txt",
        "/sitemap.xml",
        "/.well-known/oauth-protected-resource",
        "/.well-known/oauth-protected-resource/mcp",
        "/.well-known/oauth-authorization-server",
        "/manifest.webmanifest",
        "/assets/hub.css",
        "/sw.js",
    ] {
        let (_, _, body) = get(&hub, path).await;
        assert_names_no_other_product(path, &without_comparison(&body));
    }
    // What an AI reads: the instructions, and every tool as tools/list describes it.
    let info = rmcp::ServerHandler::get_info(&hub.hub);
    assert_names_no_other_product("instructions", &info.instructions.unwrap_or_default());
    for tool in crate::mcp::tools() {
        assert_names_no_other_product(&tool.name, &serde_json::to_string(&tool).unwrap());
    }
}
