//! The installed app: its manifest (`/manifest.webmanifest`) and its service worker
//! (`/sw.js`), drawn from the site. Android installs the app from the manifest's PNG icons
//! (192 and 512) and crops the separate maskable one to its own shape; each icon's `sizes`
//! is what its file measures, never what a name says.

use serde_json::{Value, json};

use super::{Asset, Site};

impl Site {
    /// The installed app's manifest.
    pub fn manifest(&self) -> String {
        let names = &self.info.assets;
        let entry = |asset: &Asset, purpose: &str| {
            let sizes = match asset.png_size() {
                Some((width, height)) => format!("{width}x{height}"),
                None => "any".to_string(),
            };
            json!({ "src": asset.url(), "sizes": sizes, "type": asset.mime, "purpose": purpose })
        };
        let icons: Vec<Value> = (self.named(names.icon.as_ref()).into_iter())
            .chain(
                names
                    .app_icons
                    .iter()
                    .filter_map(|name| self.assets.get(name)),
            )
            .map(|icon| entry(icon, "any"))
            .chain(
                self.named(names.maskable_icon.as_ref())
                    .map(|icon| entry(icon, "maskable")),
            )
            .collect();
        let manifest = json!({
            "name": self.info.product.name,
            "short_name": self.info.product.short_name.as_deref().unwrap_or(&self.info.product.name),
            "description": self.info.product.description,
            "id": "/app",
            "start_url": "/app",
            "scope": "/",
            "display": "standalone",
            "orientation": "portrait",
            "background_color": self.style.background_color,
            "theme_color": self.style.theme_color,
            "icons": icons,
        });
        serde_json::to_string_pretty(&manifest).unwrap_or_default()
    }

    /// The service worker with this site's name and icon in its notifications.
    pub fn service_worker(&self) -> String {
        let icon = self
            .named(self.info.assets.icon.as_ref())
            .map(Asset::url)
            .unwrap_or_default();
        include_str!("../web/assets/sw.js")
            .replace(
                "@TITLE@",
                &serde_json::to_string(&self.info.product.name).unwrap_or_default(),
            )
            .replace("@ICON@", &serde_json::to_string(&icon).unwrap_or_default())
    }

    /// The installed app's icons are square PNGs, measured: a phone scales a wrong one, or
    /// refuses to install.
    pub(super) fn check_app_icons(&self) -> Result<(), String> {
        let names = &self.info.assets;
        let checks = names
            .app_icons
            .iter()
            .map(|name| ("app_icons", name, 48))
            .chain(
                names
                    .maskable_icon
                    .iter()
                    .map(|name| ("maskable_icon", name, 192)),
            )
            .chain(names.badge.iter().map(|name| ("badge", name, 24)));
        for (what, name, least) in checks {
            let asset = self
                .assets
                .get(name)
                .ok_or_else(|| format!("site.json assets.{what}: there is no assets/{name}"))?;
            asset
                .square_png(least)
                .map_err(|why| format!("site.json assets.{what}: assets/{name} {why}"))?;
        }
        Ok(())
    }
}
