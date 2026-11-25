# Pattern: a hub page and its form

> **For existing server-rendered pages only.** All UI converts to Svelte ([ui.md](ui.md), 2026-11-25); no new page is written this way.

This is how a page of the hub's web UI is routed, guarded, drawn, worded and tested. It covers
`engine/hub/src/web`: the phone app (`app/`), `/admin` (`hub_settings*`), sign-in, approvals,
machines, the landing page, organisations and the operator page. Follow it for every new page
or form, and for every change to one.

The shared rules G1 to G14 in [../patterns/README.md](../patterns/README.md) hold here too.
This file states only what is particular to pages. Where a rule below leans on a shared rule,
it names it (for example [G4](../patterns/README.md)) instead of repeating it.

## Exemplar

`engine/hub/src/machines/web/machine.rs`: the page `page` and its *Update now* POST `update`. Its
test is `a_machines_page_shows_its_state_and_jobs_and_updates_from_here_only` in
`engine/hub/src/web/app/tests.rs:362-488`.

The page (`page`, `machine.rs:32-144`):
- takes the person through the shared guard, with the return path percent-encoded (38-42);
- answers a machine that is not theirs with a 404 inside the frame, in the site's words;
- escapes every value and encodes the path segment in the form action;
- renders with `pages::app_page` on `Tab::Machines` (137).

The POST (`update`, `machine.rs:149-192`) runs in the canonical order:
1. origin check (157-164);
2. person (165-168);
3. waiting gate (169-177);
4. the change, through `Hub::update_machine`, the same method MCP's `machine_update` calls,
   which audits it (178-186);
5. 303 back to the page with the outcome in the query (187-191). The page shows it escaped,
   with `role=status`.

Two parts of the exemplar go when the target lands: its own origin check (157-164) moves into
the layer of rule 2, and its own `Referrer-Policy` insert (138-142) goes, because rule 5 makes
`same-origin` the default.

`web/app/agenthub/ticket_actions.rs` (`act`) has the same shape, with `board_access` as one
more step.

Supporting exemplars:
- `web/app/mod.rs:30-63` (`routes`): a module that owns its routes.
- `web/app/mod.rs:65-81` (`person`): the page guard.
- `web/orgs.rs:75-90` (`changer`): a change guard (origin, person, waiting).
- `web/pages/head.rs` (`render`): the one render path.
- `engine/hub/src/tenancy/orgs/api/mod.rs` (`bearer_or_session`): an origin check run as a layer.
- `web/notify.rs:85-90`: every `Who` arm matched for a JSON endpoint (401, 404, 503).

## The shape

```rust
// web/<area>.rs, or web/<area>/mod.rs: the module owns its routes (rule 1)
pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/app/things/{name}", get(page))
        .route("/app/things/{name}/do", post(act))
}

pub async fn page(State(state): State<AppState>, headers: HeaderMap,
                  Path(name): Path<String>, Query(shown): Query<Shown>) -> Response {
    let here = format!("/app/things/{}", encode(&name));            // rule 6
    let subject = match person(&state, &headers, &here).await {     // rule 3
        Ok(s) => s, Err(r) => return r };
    let site = pages::site(); let copy = &site.messages.app;        // rule 7
    // not theirs: 404 in the frame, with no hint that it exists
    // values: escape(..); path segments in href/action: encode(..)
    pages::app_page(StatusCode::OK, &title, &body, Tab::Machines)   // rules 5, 8
}

pub async fn act(State(state): State<AppState>, headers: HeaderMap,
                 Path(name): Path<String>, Form(form): Form<Act>) -> Response {
    // The origin layer (rule 2) has already refused another site.
    let here = format!("/app/things/{}", encode(&name));
    let subject = match change(&state, &headers, &here).await {     // rule 3
        Ok(s) => s, Err(r) => return r };
    let outcome = state.hub.do_the_thing(&subject, ..).await;       // rule 4
    see_other(&format!("{here}?state={}", encode(&outcome)), &[])  // rule 9
}
```

`change` is the area's one guard for a change. It runs, in order:
1. **Who** (`app::person`). Signed out goes to `/auth/login?return_to=<here>`. A hub with no
   sign-in is the not-set-up 404. The store being unavailable is a 503 ([G4](../patterns/README.md)).
2. **Let in**: `Hub::waiting_refusal`, for the changes `engine/hub/src/tenancy/gate.rs:4-6`
   names (MCP tools, adding or enrolling a machine, deciding on a board), unless the domain
   method already refuses them (`Hub::add_own_machine`).

