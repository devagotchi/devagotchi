//! `/admin` → Sign-in → Email (hub-settings M3): the mail server sign-in links, password resets
//! and early-access notices go out through. Saved in Postgres (`ops::hub_settings::StoredMail`)
//! with the password sealed under the hub's key, checked before it is saved (connect, TLS, sign
//! in, nothing sent), in force at once. A mail server the config file names (`auth.email`) is
//! shown here, not changed.

use axum::{
    Form,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Response,
};
use serde::Deserialize;

use super::{
    approvals::same_origin,
    email::Mailer,
    hub_settings_owner::owner,
    operator::hidden,
    pages::{self, escape},
    passkeys::HubSignIn,
    routes::{see_other, unavailable},
};
use crate::{
    AppState,
    ops::hub_settings::{MAIL_SEALED_AS, StoredMail},
};

/// Put the mail server saved on `/admin` in force, unless the config file names one. Run when
/// the hub starts and after each save.
pub async fn apply_saved(hub: &HubSignIn) -> devagotchi_contracts::BridgeResult<()> {
    if hub.providers.email.is_from_config() {
        return Ok(());
    }
    let stored = hub.accounts().stored_mail().await?;
    hub.providers
        .email
        .set(stored.filter(|mail| mail.enabled).and_then(|mail| {
            mailer_of(&mail, hub)
                .map_err(|why| eprintln!("devagotchi hub: the saved mail server is off: {why}"))
                .ok()
        }));
    Ok(())
}

fn mailer_of(mail: &StoredMail, hub: &HubSignIn) -> Result<Mailer, String> {
    let password = hub
        .sealer()
        .open_value(MAIL_SEALED_AS, &mail.sealed_password);
    Mailer::from_parts(
        &mail.smtp_url,
        password,
        &mail.from,
        mail.per_address_per_hour,
    )
}

/// The card, with what is set and how the last send went.
pub(super) fn card(hub: &HubSignIn, stored: Option<&StoredMail>) -> String {
    let now = hub.providers.email.get();
    let status = match (hub.providers.email.is_from_config(), &now, stored) {
        (true, _, _) => "<span class=\"tag kind\">On, from this hub's configuration</span>",
        (false, Some(_), _) => "<span class=\"tag kind\">On</span>",
        (false, None, Some(_)) => "<span class=\"tag\">Off</span>",
        (false, None, None) => "<span class=\"tag\">Not set up</span>",
    };
    let live = now
        .as_ref()
        .map(|mailer| {
            let last = mailer.status();
            let last = match last.get("last_send") {
                Some(serde_json::Value::Object(send)) if send["ok"] == true => {
                    format!("sent {}", send["at"].as_str().unwrap_or(""))
                }
                Some(serde_json::Value::Object(send)) => format!(
                    "refused {}: {}",
                    send["at"].as_str().unwrap_or(""),
                    send["error"].as_str().unwrap_or("")
                ),
                _ => "nothing sent yet".into(),
            };
            format!(
                "<ul class=\"rows\"><li><strong>Mail server</strong> <span class=\"meta\">{}</span></li>\
<li><strong>Last send</strong> <span class=\"meta\">{}</span></li></ul>",
                escape(&mailer.describe()),
                escape(&last)
            )
        })
        .unwrap_or_default();
    let body = if hub.providers.email.is_from_config() {
        "<p class=\"note help\">Set by this hub's configuration (<code>auth.email</code>). Change it there, or remove it there to set it here. <code>devagotchi hub check-mail --to you@example.com</code> on the hub's host sends a test.</p>".to_string()
    } else {
        let url = stored.map(|s| s.smtp_url.as_str()).unwrap_or_default();
        let from = stored.map(|s| s.from.as_str()).unwrap_or_default();
        let per = stored.map(|s| s.per_address_per_hour).unwrap_or(5);
        let hint = if stored.is_some_and(|s| !s.sealed_password.is_empty()) {
            "Saved. Leave empty to keep it."
        } else {
            "The server's password or API key"
        };
        let enabled = stored.map(|s| s.enabled).unwrap_or(true);
        format!(
            "<ol class=\"connect-steps\">\
<li>Use any mail server that takes a login: your own, or a provider's. Its address is <code>smtps://user@host:465</code> (TLS from the start) or <code>smtp://user@host:587</code> (STARTTLS); the password goes in its own field, never in the address.</li>\
<li>Resend: <code>smtps://resend@smtp.resend.com:465</code>, the password is a Resend API key, and the sender is an address on a domain you verified there. Others (Postmark, SendGrid, Mailgun, Amazon SES, Gmail with an app password) give their own host and login in the same shape.</li>\
<li>Check and save: the hub connects, starts TLS and signs in, sending nothing, and saves only if that works. Give an address under Send a test to also get one email.</li></ol>\
<form class=\"prefs\" method=\"post\" action=\"/admin/mail\">\
<label>Mail server<input type=\"text\" name=\"smtp_url\" autocomplete=\"off\" spellcheck=\"false\" value=\"{url}\" placeholder=\"smtps://user@smtp.example.com:465\" required maxlength=\"512\"></label>\
<label>Password or API key<input type=\"password\" name=\"password\" autocomplete=\"new-password\" placeholder=\"{hint}\" maxlength=\"1024\"></label>\
<label>Send as<input type=\"text\" name=\"from\" autocomplete=\"off\" value=\"{from}\" placeholder=\"Hub &lt;hub@example.com&gt;\" required maxlength=\"320\"></label>\
<label>Emails to one address per hour<input type=\"number\" name=\"per_address_per_hour\" min=\"1\" max=\"100\" step=\"1\" value=\"{per}\" required></label>\
<label class=\"check\"><input type=\"checkbox\" name=\"enabled\" value=\"on\"{checked}><span>Send email through this server</span></label>\
<label>Send a test to<input type=\"email\" name=\"test_to\" autocomplete=\"email\" maxlength=\"254\" placeholder=\"Optional: you@example.com\"></label>\
<p class=\"actions\"><button class=\"approve\" type=\"submit\">Check and save</button></p></form>",
            url = escape(url),
            hint = hint,
            from = escape(from),
            checked = if enabled { " checked" } else { "" },
        )
    };
    format!(
        "<details class=\"connector\" id=\"mail\"{open}><summary>Email {status}</summary>\
<p class=\"note help\">Sign-in links, password resets and early-access notices go out through it. The password is encrypted in this hub's Postgres.</p>{live}{body}</details>",
        open = if now.is_none() { " open" } else { "" },
    )
}

