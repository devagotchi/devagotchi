# Identity, sign-in and configuration

How the hub knows who someone is, how they sign in, and how the hub is configured. Follow it for
every new setting, every new sign-in provider, and every change to sessions, cookies, the origin
check or the hub's own OAuth server.

It covers `engine/hub/src/config/`, `identity/`, `login/`, `ops/hub_settings.rs`, and the web
files for sign-in and `/admin`: `web/{providers,provider_apply,sign_in,sign_in_cards,passkeys,
session,sessions,oauth,device,email,claim,nomen_login,hosted,hub_settings*}.rs`, the workflow
cookies in `web/workflow.rs` and the origin check in `web/approvals.rs`.

It extends [layout](layout.md) and applies [connectors](connectors.md) rule 3 (no code path per
client) to sign-in: **a difference between providers is data, not a code path.** The shared
rules G1 to G14 in [the README](README.md) hold here and are not repeated. Each rule below says
only what is particular to this area.

## Purpose

A self-hosted hub has one owner. A hosted hub has one tenant per person. People sign in to the
hub's own accounts (`auth.mode hub`). AI clients sign in through the hub's OAuth 2.1 server, and
terminals through the device grant. The owner configures the hub in one of two places:

- at deploy, in the file or the environment, only for the keys listed in `DEPLOY_TIME`;
- on `/admin`, stored in Postgres and live.

The objective is the hub-settings plan's: a person running a hub never edits a file or an
environment variable to configure it (`.ai/dev/plans/hub-settings.md`, Objective).

## Exemplar

All ranges below were re-read at main 2b3feec.

**Adding a live setting: the live settings catalog** (hub-settings M9).

- `engine/hub/src/config/live/catalog.rs`:
  - `Setting` (:80-91) and `SETTINGS` (:98-179): one row per setting with key, group, label,
    help, kind with range and unit, and env.
  - `DEPLOY_TIME` (:183-254): every other key, with why.
  - `SPLIT_SECTIONS` (:259-265) and `PAGE_ONLY` (:269).
  - `Setting::find` and `Setting::parse` (:288-308).
- `engine/hub/src/config/live.rs`:
  - `LiveSettings` (:30-131) with `get`, `set` and `with_saved`. A pinned key is skipped (:119).
    A saved value out of range is logged and left out (:122-127).
  - `Pinned::from` (:136-163): what the file or the environment set.
  - `Live` (:168-200): an `Arc<RwLock>`, read when the decision is made.
- `engine/hub/src/web/hub_settings_live.rs`: `card` (:49-104) disables pinned fields and says
  where to change them; `read_form` (:125-147) takes all of a card or none and gives every
  reason; `save` (:149-206).
- `engine/hub/src/ops/hub_settings.rs`: `save_hub_settings` (:33-69) writes the rows and one
  `settings.changed` event per key on the home tenant, in one transaction. `apply_saved`
  (:159-164) is the same function at start (`serve/mod.rs:89`) and after a save.

Why it is the best: one table drives the page, validation, persistence, precedence and the
deploy-time map, and a test fails on a key in neither list.

**Adding a config section: `engine/hub/src/config/analytics.rs`.** Module doc with a jsonc
example (:5-10); `pub const ANALYTICS_ENV` and `RETENTION_ENV` (:20-22);
`#[serde(deny_unknown_fields, default)]` plus `Default` (:24-39); `check()` names the dotted key
and range (:42-47); `from_env` takes the injected env and treats blank as unset (:50-65); an
inline `mod tests` with an env closure (:68-112), allowed by G10 in a leaf file under the cap.
It is wired in `HubConfig::parse_with` (`config/mod.rs:209`) and `HubConfig::validate`
(`config/mod.rs:249`).

**Adding a sign-in provider: the OpenID Connect path.**

- `config/providers.rs`: `ProviderKind`, `ProviderConfig` and `ProviderConfig::check(env)`
  (:30-183).
- `web/providers.rs`:
  - `Providers::endpoints` (:227-280): discovery, cached in a `OnceCell`. The issuer must equal
    discovery's, a trailing `/` aside (:261-263). Client auth comes from
    `token_endpoint_auth_methods_supported`, basic when absent (:264-269).
  - `start` (:377-439): PKCE S256, `state` and `nonce`, sealed into `__Host-dg_signin`.
  - `callback` (:454-518): the cookie's MAC and age, then provider name and `state` (:492).
  - `prove` (:521-569) and `id_token_claims` (:573-621): `iss` (a trailing `/` aside), `aud`,
    `exp`, `nonce`; `sub` is the identity, and the email is kept only when verified.
- `web/sign_in_cards.rs`: `Guide` (:13-29), `GUIDES` (:31-96) and `OTHER` (:99-114), guidance
  as data.
- `identity/accounts/sign_in_settings.rs`: one row per name, and a `sign_in.configured` event
  without the secret (:97-104).

Why it is the best: any OIDC provider is a config entry or an `/admin` card with no code. Its
checks map onto OIDC Core §3.1.3.7 and Discovery §4.3.

**Saving a record that holds a secret: `web/hub_settings_mail.rs`.** `apply_saved` (:31-44) is
the same function at start (`serve/mod.rs:86`, as `web::apply_saved_mail`) and after a save.
`save` (:171-273) refuses a file-set record with 409 (:196-201), keeps the sealed password on an
empty field (:214-223), and checks against the live server before saving (:232-262).

## Where files go

