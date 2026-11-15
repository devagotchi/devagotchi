# Devagotchi accounts and sign-in

A hub has one account system, `auth.mode` `hub`. Accounts, passkeys, linked sign-ins and
browser sessions are in the hub's Postgres. AI clients (Claude, ChatGPT, Cursor, Codex, the
CLI) sign in with the hub's own OAuth 2.1 server at `/oauth/*`. Sign in with Shippin (Nomen),
Google, GitHub and any OpenID Connect provider are optional sign-in buttons: the hub serves
with none of them configured. Operator detail is in [crates/hub/README.md](../crates/hub/README.md):
"The hub's own accounts", "AI clients sign in with the hub itself" and "Shippin sign-in
(Nomen)".

A hosted hub (`tenancy.mode` `multi`, app.devagotchi.com) makes each person's account and
tenant the first time they connect from Claude, Codex or the CLI, and is never claimed; see
[crates/hub/README.md](../crates/hub/README.md), "A hosted hub".

`/auth/login` offers configured Google, GitHub or OIDC providers and email/password sign-in
before the passkey fallback. Password sign-in is for an existing account; single-owner hubs do
not expose public account creation. On first setup, the host-issued owner link can establish a
local login email and password directly at `/owner/password`; a passkey is optional. A
signed-in owner can change these credentials on `/account`. The address is a local login
name; it is not claimed as verified and does not require Google, Nomen, or an SMTP server.
Email-link recovery is available only when `auth.email` has a working mail server configured
and the person can receive mail at that address. Google, GitHub, Nomen and email-link sign-in
are not enabled automatically: the hub operator must configure provider credentials and mail
delivery. If locked out of a self-hosted hub without working email recovery, its host operator
can issue a replacement owner link with `devagotchi hub owner --replace`.

## A minimal hub.json

```json
{
  "listen": "127.0.0.1:8787",
  "public_url": "https://hub.example.com",
  "auth": { "mode": "hub" },
  "storage": { "url": "postgres://hub@db.internal/devagotchi" },
  "brokers": [],
  "grants": []
}
```

The hub keeps everything in that Postgres; its state directory holds keys only. The
database password is never in the file: `DEVAGOTCHI_HUB_STORAGE_URL`, set from the secret
store, names the database instead and overrides `storage.url`.

## Tokens

An AI client registers itself at `/oauth/register` (RFC 7591, public clients only), signs the
person in through `/oauth/authorize`, and trades the code (PKCE S256) at `/oauth/token` for an
access token (`dgh_…`) and a refresh token (`dgr_…`). Both are opaque and stored only as
hashes; refresh tokens rotate, and a spent one presented again ends its family.
`devagotchi login --hub URL` is device authorization: the person approves the code on the
hub's `/device` page, signed in. With `auth.nomen` as well, a token that is not the hub's own
goes to Nomen.

`auth.mode` `nomen` and `oidc` check tokens another issuer made and keep no accounts. `dev`
(a static token file) is for tests, on a loopback address only.

## Website

devagotchi.com profiles are a separate table, keyed today by a ChatGPT user id, with a unique
email. They become the same Devagotchi account when someone signs in through this hub and the
token's verified email matches the profile. See `apps/website/lib/hub-account.ts`. An
unverified or different address does not link. An account id already stored on the profile is
kept.