#[derive(Debug, Deserialize)]
pub(super) struct MailForm {
    #[serde(default)]
    smtp_url: String,
    #[serde(default)]
    password: String,
    #[serde(default)]
    from: String,
    #[serde(default)]
    per_address_per_hour: String,
    #[serde(default)]
    enabled: Option<String>,
    #[serde(default)]
    test_to: String,
}

/// The form's values, checked; else why not, for the page.
fn read(form: &MailForm) -> Result<(String, String, u32), String> {
    let url = form.smtp_url.trim().to_string();
    let from = form.from.trim().to_string();
    let bad = |text: &str, max: usize| text.len() > max || text.chars().any(char::is_control);
    if bad(&url, 512) {
        return Err(
            "Give the mail server as smtps://user@host:465 or smtp://user@host:587.".into(),
        );
    }
    crate::notify::mail::Server::parse(&url).map_err(|why| format!("The mail server: {why}."))?;
    if from.is_empty() || bad(&from, 320) || !from.contains('@') {
        return Err("Send as is an address, or Name <address>.".into());
    }
    if bad(form.password.trim(), 1024) {
        return Err("Check the password.".into());
    }
    let per = form
        .per_address_per_hour
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|n| (1..=100).contains(n))
        .ok_or("Emails to one address per hour is 1 to 100.")?;
    Ok((url, from, per))
}

pub async fn save(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<MailForm>,
) -> Response {
    let Some(person) = owner(&state, &headers).await else {
        return hidden();
    };
    let Some(hub) = &state.passkeys else {
        return hidden();
    };
    let refused = |status: StatusCode, why: &str| {
        pages::message(
            status,
            "Not saved",
            why,
            Some(("/admin#mail", "Hub settings")),
        )
    };
    if !same_origin(&headers, &state.hub.config.public_url) {
        return refused(
            StatusCode::FORBIDDEN,
            "Change this from the hub's own settings page.",
        );
    }
    if hub.providers.email.is_from_config() {
        return refused(
            StatusCode::CONFLICT,
            "This hub's configuration sets the mail server (auth.email). Change it there, or remove it there to set it here.",
        );
    }
    let (url, from, per) = match read(&form) {
        Ok(values) => values,
        Err(why) => return refused(StatusCode::BAD_REQUEST, &why),
    };
    let accounts = hub.accounts();
    let current = match accounts.stored_mail().await {
        Ok(current) => current,
        Err(error) => {
            eprintln!("devagotchi hub: mail settings: {error}");
            return unavailable();
        }
    };
    // An empty password keeps the saved one, for the same server and login.
    let password = form.password.trim();
    let sealed = if password.is_empty() {
        current
            .filter(|c| c.smtp_url == url)
            .map(|c| c.sealed_password)
            .unwrap_or_default()
    } else {
        hub.sealer().seal(MAIL_SEALED_AS, password)
    };
    let mail = StoredMail {
        smtp_url: url,
        from,
        per_address_per_hour: per,
        sealed_password: sealed,
        enabled: form.enabled.is_some(),
    };
    let mut flash = "Email".to_string();
    if mail.enabled {
        let mailer = match mailer_of(&mail, hub) {
            Ok(mailer) => mailer,
            Err(why) => return refused(StatusCode::BAD_REQUEST, &why),
        };
        if let Err(why) = mailer.check().await {
            return refused(
                StatusCode::BAD_REQUEST,
                &format!("The mail server did not take it, so nothing was saved: {why}"),
            );
        }
        let to = form.test_to.trim();
        if !to.is_empty() {
            if crate::identity::accounts::email::normalize_email(to).is_none() {
                return refused(StatusCode::BAD_REQUEST, "Send a test to is one address.");
            }
            match mailer
                .send_check(&hub.site(), to, &state.hub.config.public_url)
                .await
            {
                Ok(()) => flash = format!("Email; a test went to {to}"),
                Err(why) => {
                    return refused(
                        StatusCode::BAD_REQUEST,
                        &format!(
                            "Signed in, but the test was refused, so nothing was saved: {why}"
                        ),
                    );
                }
            }
        }
    }
    if let Err(error) = accounts.save_mail(&person.id, &mail).await {
        eprintln!("devagotchi hub: saving the mail server: {error}");
        return unavailable();
    }
    if let Err(error) = apply_saved(hub).await {
        eprintln!("devagotchi hub: applying the mail server: {error}");
        return unavailable();
    }
    see_other(&format!("/admin?saved={}#mail", super::encode(&flash)), &[])
}