| What | Today | Target |
|---|---|---|
| Config types, parse, checks, env overrides | `config/<section>.rs`, `config/mod.rs` | same |
| Live-settings catalog | `config/live.rs` + `config/live/catalog.rs` | `config/live/mod.rs` + `catalog.rs` (layout rule 2) |
| Live-settings rows and apply | `ops/hub_settings.rs` | `settings/stored.rs` with S8 (`hub_` is the settings domain, layout rule 8) |
| `/admin` page and cards | `web/hub_settings*.rs` | `settings/web/` with S8 |
| Accounts, identities, sessions, passkeys, OAuth store | `identity/accounts/<x>/` | same |
| Sign-in pages and flows | `web/{providers,provider_apply,sign_in,sign_in_cards,passkeys,session,email,claim,device,oauth}.rs` | `identity/web/` with S8 |
| Cookie names and `Set-Cookie` | spread over 5 files (see K1) | `web/cookies.rs`, then `identity/web/cookies.rs` with S8 |
| `devagotchi login` (client) | `login/` | same |
| Tests | config: inline `mod tests`, `config/tests/`, `config/live/tests.rs`; identity: `identity/tests/`, `<domain>/tests.rs` | as G10; `web/*_tests.rs` move once, with S8 |

S8 waits on the operator's ruling (`.ai/dev/plans/structure-cleanup.md`, S8). Nothing moves to
an interim place before it.

## Part A: configuration

### Section shape (from `config/analytics.rs`)

```rust
//! `<section>` in the hub's config: what it decides.
//! ```jsonc
//! "<section>": { "<key>": 90 }   // range, meaning
//! ```
//! Environment overrides: `DEVAGOTCHI_HUB_<X>`.
use serde::Deserialize;

pub const X_ENV: &str = "DEVAGOTCHI_HUB_<X>";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct SectionConfig { pub key: u32 }

impl Default for SectionConfig { fn default() -> Self { Self { key: 90 } } }

impl SectionConfig {
    pub(crate) fn check(&self) -> Result<(), String> { /* "<section>.<key> is 1 to 3650" */ }
    pub(crate) fn from_env(mut self, env: &dyn Fn(&str) -> Option<String>) -> Result<Self, String> {
        /* blank = unset; a bad value names the variable and what is allowed */
    }
}

#[cfg(test)]
mod tests { /* builds an env closure; never touches the process environment */ }
```

Wiring: a field on `HubConfig` with `#[serde(default)]`; `from_env` in `HubConfig::parse_with`;
`check` in `HubConfig::validate`.

### Rules

- **C1. The entry points that read the environment** (G8). Only these call `std::env::var`:
  - `config/file.rs`, which hands the closure to `HubConfig::parse_with`;
  - `serve/mod.rs` (`run`), which builds one closure, once, and passes it to everything it
    constructs;
  - `lib.rs` `board_cli`, for the board CLI's `Invocation`;
  - `login/client.rs`, for `DEVAGOTCHI_HUB`;
  - `ops/audit.rs`, for the OS user;
  - `cli/mod.rs`, for `Command::CheckMail`, once `Mailer::from_config` takes `env` (see Known
    violations, C1).

  Everything else takes the env closure or the value. The closure is
  `env: &dyn Fn(&str) -> Option<String>`; a constructor that takes
  `impl Fn(&str) -> Option<String>` also accepts it. Precedents: `HubConfig::parse_with`,
  `ProviderConfig::check(env)`, `EmailConfig::check(env)`, `notify::signals::Sink::new(.., env)`.
  Tests pass a closure or a 0600 file and never call `set_var`.
- **C2. Fail closed at start.** Unknown keys (G2), out-of-range values and missing secret
  variables stop the start with `invalid_config`. Each message names the dotted key and what is
  allowed: `"analytics.retention_days is 1 to 3650"`.
- **C3. Secrets in config** (G8). A secret is a `*_env` variable name or an absolute `*_file`
  path read with `identity::auth::read_private` (0600). URLs carry no password (`storage.url`,
  `clickhouse`, `auth.email.smtp_url`). Client IDs may sit in the file. A secret saved on
  `/admin` is sealed with `Sealer::seal(name, secret)`.
- **C4. Every config key is in exactly one list:** a row of `SETTINGS`, or a line of
  `DEPLOY_TIME` that says why. The `hub_settings` table holds only `SETTINGS` keys. A record
  with several fields gets its own table.
- **C5. The file wins.** A value the file or env set is pinned: the page shows it, disables it
  and says where to change it.
  - A save of a pinned scalar is skipped (`read_form`).
  - A save of a file-set record is refused with 409 (mail, ClickHouse, sinks).
  - A new name on `/admin` that the file already uses is refused with 409.
  - One exception: for the four guided sign-in names (`GUIDES`), a saved row replaces the
    file's provider (`web/provider_apply.rs` `overlay`), so the Shippin switch can turn off what
    `deploy/compose/install.sh --nomen` wrote. The exception must be written in the
    hub-settings plan and in `DEPLOY_TIME`'s `auth` line; today it is in neither (Known
    violations, C5). It never applies to a name added with "Add another".
- **C6. Live values are read when the decision is made**, through `config.live()` (the values
  at that moment) or a `Live` from `config.live_handle()`, never copied at start. A save reaches
  every holder at once.
- **C7. A setting's range in `SETTINGS` equals its section's `check()` range.** A test holds
  them equal (Mechanical checks, 1).