An owner-only area uses one `owner_change` guard: `owner` (not the owner gives the unknown
path's 404, `hidden`, `web/operator.rs:57-60`; a store failure gives 503), then whatever the
area needs next.

## Rules

1. **A module owns its routes.**
   - A page module has `fn routes() -> Router<AppState>` that registers only its own handlers.
   - `web/routes.rs` merges these routers and keeps only the shell's routes.
   - `with_state` is called once, in `engine/hub/src/serve/mod.rs`.
   - A mode that hides an area decides whether its router is merged, in one place
     (`web/one_user/mod.rs`, `orgs`), not in each handler.
   - Exemplars: `web/app/mod.rs:30-63`, `engine/hub/src/machines/routes.rs:17-21`.
   - *Check:* every `.route(` in `routes.rs` names a handler of `routes.rs`, and no `routes()`
     names a sibling module's handler.

2. **One strict origin check, run as one layer before any handler.**
   - The check is `web::from_this_hub`, defined in `web/mod.rs` and nowhere else:
     - when `Sec-Fetch-Site` is present, it must be `same-origin`;
     - otherwise `Origin` must equal the origin of the hub's `public_url`;
     - with neither header, the request is refused.
   - It runs as a `middleware::from_fn_with_state` layer on the web router in
     `serve/mod.rs`, beside `web::bar`. It passes GET, HEAD and OPTIONS.
   - It has one `EXEMPT: &[(&str, &str)]` list, each entry with its reason. Only routes that
     read no cookie are on it: `/oauth/register`, `/oauth/token`, `/oauth/revoke`,
     `/oauth/device` and `/api/v1/machines`. Non-browser callers use these bearer routes.
   - `HubSignIn::from_own_page` (sign-in, claim, OAuth and device consent) calls
     `from_this_hub`; it has no rule of its own.
   - Because the layer runs first, a cross-site POST to an owner route gets 403, not 404. A GET
     still hides the route with 404.
   - No handler, guard or page module defines or imports another origin check.
   - *Check:* every non-exempt POST answers 403 to a cross-site POST that carries a live owner
     session, both with `sec-fetch-site: cross-site` and with only
     `Origin: https://elsewhere.example`.

3. **One guard per kind of caller, and every `Who` arm is matched.**
   - Pages use `app::person`. Each area has one change guard, and owner areas have one
     `owner_change`.
   - Match all four arms of `routes::Who` (`Person`, `SignedOut`, `NotSetUp`, `Unavailable`),
     with no `_ =>` arm ([G5](../patterns/README.md)). Never write `who(..).await.ok().flatten()`.
   - `Unavailable` and store errors follow [G4](../patterns/README.md): `routes::unavailable`
     (503) on pages, `issuer_unavailable` on JSON (`web/notify.rs:85-90`).
   - *Check:* with the session store failing, `/admin` (as owner), `/app/focus` and `/account`
     answer 503.

4. **A change is one call to the domain's method** ([G1](../patterns/README.md)).
   - When MCP has the action, it is the same method MCP calls (`Hub::update_machine`,
     `Board::decide_approval`, `Hub::add_own_machine`). Otherwise it is the owning domain's
     store method (keys, connections, settings).
   - That method checks ownership and scope (`approval_board`, `board_access`, `board_for`).
   - The change writes exactly one audit line ([G7](../patterns/README.md)). When the domain
     method already writes it, the page writes none.
   - `web/` holds no SQL.

5. **One render path, and `same-origin` is the default.**
   - Every HTML response comes from a `pages::*` wrapper (`web/pages.rs:177-461`) that calls
     `pages/head.rs` `render`. It is the only place that sets `text/html`.
   - `render` sets the per-`Kind` CSP, `nosniff`, `no-store` and
     `Referrer-Policy: same-origin`.
   - No page inserts its own `Referrer-Policy`.
   - Why: under `no-referrer`, a form navigation POST carries `Origin: null` (Fetch, step 4.1),
     which the strict check of rule 2 refuses. `same-origin` still sends nothing to other sites.
     A secret a page must keep from referrers goes in the URL fragment, which no referrer
     carries (`/invite` does this, `web/assets/invite.js`).

6. **Escape at the point of interpolation.**
   - Values from a person, an agent, a machine or the store go through `pages::escape`
     (`web/pages.rs:609-622`), inside double-quoted attributes.
   - Path segments and query values go through `web::encode` (`web/mod.rs:243-254`). Its
     output is attribute-safe.
   - Redirects go through `safe_return_to` or are built locally.
   - Copy with markup goes through `site::text::Ctx::inline`.
   - `site::text::{escape_text, attr}` are only for the site's own landing text.
   - *Check:* a `<b>` value comes back as `&lt;b&gt;` (`web/app/agenthub/ticket_tests.rs:100-102`).

7. **Every word is the site's.**
   - Titles, labels, buttons and refusals come from `pages::site().messages.<section>`, and
     values are filled through `Ctx`.
   - A section's default words live in `engine/hub/site/default/content/messages.json`. Its
     struct in `engine/hub/src/site/model.rs` has no Rust `Default` and no `#[serde(default)]`,
     on the struct or on any field. `AppCopy` is the model for this, except its `delegate`
     field (see the known violations). Closed inputs follow [G2](../patterns/README.md).
   - A site's JSON merges over the built-in one (`engine/hub/src/site.rs`, `merge_json`), so
     a site needs no Rust defaults.
   - No brand appears in the code (`engine/hub/src/site/brand_tests.rs`).

8. **The phone frame is one thing.**
   - A phone page renders with `pages::app_page`, or with `pages::app` when it turns push on,
     on a `Tab`.
   - `web::bar` gives every `Kind::App` page the person's `Me` (avatar, theme).
   - Tap targets keep `hub.css`'s 44 px minimum. That matches WCAG 2.5.5 (AAA); `hub.css`
     does not cite it.
   - *Check:* no sideways scroll at 390 px (iPhone 13) or 412 px (Pixel 7).

9. **POST, then 303, then GET.**
   - A change that succeeds answers `routes::see_other` (303, `no-store`) back to the page,
     with its outcome in the query. The page shows that outcome escaped, with `role=status`.
   - A refusal answers `pages::message` with a status that says what happened:
     - 400 for bad input;
     - 403 when the caller may not do it (rule 2 gives 403 for another site);
     - 404 when the thing is not theirs;
     - 409 for a conflict;
     - 503 when a store failed ([G4](../patterns/README.md)).
   - The one exception is a page that shows a secret once (a new API key, an enrolment code).
     It answers 200.
   - A GET never changes what a person asked for. `SameSite=Lax` does not stop a cross-site
     GET (OWASP). The one exception is a workflow's link: `GET /go/<token>` and
     `/go/<token>/status` record the steps the hub already sees done, and complete the
     workflow when all are (`Hub::look_at_workflow`, `engine/hub/src/workflows/mod.rs`). That
     write only records what already happened, the token is the credential, and a repeat
     changes nothing. The header of `web/workflow.rs` says such a GET writes nothing, which is
     not true (G11).

## Where files go

- **Today:** `web/<area>.rs`, or `web/<area>/mod.rs` with `routes()` when the area spans more
  than one file. Never both `<area>.rs` and `<area>/` ([layout.md](layout.md) rule 2).
- **Tests:** as [G10](../patterns/README.md) says. The `*_tests.rs` files already in `web/`
  move once, with S8, and not before (C15 in the rewrite plan). Do not add new ones.
- **Assets:** scripts are `web/assets/<name>.js`, compiled in and served from this origin. CSS
  is `web/hub.css` and `web/landing.css` (`engine/hub/src/site/style.rs`). Copy is
  `messages.json` plus its struct in `site/model.rs`.
- **After the S8 ruling** ([structure-cleanup S8](../dev/plans/structure-cleanup.md)): a
  domain's pages and their tests move to `<domain>/web/` unchanged, and the `routes()` they
  already own is what makes that a pure move. `web/` keeps the shell: `mod.rs`, `pages`,
  `routes.rs`, the app frame and the test harness. Until a step lands, code stays where it is.

## The canonical test

Rust, with the whole router served over HTTP on the test Postgres:
- Use `Hosted` (`engine/hub/src/tenancy/tests/tenants.rs`; its `form()` sends the hub's
  `Origin`) or `World` (`web/passkey_tests.rs`). This is axum's real-server style
  (`the_real_deal`): `bar`, `counted`, cookies, `Origin` and redirects only exist as served.
- Store fixtures follow [G13](../patterns/README.md). Without `DEVAGOTCHI_TEST_POSTGRES_URL`,
  `or_skip!` skips the test, so a local pass without that variable is not evidence. At main
  2b3feec it returns silently. B0 makes it print `SKIPPED: no DEVAGOTCHI_TEST_POSTGRES_URL`,
  and CI's `hub-postgres` job sets both the URL and `DEVAGOTCHI_TEST_REQUIRE_POSTGRES=1`
  (`.github/workflows/ci.yml`, job `hub-postgres`).

One test per page and form (template: `web/app/tests.rs:362-488`) asserts:
1. a cross-site POST with a live cookie gets 403 and nothing changes, once with
   `sec-fetch-site: cross-site` and once with only `Origin: https://elsewhere.example`;
2. a same-origin `hub.form` gets 303 to the page with the outcome;
3. the page shows the outcome;
4. exactly one audit row exists for the change ([G7](../patterns/README.md));
5. another tenant gets 404 on the page, and its POST changes nothing;
6. signed out, the page redirects to `/auth/login?return_to=<encoded here>`;
7. a `<b>` value is escaped.

Browser: `tests/<area>_browser.py`. CI's `e2e` job runs every one of them on `ubuntu-24.04`
(step *Sign-in pages in Chrome* at main 2b3feec, `ci.yml:258-272`; B0 splits it into one step
per script). Each runs through Playwright on the runner's Chrome (`--channel chrome`), against
the real binary on Postgres. A page's script uses a phone device preset (`p.devices`), taps
through the flow and asserts no sideways scroll (`tests/app_browser.py:93-96`,
`no_sideways_scroll`, 390 px; `tests/android_browser.py:62-70`, `sideways`, 412 px).

## Anti-patterns

- Registering another module's handlers (`web/hub_settings_live.rs:27-46`).
- An origin check defined or imported outside `web/mod.rs` (`approvals::same_origin`), or one
  that lets a request with neither header through.
- `hub.who(headers).await.ok().flatten()?` and `_ => 401`: a store outage reads as "not the
  owner" or "signed out" (`hub_settings_owner.rs:48`, `operator.rs:53`, `app/mod.rs:134`).
- `unwrap_or_default()` on a store read that a page shows (`routes.rs:433, 442`).
- A page that inserts its own `Referrer-Policy`.
- `escape` where a path segment needs `encode` (`app/keys.rs:75-77`).
- English literals in handlers, and default copy that exists only in a Rust `Default`.
- SQL in `web/` (`web/sessions.rs`).
- A GET that changes state.

## The mechanical check

None of these checks exists at main 2b3feec. Each lands green together with the fix it
guards ([G12](../patterns/README.md)). Single-crate source scans share the walker lifted from
`site/brand_tests.rs` (G12).

1. **Rules 2 and 3: `engine/hub/src/web/tests/forms.rs`,
   `every_post_route_refuses_another_site`.**
   - Collect every route path whose method chain contains `post(` from `routes.rs` and every
     `routes()` it merges, with a small parser over multi-line `.route(` calls.
   - Assert that each `EXEMPT` entry is a real route and that the remaining set is not empty.
   - On `Hosted` with the owner's cookie, POST each path (with `{x}` filled in) twice: once
     with `sec-fetch-site: cross-site`, once with only `Origin: https://elsewhere.example`.
     Expect 403 both times. The layer runs before form extraction, so no body is needed.
   - A source scan: no file under `web/` other than `mod.rs` defines or imports an origin
     check.
2. **Rule 5: `pages::head::tests::every_page_kind_keeps_its_origin_on_its_own_posts`**
   (inline in `head.rs`, a leaf under the cap, G10).
   - `render` for each `Kind` answers `Referrer-Policy: same-origin`, `nosniff`,
     `Cache-Control: no-store`, and a CSP with `default-src 'none'`, `frame-ancestors 'none'`
     and `base-uri 'none'`.
   - A source scan fails on any `REFERRER_POLICY` insert under `web/` outside `head.rs`.
3. **Rule 3: a source scan and a behaviour test, in `web/tests/forms.rs`.**
   - Fail on `who(` followed by `.await.ok()`, across line breaks, under `web/`, and on
     `Ok(Some(..)) = ..who(..)`. Its allowlist only shrinks; today it holds
     `hub_settings_owner.rs` (`owner`), `operator.rs` (`operator`), `providers.rs` (`finish`),
     `sign_in.rs` (`answer`), `workflow.rs` (`page`) and `offers.rs` (`go`).
   - Fail on a `match` over `Who` with a `_ =>` arm.
   - Behaviour: `Hosted` with its store pool closed; `/admin` as owner, `/app/focus` and
     `/account` each answer 503.
4. **Rule 7: the copy ratchet, `engine/hub/src/web/tests/copy.rs` with its baseline file
   beside it.**
   - Count each web file's non-test literals of two or more English words outside markup,
     with the `brand_tests` scanner.
   - A listed file may shrink but never grow; an unlisted file must be at 0.
     `DEVAGOTCHI_WRITE_COPY=1` only lowers counts.
   - In `engine/hub/src/site/tests.rs`: the built-in `messages.json` parses, and no section
     of `Messages`, and no field of a section, carries `#[serde(default)]`.
5. **Rule 8: `web/app/tests.rs`, `every_app_framed_page_wears_the_persons_theme`.**
   - A `Hosted` person saves `theme=dark` on `/app/me`.
   - `/app`, `/approvals`, `/admin` (as owner) and `/orgs` each contain `data-theme="dark"` and
     `id="me-link"`. It fails today on `/admin` and `/orgs`.

Already in place and kept: `site/brand_tests.rs`; `runtime/devagotchi/tests/layout.rs` (the
500-line ratchet); the per-flow `Hosted` tests; the browser suite's sideways-scroll checks in
CI `e2e`.

## Upstream sources

| Source | What it grounds |
|---|---|
| https://pkg.go.dev/net/http#CrossOriginProtection | Go's CSRF guard is one handler wrapper with an exempt list, and it always allows GET, HEAD and OPTIONS. The model for rule 2's layer. |
| https://go.dev/src/net/http/csrf.go | Go allows `Sec-Fetch-Site: none`, compares `Origin` with `req.Host`, and fails open when both headers are missing. Rule 2 is stricter on all three: it refuses `none`, compares the configured `public_url` (right behind a proxy that rewrites Host), and fails closed, because non-browser callers use the exempt bearer routes. |
| https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html | `Sec-Fetch-Site` first, `Origin` as the fallback, fail safe when both are missing, `Origin: null` is not same-origin, SameSite is defence in depth only, a GET must not change state. |
| https://fetch.spec.whatwg.org/#append-a-request-origin-header | Step 4.1: for a non-GET/HEAD request not in cors mode, `no-referrer` serializes the origin as `null`. The bug recorded at `web/oauth.rs:472-475`. `fetch()` in cors mode keeps its origin. |
| https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Referrer-Policy | `same-origin` sends nothing cross-origin, and the fragment is never sent. Rule 5's default. |
| https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Sec-Fetch-Site | Available across browsers since March 2023; `none` is a user-originated navigation. |
| https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Status/303 | 303 after a POST so the client fetches the result with GET. Rule 9. |
| https://docs.rs/axum/latest/axum/routing/struct.Router.html | `merge` combines module routers; `with_state` once. Rule 1. |
| https://docs.rs/axum/latest/axum/middleware/index.html | `from_fn_with_state`, already used by `web::bar` and `bearer_or_session`. Rule 2's layer. |
| https://github.com/tokio-rs/axum/blob/main/examples/form/src/main.rs | `get(show).post(accept)` with `Form<T>`. Upstream has no CSRF check and no PRG; the repo adds both. |
| https://github.com/tokio-rs/axum/blob/main/examples/testing/src/main.rs | `the_real_deal`: a bound listener and a real client. The canonical test's style. |
| https://github.com/tokio-rs/axum/blob/main/examples/templates/src/main.rs | Upstream renders with Askama. The repo uses `format!` with escaping at interpolation, which holds only with one render path, a strict CSP and escaping tests (rules 5, 6). Not re-fetched in the review. |
| https://cheatsheetseries.owasp.org/cheatsheets/Cross_Site_Scripting_Prevention_Cheat_Sheet.html | HTML-encode body and quoted-attribute values; percent-encode URL parts. `pages::escape` and `web::encode`. Not re-fetched in the review. |
| https://playwright.dev/docs/emulation | Device presets set viewport and `isMobile` (`p.devices` in the browser tests). Not re-fetched in the review. |
| https://www.w3.org/WAI/WCAG22/Understanding/target-size-enhanced.html | 2.5.5 Target Size (Enhanced), AAA: 44 by 44 CSS px. Rule 8. |

## Known violations

All re-checked at main 2b3feec. Paths are under `engine/hub/src/` unless they say otherwise.

### Rule 1 (a module owns its routes)

- `web/routes.rs:42-136` (`router`): registers about 60 handlers that belong to passkeys,
  landing, providers, email, machines, operator, analytics, approvals, notify, public,
  connectors, discovery, workflow, offers, media, claim, oauth and device. Only app, orgs
  (through `one_user::orgs`), sign_in and hub_settings_live own their routers. -> Give each
  module `pub(super) fn routes() -> Router<AppState>`. `routes.rs` keeps the shell's routes
  (`/`, `/account`, `/signed-out`, `/auth/login`, `/auth/callback`, `/auth/logout`,
  `/auth/password/set`, `/api/v1/me`, `/assets/hub.css`) and merges the rest. This is the
  precondition S8 names (`.ai/dev/plans/structure-cleanup.md:117`).
- `web/routes.rs:29-34, 110-112` (`router`'s `hub_authorization`): always `true`; its one
  caller is `serve/mod.rs:447`. The early return is dead, and the doc names a
  `crate::oauth` that does not exist. -> Remove the parameter and the branch, merge
  `claim::routes()`, `oauth::routes()` and `device::routes()` unconditionally, and update
  `serve/mod.rs:447`.
- `web/hub_settings_live.rs:27-46` (`routes`): the `live` part registers the whole `/admin`
  area, including `hub_settings::page`, `save_sign_in`, `save_analytics`,
  `hub_settings_mail::save` and `hub_settings_sinks::save` and `remove`. -> Move `routes()`
  into `hub_settings.rs` and merge `hub_settings::routes()`. The folder move waits for S8,
  which targets `settings/` (`.ai/dev/plans/structure-cleanup-map.md:221`).

### Rule 2 (one strict origin check, in one layer)

- `web/approvals.rs:85-95` (`same_origin`), with 32 call sites in 21 files, and
  `web/mod.rs:138-142` (`from_this_hub`, a wrapper used once, by
  `tenancy/orgs/api/mod.rs` `bearer_or_session`): the check lives in a page module, every
  handler calls it, and it lets a request with neither header through (`None => true`). ->
  Move the strict body into `web/mod.rs::from_this_hub`, run it as the layer with `EXEMPT`,
  delete `approvals::same_origin` and every per-handler call. Any test that POSTs without
  `Origin` must send it (`Hosted::form` and the `hub_settings_tests.rs` helper already do).
- `web/passkeys.rs:303-310` (`HubSignIn::from_own_page`): a second origin rule. -> Call
  `from_this_hub`.

### Rule 3 (one guard per caller, every `Who` arm)

- `web/approvals.rs:102, 212, 347, 419` (`list`, `decider`, `test`, `decide`): four inline
  copies of the `Who` match, each with an unencoded `return_to=/approvals`. -> Use
  `super::app::person(&state, &headers, "/approvals")`, and route `test` and `decide` through
  one guard shaped like `decider`.
- `web/machines.rs:30-50` (`person`), `web/orgs.rs:63-74` (`signed_in`),
  `web/routes.rs:426` (`account`): re-implementations of `app::person`. -> machines: call
  `app::person(state, headers, "/account")` (the layer does the origin). orgs: delete
  `signed_in`; `changer` calls `super::app::person`. account: `match app::person(..)`.
- `web/app/connections.rs:155-175, 232-246` (`add`, `remove`), `web/app/machine.rs:157-177`
  (`update`), `web/app/agenthub/ticket_actions.rs:52-69` (`act`), `web/app/me.rs:310-322`
  (`save`), `web/app/keys.rs:199-209` (`gate`): six copies, in five files, of the origin check
  and the person guard. Only `machine.rs` and `ticket_actions.rs` add the waiting gate.
  `connections.rs` runs person before origin, with its own English refusals. No write happens
  before the check. -> Add `pub(super) async fn change(state, headers, here)` in
  `web/app/mod.rs`: person, then `waiting_refusal` where `gate.rs` names the action. Make it
  the first statement of each handler and delete `keys::gate`.
- `web/hub_settings.rs:280-294, 391-410`, `web/hub_settings_live.rs:155-170`,
  `web/hub_settings_mail.rs:176-195`, `web/hub_settings_sinks.rs:252-271, 330-343`: six
  copies of `owner`, then the passkeys check, then `same_origin` with the same English
  refusal. -> Add `owner_change(state, headers) -> Result<(Subject, &Arc<HubSignIn>), Response>`
  in `hub_settings_owner.rs`: `owner` (not the owner gives `hidden`, a store error gives
  `unavailable`), then passkeys. The origin check goes to the layer.
- `web/hub_settings_owner.rs:45-50` (`owner`), `web/operator.rs:47-55` (`operator`):
  `hub.who(headers).await.ok().flatten()?` turns a session-store failure into a 404 on
  `/admin`, its 6 POSTs, `/operator`, `/operator/admit`, `/operator/plan` and
  `/operator/analytics(.csv)`. -> Return `Result<Option<Subject>, ()>`; callers map `Err` to
  `unavailable()` and `Ok(None)` to `hidden()`.
- `web/app/mod.rs:132-135` (attention) and `161-164` (focus): `_ => 401` folds `Unavailable`
  and `NotSetUp` into signed out. `web/assets/app.js` ignores any answer that is not ok, so
  the harm is a wrong status in logs and monitoring. -> Match as `notify.rs:85-90` does: 401,
  404, 503.
- `web/routes.rs:433, 442` (`account`): the tenant's events and linked identities are read with
  `unwrap_or_default()`, so a store error shows an empty history as if it were true. -> On
  `Err`, log and return `unavailable()`, as the same file does for other reads.
- `web/orgs.rs:758-761` (`remove_member`, `leaving`), `web/orgs.rs:998` (`invite_page`),
  `web/mod.rs:127-136` (`session_subject`), `web/providers.rs:688` (`finish`),
  `web/sign_in.rs:222` (`answer`), `web/workflow.rs:244-251` (`page`, `session_account`),
  `web/offers.rs:156` (`go`): `Who::Unavailable` or a store error reads as not signed in. For
  `session_subject`, the orgs API falls through to bearer and answers 401 instead of 503. ->
  Match every arm and return `unavailable()` on `Unavailable`. `session_subject` returns
  `Result<Option<Subject>, ()>`, and `bearer_or_session` maps `Err` to 503
  `issuer_unavailable`.
- `web/mod.rs:160-175, 184-187` (`bar`): a store error on the session reads as signed out (the
  bar offers Sign in), and the `Who` match that finds `Me` ends in `_ => None`. -> Name each
  arm; on `Unavailable` the bar shows neither Sign in nor `Me`.

### Rule 4 (one call to the domain's method; no SQL in `web/`)

- `web/sessions.rs` (`SessionStore`): the Nomen web-session store (`auth_web_sessions`) runs
  sqlx queries from `web/`. -> It moves with S8 to `identity/web/sessions.rs`
  (`.ai/dev/plans/structure-cleanup-map.md:246`), with its SQL in that domain's stored rows.
  `nomen_login.rs` calls it through the domain's API.

### Rule 5 (one render path, `same-origin` by default)

- `web/pages/head.rs:143-146` (`render`): the default is `no-referrer`, so every form
  navigation POST carries `Origin: null`. 13 places opt in to `same-origin` by hand:
  `app/agenthub/ticket.rs:410`, `app/connections.rs:144`, `app/keys.rs:177`,
  `app/machine.rs:140`, `device.rs:148`, `hub_settings.rs:268`, `oauth.rs:477`,
  `offers.rs:65`, `pages.rs:388`, `pages.rs:591`, `passkeys.rs:471`, `providers.rs:824`,
  `sign_in.rs:45`. Missing it once broke OAuth Connect (`oauth.rs:472-475`). -> Set
  `same-origin` in `render` and delete the 13 inserts.
- Pages with a POST form still under `no-referrer`: `approvals.rs` (`list`), `app/me.rs`
  (`page`), `operator.rs` (`page`), the `/orgs*` pages (`orgs.rs`), `pages.rs` (`account`
  when secured, and the bar's Sign out in `bar_html`), `email.rs` (`page`), `passkeys.rs`
  (`owner_password_page`). Today they pass only because the browser sends `Sec-Fetch-Site`.
  A browser that does not sends `Origin: null`, which today's check and rule 2's both refuse.
  -> Fixed by the `render` default.
- `web/orgs.rs:1023-1026` (`invite_page`): sets `no-referrer` explicitly on a page with a POST
  form (1002-1003), to protect a token that is in the URL fragment, which no referrer carries.
  -> Delete the insert.

### Rule 6 (escape and encode)

- `web/app/connections.rs:98` (connection name), `web/app/keys.rs:75-77` (key id),
  `web/app/me.rs:178` (org slug), `web/hub_settings_sinks.rs:154` (sink name),
  `web/sign_in_cards.rs:260` (provider name): path segments in `action` or `href` go through
  `escape` instead of `encode`. Safe today only because the names use a narrow character
  set. -> `crate::web::encode(&value)` for the segment, `escape` for display, as
  `app/machine.rs:133` does.

### Rule 7 (every word is the site's)

- `web/approvals.rs`, `web/app/keys.rs`, `web/app/connections.rs`: the phone app's Approvals,
  API keys and Connections pages hardcode English (for example `approvals.rs:108-113`,
  `keys.rs:190-206`, `connections.rs:162-174`), though `app/mod.rs` promises "Every word is
  the site's". -> Add `approvals`, `keys` and `connections` sections: structs in
  `site/model.rs`, text in `messages.json`, rendered through `copy` and `Ctx`.
- The shell, account, admin and sign-in pages: `web/routes.rs:179-195`, `web/pages.rs`
  (`bar_html`, `account`), `machines.rs`, `hub_settings*.rs`, `sign_in_cards.rs`,
  `sign_in.rs:192-196`, `passkeys.rs`, `providers.rs`, `email.rs`, `device.rs`, `oauth.rs`,
  `claim.rs`, `hosted.rs`, `landing/`, `orgs.rs:726` (`role_of`): the words are in Rust,
  including the bar (Approvals, Machines, Sign out), the unsecured nudge, and refusals such as
  "Not from this hub". -> Move them section by section (`bar` and `account` first, then admin,
  then sign-in) and hold the rest with the copy ratchet (check 4).
- `site/model.rs` `FocusCopy`, `MeCopy`, `OrgsCopy` and `SignInMessages` (their `Default`
  impls at `model.rs:961, 1030, 1154, 1367`), with `#[serde(default)]` on their `Messages`
  fields (`model.rs:893-905`) and on each struct: `me`, `orgs` and `focus` exist only as Rust
  defaults and are missing from the built-in `messages.json`; `sign_in` has both. `AppCopy`'s
  `delegate` field is the same case: `#[serde(default)]` at `model.rs:1325`, `DelegateCopy`'s
  `Default` at `model.rs:921`, and no `delegate` in `messages.json`'s `app`. -> Write the
  current defaults into `messages.json`, then drop `#[serde(default)]` and the `Default` impls.

### Rule 8 (the phone frame)

- `web/mod.rs:181` (`bar`, `framed`): covers only `/approvals`, `/app` and `/app/*`. `/admin`
  (`hub_settings.rs:266`, `pages::app`) and `/orgs*` (`orgs.rs:55`, `app_page`) also render
  `Kind::App` but get no `Me`: no avatar link, and the person's theme is ignored. -> Compute
  `me` for every signed-in request and let `render` use it only for `Kind::App`, so the path
  list cannot drift from `Kind`.

### Rule 9 (status codes)

- `web/machines.rs` (`add`, `add_json`, `revoke`): `add` and `add_json` map `device_store` to
  503, but nothing produces that code. A store failure arrives as `account_store`
  (`devagotchi_store::store_error`, `platform/store/src/lib.rs`), so all three answer it 409.
  `revoke` also answers its `not_found` 409. `web/workflow.rs` (`command`) has the same dead
  `device_store` arm. -> Map `account_store` to 503 and `not_found` to 404, and keep the rest.
- `web/app/keys.rs:274, 296` (`revoke`, `rotate`): `not_found` answers 400 through `refused`.
  -> `Err(e) if e.code == "not_found" => pages::message(StatusCode::NOT_FOUND, ..)`.

### Where files go (layout rules 2 and 3)

- `web/pages.rs` with `web/pages/`, and `web/landing.rs` with `web/landing/`: each area has two
  homes. -> `pages` is shell and stays: `git mv pages.rs pages/mod.rs` now and move its
  `runtime/devagotchi/tests/layout/oversize.txt` entry. `landing` moves in S8 to `site/web/landing/mod.rs`
  (`.ai/dev/plans/structure-cleanup-map.md:223`).
- `web/{analytics,claim,discovery,hub_settings,hub_settings_live,hub_settings_mail,hub_settings_sinks,landing,media,naming,operator,passkey,shippin,site,walkthrough}_tests.rs`,
  `web/app/approval_tests.rs`, `web/app/agenthub/ticket_tests.rs`: 17 `*_tests.rs` files beside
  code (G10). -> Each moves once, with its domain in S8, as a rename with the nextest count
  unchanged. No interim move.
