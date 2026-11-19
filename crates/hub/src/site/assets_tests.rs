//! A site's own files (`assets.rs`, `pwa.rs`): fonts it serves from this hub, and the
//! installed app's PNG icons, measured, with a separate maskable one and a badge.

use std::path::Path;

use serde_json::Value;

use super::{
    builtin,
    tests::{edit_json, neutral, variant},
};

#[test]
fn a_site_may_serve_its_own_fonts_from_this_hub() {
    let (_dir, loaded) = variant(|d| {
        let mut font = b"wOF2".to_vec();
        font.extend_from_slice(&[0; 60]);
        std::fs::write(d.join("assets/body.woff2"), font).unwrap();
        edit_json(&d.join("tokens.json"), |v| {
            v["font_files"] = serde_json::json!([
                { "family": "Body Sans", "file": "body.woff2", "weight": "100 900" }
            ]);
            v["fonts"] = serde_json::json!({ "ui": "\"Body Sans\", system-ui, sans-serif" });
        });
    });
    let site = loaded.unwrap();
    assert!(site.has_fonts());
    let font = &site.assets["body.woff2"];
    assert_eq!(font.mime, "font/woff2");
    let css = site.stylesheet();
    assert!(
        css.contains(&format!(
            "@font-face {{ font-family: \"Body Sans\"; src: url(/assets/body.woff2?v={}) format(\"woff2\");",
            font.version
        )),
        "{css}"
    );
    assert!(css.contains("--font-ui: \"Body Sans\", system-ui, sans-serif;"));
    // A font that is not one, or names a file the site does not have, is refused.
    let (_dir, loaded) = variant(|d| {
        std::fs::write(d.join("assets/body.woff2"), b"not a font").unwrap();
    });
    assert!(loaded.unwrap_err().contains("assets/body.woff2"));
    let (_dir, loaded) = variant(|d| {
        edit_json(&d.join("tokens.json"), |v| {
            v["font_files"] = serde_json::json!([{ "family": "X", "file": "gone.woff2" }]);
        });
    });
    assert!(loaded.unwrap_err().contains("no assets/gone.woff2"));
}

/// The manifest's icons as `(purpose, type, sizes, src)`.
fn icons(manifest: &str) -> Vec<(String, String, String, String)> {
    let manifest: Value = serde_json::from_str(manifest).unwrap();
    let text = |icon: &Value, key: &str| icon[key].as_str().unwrap_or_default().to_string();
    manifest["icons"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| {
            (
                text(i, "purpose"),
                text(i, "type"),
                text(i, "sizes"),
                text(i, "src"),
            )
        })
        .collect()
}

#[test]
fn the_installed_app_has_measured_png_icons_a_maskable_one_and_a_badge() {
    for site in [builtin(), std::sync::Arc::new(neutral())] {
        let listed = icons(&site.manifest());
        let png = |purpose: &str, sizes: &str| {
            listed
                .iter()
                .any(|(p, kind, s, _)| p == purpose && kind == "image/png" && s == sizes)
        };
        assert!(png("any", "192x192") && png("any", "512x512"), "{listed:?}");
        assert!(png("maskable", "512x512"), "{listed:?}");
        // Each size is what its file measures, and the maskable one is a file of its own.
        for (purpose, _, sizes, src) in &listed {
            let name = src
                .trim_start_matches("/assets/")
                .split('?')
                .next()
                .unwrap();
            let asset = &site.assets[name];
            if let Some((width, height)) = asset.png_size() {
                assert_eq!(sizes, &format!("{width}x{height}"), "{src}");
            }
            assert_eq!(
                purpose == "maskable",
                name == "icon-maskable-512.png",
                "{src}"
            );
        }
        let badge = site.named(site.info.assets.badge.as_ref()).unwrap();
        assert_eq!(badge.png_size(), Some((96, 96)));
        // The splash is the page's own background.
        let manifest: Value = serde_json::from_str(&site.manifest()).unwrap();
        assert!(
            site.stylesheet().contains(&format!(
                "--bg: {};",
                manifest["background_color"].as_str().unwrap()
            )),
            "{manifest}"
        );
    }
}

#[test]
fn an_app_icon_that_is_not_a_square_png_stops_the_start() {
    let edit = |file: &str, bytes: Vec<u8>| {
        let file = file.to_string();
        move |d: &Path| std::fs::write(d.join("assets").join(&file), &bytes).unwrap()
    };
    // A PNG header of 192 by 96.
    let mut wide = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
    wide.extend_from_slice(&192u32.to_be_bytes());
    wide.extend_from_slice(&96u32.to_be_bytes());
    let (_dir, loaded) = variant(edit("icon-192.png", wide));
    let error = loaded.unwrap_err();
    assert!(
        error.contains("assets.app_icons: assets/icon-192.png is 192x96"),
        "{error}"
    );
    let (_dir, loaded) = variant(edit("badge-96.png", b"not a png".to_vec()));
    assert!(loaded.unwrap_err().contains("assets/badge-96.png"));
    let (_dir, loaded) = variant(|d| {
        edit_json(&d.join("site.json"), |v| {
            v["assets"]["maskable_icon"] = "gone.png".into();
        });
    });
    let error = loaded.unwrap_err();
    assert!(
        error.contains("assets.maskable_icon: there is no assets/gone.png"),
        "{error}"
    );
}