- **C8. A deploy-time key states why** it cannot be live: it needs a restart, decides who owns
  the hub, holds the store the settings live in, or is a deploy secret.

### Recipe: add a live setting

1. Add the field to its section, with its `Default` and its `check()` range.
2. Add a `Setting` row to `SETTINGS` with the same dotted key and the same range, plus `env` if
   a variable pins it.
3. Add the field to `LiveSettings`, `from_sections`, `get` and `set`.
4. If the section was deploy-time as a whole, add it to `SPLIT_SECTIONS` and move its other
   keys to `DEPLOY_TIME`. A page-only setting goes in `PAGE_ONLY`.
5. Read it with `config.live().<field>` at the decision.
6. Run `cargo test -p devagotchi-hub config::live`, then extend
   `web/hub_settings_live_tests.rs` with its effect once saved.

No new route, form or migration is needed: the card, the `hub_settings` row and the
`settings.changed` event follow.

### Recipe: add a record that holds a secret (mail, ClickHouse, a sink, a provider)

The save handler runs in this order:

1. Origin (K5). The layer answers a cross-site POST with 403 before the handler runs.
2. `owner()`, else `hidden()` (404).
3. Set by the file: 409.
4. Validate every field with the same checks the config file gets. Sinks reuse
   `SignalsConfig::check`; OIDC reuses `ProviderConfig::check`. On any error, return 400 and
   save nothing.
5. An empty secret field keeps the sealed one. The page never echoes a secret.
6. When the service has a side-effect-free check (SMTP login, a ClickHouse query, OIDC
   discovery), run it before saving, and return 400 when it fails. Sinks have none.
7. `Sealer::seal(<name>, secret)`.
8. Save in one transaction, with an event on the home tenant that names the setting and never
   the secret.
9. `apply_saved()`, the same function start runs.
10. 303 to `/admin?saved=..#<card>`.

A store failure at any step is 503 (G4). The table carries its domain's prefix (layout rule 8).

## Part B: sign-in providers

### Shape

```jsonc
"auth": { "mode": "hub", "providers": [
  { "kind": "oidc", "name": "okta", "label": "Okta", "issuer": "https://example.okta.com",
    "client_id_env": "DEVAGOTCHI_OKTA_CLIENT_ID", "client_secret_env": "DEVAGOTCHI_OKTA_CLIENT_SECRET" } ] }
```

Or use `/admin` → Sign-in → "Add another OpenID Connect provider".

The flow (`web/providers.rs`):

1. `GET /auth/provider/<name>` (`start`). Discovery is fetched once and cached, and its
   `issuer` must equal the configured one (a trailing `/` aside); when it does not answer or
   does not match, the page is 503 "That sign-in is unavailable". `Started{p,s,v,n,r,l,t}` is
   MAC'd with the purpose `"provider sign-in"` into `__Host-dg_signin` for 600 s. The redirect
   carries `response_type=code`, PKCE S256, `state`, and `nonce` for every kind but GitHub.
2. `GET /auth/provider/<name>/callback` (`callback`) checks:
   - an unknown name is 404; then the cookie's MAC and age; then, in one check, that the
     provider name and `state` match;
   - the code exchange: no redirects followed, the answer capped at 256 KiB. The secret goes by
     HTTP basic auth when discovery lists `client_secret_basic` or lists no methods, and in the
     form otherwise; GitHub always takes it in the form;
   - the ID token: `iss` (equal to the issuer, a trailing `/` aside), `aud` (contains
     client_id), `exp` (60 s leeway) and `nonce`.

   The result is `Proven{provider, subject = sub, verified_email}`.
3. `finish()` links or signs in, then `session::open_and_return()`.

### Rules

- **P1. OIDC is data.** Any OpenID Connect provider is a config entry or an `/admin` card. A new
  `ProviderKind` is only for a protocol with no ID token (GitHub). It adds one arm to
  `endpoints()` and one to `prove()`, plus its default name and label.
- **P2. The name is the key:** 1 to 32 characters of `[a-z0-9-]`, unique across the file and
  `/admin`, and never `email`. It forms the callback path, the identity `<name>:<subject>` and
  the `operators` entry. For OIDC the mix-up defence is the ID token `iss` (RFC 9700 §4.4.2.1).
  For GitHub it is the distinct callback (§4.4.2.2).
- **P3. No provider name in flow code.** `web/` and `identity/` never compare a provider with a
  literal. A difference is a `ProviderConfig` field, a `Guide` field (`notice`, `steps`,
  `callback_label`) or site words. One function answers each question (G1): the offered
  providers, in order, with their labels, come from one place.
