//! Checking a site when it is read: everything a request could render is rendered once,
//! so a bad placeholder, link, section or asset stops the start, not a page later.

use super::{Ctx, Front, Site, assets, discovery, markdown};

impl Site {
    /// Everything a request could render, rendered once now: a placeholder, a link or a
    /// section that does not check out stops the start, not a page later.
    pub(super) fn check(&self) -> Result<(), String> {
        self.info.check()?;
        self.messages.app.catalog.check()?;
        self.landing.check()?;
        self.check_assets()?;
        self.info.landing.check(&self.landing)?;
        let mut errors = Vec::new();
        for front in [
            Front::Unclaimed,
            Front::Passkeys,
            Front::Nomen,
            Front::Hosted,
        ] {
            let ctx = Ctx::checking(self);
            crate::web::landing_for(self, &ctx, front, "https://hub.example.org");
            errors.extend(
                ctx.errors()
                    .into_iter()
                    .map(|e| format!("content/landing.json ({}): {e}", front.key())),
            );
        }
        for (name, page) in &self.pages {
            for front in markdown::DOC_FRONTS {
                for switch in markdown::SWITCHES {
                    let ctx = Ctx::checking(self)
                        .with("mcp", "https://hub.example.org/mcp")
                        .with("updated", page.updated.clone())
                        .with("retention_days", "90");
                    page.render(&ctx, &[front, switch]);
                    page.markdown(&ctx, &[front, switch]);
                    ctx.plain(&page.description);
                    errors.extend(
                        ctx.errors()
                            .into_iter()
                            .map(|e| format!("content/{name}.md: {e}")),
                    );
                }
            }
        }
        for front in markdown::DOC_FRONTS {
            let ctx = discovery::llms_ctx(Ctx::checking(self), "https://hub.example.org");
            self.llms.markdown(&ctx, &[front]);
            errors.extend(
                ctx.errors()
                    .into_iter()
                    .map(|e| format!("content/llms.md: {e}")),
            );
        }
        // Each message with what fills it where it is sent.
        for (what, template, var, value, inline) in [
            (
                "email.subject",
                &self.messages.email.subject,
                "link",
                "https://hub.example.org/auth/email#x",
                false,
            ),
            (
                "email.body",
                &self.messages.email.body,
                "link",
                "https://hub.example.org/auth/email#x",
                false,
            ),
            (
                "welcome.title",
                &self.messages.welcome.title,
                "hub",
                "https://hub.example.org",
                false,
            ),
            (
                "welcome.body",
                &self.messages.welcome.body,
                "hub",
                "https://hub.example.org",
                false,
            ),
            (
                "page_footer",
                &self.messages.page_footer,
                "updated",
                "1 January 2026",
                true,
            ),
        ] {
            let ctx = Ctx::checking(self).with(var, value);
            if inline {
                ctx.inline(template);
            } else {
                ctx.plain(template);
            }
            errors.extend(
                ctx.errors()
                    .into_iter()
                    .map(|e| format!("content/messages.json {what}: {e}")),
            );
        }
        let workflow_checks = self.workflow_checks()?;
        for (what, template, vars, inline) in workflow_checks {
            let mut ctx = Ctx::checking(self);
            for (var, value) in vars {
                ctx.set(var, value);
            }
            if inline {
                ctx.inline(template);
            } else {
                ctx.plain(template);
            }
            errors.extend(ctx.errors().into_iter().map(|e| format!("{what}: {e}")));
        }
        for (what, template, vars, inline) in self.message_checks() {
            let mut ctx = Ctx::checking(self);
            for (var, value) in vars {
                ctx.set(var, value);
            }
            if inline {
                ctx.inline(template);
            } else {
                ctx.plain(template);
            }
            errors.extend(
                ctx.errors()
                    .into_iter()
                    .map(|e| format!("content/messages.json {what}: {e}")),
            );
        }
        if let Some(offers) = self.info.offers() {
            let ctx = Ctx::checking(self);
            for text in [&offers.brand, &offers.question, &offers.instructions] {
                ctx.plain(text);
            }
            let link = &offers.link;
            for text in [
                &link.title,
                &link.text,
                &link.button,
                &link.done,
                &link.gone,
            ] {
                ctx.inline(text);
            }
            errors.extend(
                ctx.errors()
                    .into_iter()
                    .map(|e| format!("site.json offers: {e}")),
            );
        }
        let ctx = discovery::instructions_ctx(Ctx::checking(self), "https://hub.example.org");
        ctx.plain(&self.info.mcp.instructions);
        errors.extend(
            ctx.errors()
                .into_iter()
                .map(|e| format!("site.json mcp.instructions: {e}")),
        );
        errors.dedup();
        match errors.first() {
            None => Ok(()),
            Some(_) if errors.len() == 1 => Err(errors.remove(0)),
            Some(_) => Err(format!("{} (and {} more)", errors[0], errors.len() - 1)),
        }
    }

    fn check_assets(&self) -> Result<(), String> {
        let names = &self.info.assets;
        for (what, name, kinds, inline) in [
            ("logo", &names.logo, &["svg"][..], true),
            ("mascot", &names.mascot, &["svg"][..], true),
            ("diagram", &names.diagram, &["svg"][..], true),
            ("icon", &names.icon, &["svg", "png"][..], false),
            (
                "preview",
                &names.preview,
                &["png", "jpg", "jpeg", "webp"][..],
                false,
            ),
        ] {
            let Some(name) = name else { continue };
            let asset = self
                .assets
                .get(name)
                .ok_or_else(|| format!("site.json assets.{what}: there is no assets/{name}"))?;
            if !kinds.contains(&asset.extension()) {
                return Err(format!(
                    "site.json assets.{what}: assets/{name} must be {}",
                    kinds.join(" or ")
                ));
            }
            if inline {
                assets::check_inline_svg(&asset.text())
                    .map_err(|why| format!("assets/{name} ({what}, drawn inline): {why}"))?;
            }
        }
        self.check_app_icons()
    }
}