- **P4. Identity is `(provider, subject)`.** An email counts only when the provider verified it
  (`email_verified`, or GitHub's primary verified address).
- **P5. Linking by email proves the existing account first,** for every provider. The proof is
  the account's password, a passkey, or another sign-in already linked to it. There is no silent
  link by email.
- **P6. An ID token's signature may be left to TLS** only because it came straight from the
  token endpoint (OIDC Core §3.1.3.7 item 6).
- **P7. Guidance is a `Guide` row.** It never prefills an issuer (hub-settings M6).
- **P8. An `/admin` save checks like the config file and contacts discovery before saving,** so
  a typo fails on the page and not at someone's sign-in.

### Recipe

- **An OIDC provider: no code.** For a guided card, add a `Guide` to `GUIDES` and extend
  `web/hub_settings_tests.rs`.
- **A protocol with no ID token:**
  1. Add a `ProviderKind` variant with its defaults and `check()` constraints.
  2. Add its arms in `endpoints()` and `prove()`, and its `Guide`.
  3. Add stub routes to `identity/tests/providers.rs`.
  4. Add a round-trip test plus refusal tests: wrong audience, no cookie, swapped state, forged
     MAC, unknown name, wrong issuer, expired token, wrong nonce, and a discovery issuer
     mismatch.

## Part C: sessions, cookies and origin

- **K1. One cookie helper.** Every cookie is `__Host-dg_<x>` on https and `dg_<x>` only on
  loopback http. One helper builds it: `Path=/; HttpOnly; SameSite=Lax; Max-Age=..`, plus
  `Secure` on https.
- **K2. One seal.** State in a cookie is MAC'd by one `seal(purpose, value)` /
  `unseal(purpose, cookie, ttl)` pair under the account store's key, bound to a purpose and an
  issue time.
- **K3. Session tokens.** A session token is 32 random bytes, and only its SHA-256 is stored.
  Sessions end at `SESSION_IDLE_SECS` unused or `SESSION_MAX_SECS`, with at most
  `MAX_SESSIONS_PER_ACCOUNT` (50) per account (`identity/accounts/sessions/mod.rs`).
- **K4. A new session on every privilege change:** a different account signs in, or an
  unsecured account becomes secured (OWASP Session Management).
- **K5. One origin check, strict.** `web::from_this_hub` is the only function that reads
  `Sec-Fetch-Site` and `Origin`:
  - when `Sec-Fetch-Site` is present, it must be `same-origin`;
  - otherwise `Origin` must equal the hub's origin;
  - with neither, reject (OWASP CSRF).

  It runs as one layer before every handler, with an `EXEMPT` list of the cookie-less routes
  ([hub-pages](hub-pages.md) rule 2). `HubSignIn::from_own_page` calls it. Non-browser callers
  use bearer routes. A cross-site POST to an owner route gets 403; a GET still hides the route
  with 404.

## Part D: the hub's OAuth server for AI clients

`web/oauth.rs` and `identity/accounts/oauth/` implement the MCP authorization profile:

- RFC 8414 and RFC 9728 metadata;
- RFC 7591 registration of public clients only, with PKCE S256;
- codes that are single-use and bound to client, redirect, verifier and resource (RFC 8707);
- opaque `dgh_` and `dgr_` tokens, stored as SHA-256;
- rotation that ends the whole family on reuse;
- RFC 7009 revocation and the RFC 8628 device grant.

Lifetimes come from `auth.tokens`, which is deploy-time. Any change keeps every MUST of the MCP
authorization spec and adds a test in `identity/accounts/oauth/tests.rs`.

## The canonical tests

| Subject | Canonical test |
|---|---|
| A config section | `config/analytics.rs` inline `mod tests` (:68-112), with an env closure |
| Catalog completeness | `config/live/tests.rs` `every_config_key_has_a_page_or_is_listed_as_set_at_deploy` (:59-98) |
| Ranges, get and set | `config/live/tests.rs` `a_setting_takes_only_its_own_kind_inside_its_range` (:100-133) |
| Precedence | `config/live/tests.rs` `saved_values_apply_over_the_file_unless_the_file_or_environment_pinned_them` (:135-173) |
| A live setting over HTTP | `web/hub_settings_live_tests.rs` `settings_saved_on_the_page_apply_at_once_are_audited_and_leave_what_the_file_set` (:28-186) |
| A secret-bearing record | `web/hub_settings_mail_tests.rs` `a_mail_server_saved_on_the_page_is_checked_sealed_and_sends_at_once` (:27-158) |
| A provider round trip | `identity/tests/providers.rs`: `stub_server` on `127.0.0.1:0` (:157), `world()` with secrets in 0600 files (:184-220), `World::sign_in` replaying the 303s (:229) |
| A provider set on `/admin` | `web/hub_settings_tests.rs` `run` (:26-250), called by `a_provider_saved_on_the_settings_page_is_sealed_and_offered_at_once` (:252-255) |

Store fixtures and skips follow G13. Every refusal test asserts the status and that nothing was
saved, sealed, linked or signed in.

## Anti-patterns

- `std::env::var` in a constructor or handler: a second config door that tests cannot inject.
- `if provider == "google"`: a provider code path.
- An `/admin` save that skips the file's checks, or saves before the live check.
- A `hub_settings` row whose key is not a `SETTINGS` key.
- A page value that silently beats the file's.
- A cookie without a `__Host-` twin, or a hand-built `Set-Cookie`.
- A copy-pasted MAC seal or unseal.
- A second origin helper, or one that accepts a POST with neither `Origin` nor
  `Sec-Fetch-Site`.
- Linking by email without proving the existing account.
- A range in `SETTINGS` and in `check()` with nothing holding them equal.

## Mechanical checks

Two checks exist. `every_config_key_has_a_page_or_is_listed_as_set_at_deploy`
(`config/live/tests.rs`) holds C4. `a_setting_takes_only_its_own_kind_inside_its_range` catches
a setting missing from `get` or `set`. The checks below are to be written. None has been
compiled or run. Each lands green together with the fix it guards (G12).

1. **C7**, in `engine/hub/src/config/live/tests.rs`. It is pure (no Postgres). It parses a base
   config with each number setting at min-1, min, max and max+1:

   ```rust
   #[test]
   fn a_number_settings_range_is_the_config_files_range() {
       let base = serde_json::json!({"listen": "127.0.0.1:8787", "public_url": "https://app.devagotchi.com",
           "auth": {"mode": "hub"}, "brokers": [], "grants": []});
       for setting in SETTINGS {
           let Kind::Number { min, max, .. } = setting.kind else { continue };
           if PAGE_ONLY.contains(&setting.key) { continue }
           let parts: Vec<&str> = setting.key.split('.').collect();
           for (n, ok) in [(min - 1, false), (min, true), (max, true), (max + 1, false)] {
               let mut raw = base.clone();
               let mut at = &mut raw;
               for part in &parts[..parts.len() - 1] {
                   at = at.as_object_mut().unwrap().entry(*part).or_insert(serde_json::json!({}));
               }
               at[parts[parts.len() - 1]] = n.into();
               assert_eq!(crate::config::HubConfig::parse(&raw.to_string()).is_ok(), ok,
                   "{} = {n}: the file and /admin must take the same range", setting.key);
           }
       }
   }
   ```

   At min-1 = -1 a `u32` field fails to deserialize, which correctly counts as refused.

2. **Source scans** in a new `engine/hub/src/identity/tests/conventions.rs`, declared from
   `identity/tests/mod.rs`. It walks `engine/hub/src/**/*.rs` with the walker lifted from
   `site/brand_tests.rs` (G12), and skips `tests.rs`, `*_tests.rs` and `tests/`.
   - `every_cookie_has_a_host_prefixed_twin` (K1): every literal matching `"(dg|dvg)_[a-z_]+"`
     needs a `"__Host-<same>"` literal in the same file. Today it matches the 9 cookie literals
     and fails only on `web/sign_in.rs` (`FRESH`). Once `web/cookies.rs` lands, tighten it: only
     `cookies.rs` contains `Path=/; HttpOnly`.
   - `no_provider_name_in_flow_code` (P3): scans `web/providers.rs`, `web/sign_in.rs`,
     `web/session.rs`, `web/passkeys.rs` and `identity/accounts/identities.rs` for
     `== "(google|github|nomen|oidc)"` or `"(google|github|nomen|oidc)" ==`. Defaults
     (`config/providers.rs`), `GUIDES` (`web/sign_in_cards.rs`) and `OPERATOR_PROVIDERS`
     (`config/tenancy.rs`) are outside the scanned set. It fails today on `Providers::buttons`
     and `finish` in `web/providers.rs`.
   - `the_environment_is_read_only_at_entry_points` (C1): no `env::var(` outside the C1 list,
     each entry with its reason, and exactly one occurrence in `serve/mod.rs`. It fails today on
     `web/providers.rs`, `web/email.rs`, `offers/nomen.rs`, `agenthub/issue_lifecycle/mod.rs`,
     and on the two occurrences in `serve/mod.rs`.
3. **K5** will be held by hub-pages' `engine/hub/src/web/tests/forms.rs`, which does not exist
   yet and lands with the K5 fix: every non-exempt POST answers 403 to a cross-site request and
   to one with neither header, and only one function reads both headers. Pair it with an HTTP
   test that a POST to `/admin/settings/passwords` with no `Origin` and no `Sec-Fetch-Site`
   gets 403.
4. **C4 over HTTP**, optional: `every_hub_settings_row_key_is_a_setting` (Postgres, G13). After
   saving every `/admin` card, every `hub_settings` key passes `Setting::find`. It fails today on
   the five `mail.*` rows.

## Upstream sources

| Source | What it settles here |
|---|---|
| https://openid.net/specs/openid-connect-core-1_0.html#IDTokenValidation | §3.1.3.7 items 2 (`iss` exact), 3 (`aud` contains client_id), 9 (`exp`), 11 (`nonce`). Item 6 lets a token taken straight from the token endpoint over TLS skip the signature check (P6, `id_token_claims`). |
| https://openid.net/specs/openid-connect-discovery-1_0.html | §3: `token_endpoint_auth_methods_supported` defaults to `client_secret_basic`. §4.3: the returned `issuer` must be identical to the Issuer URL (`Providers::endpoints`). |
| https://developers.google.com/identity/openid-connect/openid-connect | Use `sub`, not the email, as the identity; `email_verified` marks a verified address (P4, `Proven`). |
| https://www.rfc-editor.org/rfc/rfc9700.html | §2.1.1 PKCE; §4.4.2 mix-up: issuer identification (§4.4.2.1) is the primary defence, distinct redirect URIs (§4.4.2.2) the weaker one, used only for GitHub, which has no `iss` (P2). |
| https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps | GitHub supports PKCE S256 only, returns JSON given `Accept: application/json`, and issues no ID token. This is why `github` is the only non-OIDC `ProviderKind` (P1). |
| https://modelcontextprotocol.io/specification/2025-06-18/basic/authorization | RFC 8414 metadata, RFC 9728 and `WWW-Authenticate` on 401, DCR, PKCE, RFC 8707 audience, refresh rotation for public clients, localhost or HTTPS redirects (Part D). Public-only registration is a narrowing the spec allows. |
| https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Set-Cookie | A `__Host-` cookie is `Secure`, set from https, with no `Domain` and `Path=/` (K1). |
| https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html | Renew the session ID after any privilege level change (K4). |
| https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html | Reject when `Sec-Fetch-Site` is cross-site; fall back to `Origin`; reject when neither header establishes the origin (K5). |
| https://auth0.com/docs/manage-users/user-accounts/user-account-linking | Authenticate both accounts before an automatic link (P5). |
| https://raw.githubusercontent.com/tokio-rs/axum/main/examples/oauth/src/main.rs | The axum example keeps CSRF state in a server-side store and uses no PKCE. The hub departs on purpose: a stateless MAC'd cookie (nothing stored before the callback, any instance can finish) plus PKCE. |
| https://12factor.net/config | 12-factor keeps config in environment variables. The hub keeps a typed JSON file with env overrides, because nested lists (`auth.providers`, `signals.sinks`) do not fit flat variables. Secrets still come only from env or a 0600 file (C3). The live `/admin` tier comes from the hub-settings plan, not from 12-factor. |
| https://www.w3.org/TR/webauthn-3/ | §5.2.1.1: `getPublicKey` and `getAuthenticatorData` let the relying party skip CBOR (`identity/accounts/passkey/mod.rs`). |

## Known violations

Re-verified at main 2b3feec. Line numbers are given where they help find the spot.

### C1: the environment is read at an entry point

- `engine/hub/src/serve/mod.rs` (`run`): reads the ClickHouse password with `std::env::var`
  (:192) and builds a second `env::var` closure for `Sink::new` (:224) -> build
  `let env = |name: &str| std::env::var(name).ok();` once, use `env(CLICKHOUSE_PASSWORD_ENV)`,
  and pass `&env` to `Providers::from_config` (which hands it to `Mailer::from_config`),
  `Offers::from_config`, `IssueLifecycleStore::from_config` and every `Sink::new`.
- `engine/hub/src/web/providers.rs` (`Providers::from_config`, :133, :139): reads the client
  secret and client id variables itself, while `ProviderConfig::check` read them through the
  injected env, so the two reads can disagree -> take `env` and use `env(v).ok_or_else(..)`.
- `engine/hub/src/web/email.rs` (`Mailer::from_config`, :92, :98): the same for the SMTP URL
  and password, against `EmailConfig::check` -> the same rewrite. Its other caller,
  `engine/hub/src/cli/mod.rs` (`Command::CheckMail`, `devagotchi hub check-mail`), then passes
  its own `|name| std::env::var(name).ok()`, which is why `cli/mod.rs` is on the C1 list.
- `engine/hub/src/offers/nomen.rs` (`Nomen::from_config`, :50): reads the provisioning token ->
  take `env`, passed down from `Offers::from_config(&config, accounts, &env)`.
- `engine/hub/src/agenthub/issue_lifecycle/mod.rs` (`IssueLifecycleStore::from_config`, :29):
  reads the webhook secret -> take `env`.
- `engine/hub/src/identity/tests/providers.rs` (`world`): every provider secret is a `*_file`,
  so the `*_env` path has no test -> add one provider test that injects the client secret
  through the closure.

### C4: `hub_settings` holds only `SETTINGS` keys

- `engine/hub/src/ops/hub_settings.rs` (`MAIL_KEYS`, `stored_mail`, `save_mail`): five `mail.*`
  rows live in `hub_settings`. None passes `Setting::find`, so `LiveSettings::with_saved` skips
  them, and they are parsed by hand. `unwrap_or(5)` in `stored_mail` restates
  `default_per_address` (G3) and turns a damaged row into a default (G4) -> a `mail_settings`
  table (layout rule 8, notify `mail_`) with `smtp_url`, `from`, `per_address_per_hour`,
  `sealed_password`, `enabled`, `updated_at` and `updated_by`; the migration copies the `mail.*`
  rows and deletes them from `hub_settings`; `stored_mail` and `save_mail` use it; the column is
  `NOT NULL`, so no fallback is needed.

### C5: the file wins

- `engine/hub/src/web/hub_settings.rs` (`save_sign_in`, the `"new"` branch, :306-314): refuses
  a guided name or a stored name, but not a name the file's `auth.providers` uses. Saving `okta`
  when the file names `okta` makes `provider_apply::overlay` drop the file's provider without a
  word. Sinks refuse this case with 409 -> add `Providers::config_names()` and refuse with 409
  "This hub's configuration has a sign-in of that name; choose another."; add the case to
  `web/hub_settings_tests.rs`. The file is at 496 of 500 lines, so move `save_sign_in` into its
  own file first rather than grow it (layout rule 6).
- `engine/hub/src/web/provider_apply.rs` (`overlay`) with `engine/hub/src/web/sign_in_cards.rs`
  (`sign_in_card`): for the four guided names a saved row replaces the file's provider and its
  secret. The exception is not written down anywhere the plan or `/admin` shows -> record it in
  `.ai/dev/plans/hub-settings.md` and in `DEPLOY_TIME`'s `auth` line
  (`config/live/catalog.rs`), limited to the four guided names (the 409 above holds for every
  other name).

### C7: `SETTINGS` and `check()` ranges agree

- `engine/hub/src/config/tenancy.rs` (`TenancyConfig::check`), `config/workflows.rs`
  (`WorkflowsConfig::check`) and `config/analytics.rs` (`AnalyticsConfig::check`) against
  `config/live/catalog.rs` (`SETTINGS`): seven ranges are written twice (`starts_per_hour`,
  `max_unsecured`, `unsecured.machines`, `unsecured.clients`, `unsecured.idle_days`,
  `link_ttl_secs`, `retention_days`). They agree today, and nothing holds them -> add Mechanical
  check 1. Optionally, have each `check()` call a catalog helper
  `pub(crate) fn in_range(key, n: i64) -> Result<(), String>` built on `Setting::find` and
  `parse`, and delete the literals.

### P8: contact discovery before saving

- `engine/hub/src/web/hub_settings.rs` (`save_sign_in`): an OIDC provider is saved without
  fetching discovery, so a typo or an issuer mismatch fails only at a person's sign-in. Mail and
  ClickHouse check first. The test in `web/hub_settings_tests.rs` (`run`) saves
  `https://id.example.com`, which does not answer -> for an enabled `oidc` row, build the
  candidate with `Provider::from_parts` and run discovery (expose `Providers::endpoints` to the
  module) before saving; on failure return 400 "The issuer did not answer as an OpenID provider,
  so nothing was saved: {why}". Point the test at the stub issuer from
  `identity/tests/providers.rs` (`stub_server`), and add a case where an unreachable issuer saves
  nothing.

### P5 and P3: linking by email

- `engine/hub/src/web/providers.rs` (`finish`, :692) with
  `engine/hub/src/identity/accounts/identities.rs` (`sign_in_identity_in`, the email branch at
  :259-282, and the module doc): only `proven.provider == "google"` gets the link page, and only
  for a password account. Any other provider whose verified email another sign-in already
  carries is linked to the first such account without proof -> drop the provider condition;
  generalize `password_account_for` (`identity/accounts/passwords/mod.rs`) into
  `account_for_email(proven) -> Option<(account, proof)>`, where the proof is a password, a
  passkey or another linked sign-in; show the link page for every provider ("Sign in to that
  account first, then add this sign-in" when there is no password); remove the silent link and
  correct the module doc (G11); add GitHub and OIDC cases next to
  `google_for_an_address_with_a_password_account_asks_to_link_it` in
  `identity/tests/providers.rs`.

### P3: no provider name in flow code

- `engine/hub/src/web/providers.rs` (`Providers::buttons`, :208): `provider.config.name() ==
  "nomen"` decides whether `SHIPPIN_NOTICE` shows, while the nomen `Guide` already carries
  `notice: Some(SHIPPIN_NOTICE)` -> add `sign_in_cards::notice(name) -> Option<&'static str>`
  and render it when `Some`.
- `engine/hub/src/web/passkeys.rs` (`HubSignIn::provider_names`) against
  `web/providers.rs` (`Providers::buttons`): both sort by `sign_in_rank` and build the label
  -> add `Providers::offered(&self, site) -> Vec<(String, String)>`; `buttons` maps it to HTML
  and `provider_names` returns it.

### P7: guidance never prefills an issuer

- `engine/hub/src/web/sign_in_cards.rs` (`Guide::issuer`): the field's doc says the issuer is
  "filled in for the person to keep or change", but every row is `None` and P7 forbids a
  prefill (G11) -> drop the field, or state that it stays empty.

### K1: one cookie helper, `__Host-` everywhere

- `engine/hub/src/web/sign_in.rs` (`FRESH`, `fresh_cookie`, `fresh`): `dvg_fresh` has no
  `__Host-` name and a different prefix, against the `web/mod.rs` doc and MDN -> `__Host-dg_fresh`
  on https and `dg_fresh` on loopback, through the helper below; update
  `engine/hub/src/web/tests/mod.rs` and `tests/operator_browser.py`, which read `dvg_fresh`.
- `engine/hub/src/web/passkeys.rs` (`HubSignIn::session_cookie`, `challenge_cookie`, `cookie`),
  `web/nomen_login.rs` (`session_cookie`, `login_cookie`, `cookie`), `web/workflow.rs`
  (`code_cookie`, `browser_cookie`, `cookie`) and `web/providers.rs` (`signin_cookie`,
  `link_cookie`): eight hand-written name pairs and three identical `Set-Cookie` builders ->
  `web/cookies.rs` with `name(secure, base)` and `set(secure, base, value, max_age)`; replace
  the eight pairs and the three builders.

### K2: one seal and unseal

- `engine/hub/src/web/providers.rs` (`link_page` and `link`): the `Waiting` cookie repeats the
  MAC seal and unseal inline, beside `seal` and `unseal` for `Started` -> generic
  `seal<T: Serialize>(hub, purpose, value)` and
  `unseal<T: DeserializeOwned>(hub, purpose, cookie, ttl, issued)`. `Started` uses
  `"provider sign-in"` with `SIGN_IN_SECS`; `Waiting` uses `"link sign-in"` with `LINK_SECS`.

### K4: a new session on a privilege change

- `engine/hub/src/web/session.rs` (`open_and_return`): when an unsecured account is secured by
  linking a provider, the same session goes on, and
  `identity/tests/providers.rs` (`github_secures_the_account_made_on_first_connect`) asserts
  it -> pass `secured: bool` (the account was provisional before the link); when it is true,
  open a new session, set its cookie and end the old token; the test expects a new
  `dg_session`.

### K5: one strict origin check

- `engine/hub/src/web/approvals.rs` (`same_origin`, wrapped by `web::from_this_hub` in
  `web/mod.rs`): returns true when neither `Sec-Fetch-Site` nor `Origin` is sent. More than 30
  handlers in `web/` call it directly, among them the `/admin` saves, `POST /auth/password`
  (`web/sign_in.rs`) and the link page (`providers::link`); `tenancy/orgs/api/mod.rs` calls it
  through `from_this_hub` -> make `from_this_hub` strict as K5 states, run it as one layer with
  `EXEMPT`, and delete `approvals::same_origin` and its imports.
- `engine/hub/src/web/passkeys.rs` (`HubSignIn::from_own_page`): a second helper with another
  rule (it always requires `Origin`), used by `web/passkeys.rs`, `web/claim.rs`,
  `web/device.rs` and `web/oauth.rs` -> call `from_this_hub`.
- `engine/hub/src/web/hub_settings.rs` (`save_sign_in`, `save_analytics`),
  `web/hub_settings_live.rs` (`save`), `web/hub_settings_mail.rs` (`save`),
  `web/hub_settings_sinks.rs` (`save`, `remove`): each checks the owner first and the origin
  second, inside the handler -> the layer refuses first (step 1 of the record recipe); the
  handler keeps owner, 409 and validation.

### Record recipe step 8: the event, tested

- `engine/hub/src/web/hub_settings_tests.rs` (`run`): `save_sign_in` writes
  `sign_in.configured`, but no test asserts the event or that the secret is absent from it. The
  mail and live tests do -> after the GitHub save, read `hub.accounts.events(&home, 50)`, assert
  a `sign_in.configured` event with detail `github: on`, and assert that no event detail holds
  the saved secret.

### Provider recipe: a refusal test for every ID-token check

- `engine/hub/src/identity/tests/providers.rs` (`a_sign_in_that_does_not_check_out_is_refused`,
  with `Person`, `oidc_token` and `discovery`): covers wrong audience, missing cookie, swapped
  state, forged MAC and unknown provider, but not wrong `iss`, expired `exp`, wrong `nonce`, or a
  discovery issuer mismatch -> add `Person` fields `wrong_issuer`, `expired` and `wrong_nonce`,
  used by `oidc_token`, and assert 400 for each; add a `discovery_issuer` override and assert
  503 "That sign-in is unavailable".

### Layout and front doors

- `engine/hub/src/config/live.rs` beside `config/live/` (layout rule 2): one module, two homes
  -> `git mv engine/hub/src/config/live.rs engine/hub/src/config/live/mod.rs`; `mod catalog;`
  and `mod tests;` already resolve.
- `engine/hub/src/config/live.rs` (module doc, line 1): cites "hub-settings M7", but the plan's
  M7 is Network providers and the live settings are M9 (G11) -> say M9.
- `engine/hub/src/config/mod.rs` and `engine/hub/src/login/mod.rs` (front doors, layout rule 7):
  neither has Owns / Serves / Settings / Depends on -> `config/mod.rs`: Owns nothing in Postgres
  (reads the file and the env); Serves `HubConfig`, `Live` and `Pinned`; Settings: the file, its
  env overrides, and `SETTINGS` against `DEPLOY_TIME`; Depends on site, store, machines,
  identity, notify, extensions, `devagotchi-contracts` and `devagotchi-agenthub`.
  `login/mod.rs`: Owns `account.json` (0600) in the config directory; Serves
  `devagotchi login`, `logout` and `whoami`; Settings: `DEVAGOTCHI_HUB` and `--hub`; Depends on
  the hub's RFC 8414 metadata.
- `engine/hub/src/ops/hub_settings.rs` and `web/hub_settings{,_live,_mail,_owner,_sinks}.rs`
  (layout rule 8): `ops/mod.rs` Owns `hub_settings`, but `hub_` is the hub-settings domain ->
  with S8, a `settings/` domain: `mod.rs` (Owns `hub_settings`, plus `mail_settings` after the
  C4 fix; Serves `/admin` and `/admin/*`; Settings: `SETTINGS`; Depends on config, identity,
  notify, ops), `stored.rs` from `ops/hub_settings.rs`, and `web/`. Remove `hub_settings` from
  the Owns of `ops/mod.rs`.
- `engine/hub/src/web/{passkey_tests,claim_tests,hub_settings_tests,hub_settings_live_tests,hub_settings_mail_tests,hub_settings_sinks_tests}.rs`
  (G10): six test files beside the code -> move once, with S8: `passkey_tests.rs` (split by
  ceremony) and `claim_tests.rs` to `identity/web/tests/`, the `hub_settings*_tests.rs` to
  `settings/web/tests/{providers,live,mail,sinks}.rs`. No interim move.
- Over the 500-line cap (layout rule 6), held at their current size by
  `runtime/devagotchi/tests/layout/oversize.txt`: `web/providers.rs` (937), `web/passkeys.rs`
  (1112), `web/passkey_tests.rs` (1534), `web/workflow.rs` (1086), `web/oauth.rs` (751),
  `web/hub_settings_tests.rs` (542), `identity/auth.rs` (686), `identity/nomen/mod.rs` (696),
  `identity/nomen/tests/mod.rs` (711), `identity/accounts/oauth/mod.rs` (647),
  `identity/accounts/oauth/tests.rs` (683), `identity/accounts/tests.rs` (919),
  `identity/accounts/tenants/mod.rs` (506), `identity/accounts/tenants/tests.rs` (768),
  `identity/tests/providers.rs` (764) -> fold the K1 and K2 helpers out of `providers.rs` and
  `passkeys.rs` first; then split by subject (`providers/{mod,discovery,flow,link}.rs`,
  `passkeys/{mod,ceremonies,owner_link}.rs`, test files into `tests/` folders); lower each
  `oversize.txt` entry with `DEVAGOTCHI_WRITE_LAYOUT=1`.

Store failures read as "signed out" in this area (`hub_settings_owner::owner`,
`providers::finish`, both through `hub.who(headers).await.ok().flatten()`) break G4. They are
listed and fixed with hub-pages' `Who` arms, not here.
