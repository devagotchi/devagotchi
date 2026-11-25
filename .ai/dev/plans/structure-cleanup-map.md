# Structure cleanup: the file-by-file map

Part of [structure-cleanup](structure-cleanup.md). Every Rust file in `crates/` and `apps/` on `main` 5104f35 (2026-11-16), where it goes, the step that takes it there, and the one job it owns, which is the first line of its module doc. A file with no step stays where it is. Paths are shown inside their crate's `src/`. AgentHub's own files are in `devagotchi/agenthub` and get their own plan there (S11).

288 files; 91 move; 197 stay.

## apps/desktop

| Now | Goes to | Step | Lines | Job, and what is split out |
|---|---|---|---|---|
| `src-tauri/build.rs` |  |  | 14 | (no module doc) |
| `src-tauri/src/lib.rs` |  |  | 351 | A small native inspector. The independently running broker owns every job. |
| `src-tauri/src/main.rs` |  |  | 28 | (no module doc) |

## engine/connectors

| Now | Goes to | Step | Lines | Job, and what is split out |
|---|---|---|---|---|
| `catalog.rs` |  |  | 66 | The catalog's files, compiled in. A test holds this list equal to the `catalog` folder. |
| `lib.rs` |  |  | 287 | How each AI client connects to a Devagotchi hub, as data. |
| `tests.rs` |  |  | 175 | The catalog is checked as code: every file on disk is compiled in, reads against the |

## platform/contracts

| Now | Goes to | Step | Lines | Job, and what is split out |
|---|---|---|---|---|
| `bin/export-contracts.rs` |  |  | 112 | Export canonical Rust DTO schemas without opening a broker or journal. |
| `files.rs` |  |  | 266 | The file tools' wire types: bounded reads, listings, writes, edits, information, new |
| `interactive.rs` | `interactive/mod.rs` | S10 | 236 | Interactive operations: a program started with its input kept open, on a |
| `interactive/tests.rs` |  |  | 75 | (no module doc) |
| `lib.rs` |  |  | 468 | Canonical wire types. Serde rejects unknown fields; broker validation enforces bounds. |
| `link_frame_tests.rs` | `link/tests.rs` | S10 | 61 | done in B1: the link frames are in `link/mod.rs`, re-exported flat |

## runtime/devagotchi

| Now | Goes to | Step | Lines | Job, and what is split out |
|---|---|---|---|---|
| `build.rs` |  |  | 12 | The build's name, fixed at compile time: `DEVAGOTCHI_BUILD` from the environment (CI sets |
| `main.rs` |  |  | 292 | `devagotchi`: the one binary. A nested command tree over the same libraries the |
| `paths.rs` |  |  | 179 | Where this user's Devagotchi lives when nothing says otherwise: the XDG directories |
| `start.rs` |  |  | 495 | `devagotchi start`: this user's broker, with no paths to remember. The first run writes |
| `tests/ai_workflow.rs` |  |  | 417 | Native CLI integration: no board, account, shell JSON parsing or network required. |
| `tests/layout.rs` |  |  | 261 | The size ratchet from `.ai/patterns/layout.md`: no new Rust file over the cap, and no |
| `tests/one_binary.rs` |  |  | 1148 | The one binary, driven as a person would: `devagotchi start` with nothing but an |

## engine/hub

| Now | Goes to | Step | Lines | Job, and what is split out |
|---|---|---|---|---|
| `build.rs` |  |  | 6 | The migrations are embedded at compile time (`sqlx::migrate!`); a new or changed file there |
| `agenthub/issue_lifecycle.rs` | `agenthub/issue_lifecycle/mod.rs` | S5 | 263 | Signed ingress and authorized projections of the canonical board; no separate task store. |
| `agenthub/issue_lifecycle/tests.rs` |  |  | 626 | (no module doc) |
| `agenthub/mcp/mod.rs` |  | S9 | 2998 | The board's MCP face: `board_*` tools on the hub's `/mcp`, one per protocol verb,; S3b: keeps Boards, Accounts, Bound, Kind and the helpers; the two `impl Hub` blocks (lines 623 to 2461) go by tool group to `claims.rs`, `tickets.rs`, `notes.rs`, `settings.rs`, `approvals.rs`, `inbox.rs`; tests to `agenthub/mcp/tests/` |
| `agenthub/mcp/schema.rs` |  |  | 397 | What the board tools take and give: each tool's input and output, as the MCP schema |
| `agenthub/mod.rs` |  |  | 17 | AgentHub in the hub. The board itself (tickets, leases, workflow, approvals, notes that |
| `agenthub/registry.rs` |  |  | 148 | (no module doc) |
| `agenthub/tests/ahead.rs` |  |  | 147 | Approving ahead of time end to end over HTTP, the whole router as it serves |
| `agenthub/tests/hosted.rs` |  |  | 251 | The hub's hosted boards as the hub opens them: the router on Postgres alone, its listener, |
| `agenthub/tests/mod.rs` |  |  | 5 | (no module doc) |
| `agenthub/tests/notes.rs` |  |  | 156 | A note left for an AI, end to end over HTTP, the whole router as it serves: a person leaves |
| `agenthub/tests/served.rs` |  |  | 592 | AgentHub as the hub serves it: the board command line against a real hub (`mod bound`), a |
| `agenthub/tests/settings.rs` |  |  | 128 | `board_settings` on a hosted hub over MCP (`tenancy/tests/tenants.rs`'s world): the account sets |
| `config.rs` | `config/mod.rs` | S5 | 1544 | Hub configuration: who may reach which broker, and how callers are authenticated.; then S7 splits it by section: `auth.rs` (AuthConfig, TokenConfig, NomenAuth, NomenRef, TrustedIssuer, check_rp_id), `storage.rs` (StorageConfig), `site.rs` (SiteConfig), `boards.rs` (AccountBoardsConfig, BoardEntry), `signals.rs` (SignalsConfig, SinkConfig, SinkFields, valid_env_name, sink_url_ok), `web.rs` (WebConfig, loopback_http); BrokerEntry and Grant join `machines.rs`; each section validates itself; `mod.rs` keeps HubConfig and load |
| `config/analytics.rs` |  |  | 112 | `analytics` in the hub's config: whether a hosted hub counts visits to its public pages |
| `config/github.rs` |  |  | 98 | (no module doc) |
| `config/hub_tests.rs` | `config/tests/hub.rs` | S7 | 368 | `auth.mode hub` (D25, §14): the hub signs people in with its own accounts and passkeys,; with config.rs's inline tests as `config/tests/<section>.rs` |
| `config/machines.rs` |  |  | 39 | `machines` in the hub's config: whether machines update themselves to the build this hub |
| `config/providers.rs` |  |  | 239 | `auth.providers` and `auth.email` in hub mode: the ways besides a passkey that people |
| `config/shippin.rs` |  |  | 141 | `shippin` in the hub's config: how an account whose person said yes to the site's offers |
| `config/tenancy.rs` |  |  | 365 | `tenancy` and `operators` in the hub's config: whether this hub is one person's (or one |
| `config/workflows.rs` |  |  | 63 | `workflows` in the hub's config: how long the link of a workflow an AI starts for its |
| `connections/catalog.rs` |  |  | 292 | The catalog of outside services agents and apps call through the hub (plan: |
| `connections/mod.rs` |  |  | 390 | Connections: a tenant's outside services, called through the hub (plan |
| `connections/stored.rs` |  |  | 161 | A tenant's connections to outside services, kept in the store under its row-level security |
| `connections/tests.rs` |  |  | 288 | Connections over HTTP and MCP on a hosted hub, against a stand-in service: a person adds |
| `extensions.rs` |  |  | 303 | What a hosted edition adds, and what this core does when nothing is added. |
| `identity/accounts/api_keys.rs` | `identity/accounts/api_keys/mod.rs` | S5 | 567 | API keys: OAuth is the handshake a person does once; a key is what they hand an agent, a |
| `identity/accounts/api_keys/tests.rs` |  |  | 260 | API keys against the store and the limits they carry. |
| `identity/accounts/claim.rs` | `identity/accounts/claim/mod.rs` | S5 | 257 | Claiming a new hub from a browser (16c): a hub in `auth.mode hub` whose config names no |
| `identity/accounts/claim/tests.rs` |  |  | 309 | (no module doc) |
| `identity/accounts/clients.rs` |  |  | 59 | The AI clients a tenant has working (migration 0011): which families of AI client |
| `identity/accounts/email.rs` |  |  | 156 | Sign-in links sent by email. A link proves the mailbox only because it is delivered out |
| `identity/accounts/identities.rs` |  |  | 440 | Sign-ins an account is reached by besides its passkeys: Google, GitHub, an email link, |
| `identity/accounts/mod.rs` |  | S9 | 1263 | The hub's own accounts (D25, §14): people sign in with passkeys on the hub itself, so a; keeps Account, Passkey, Accounts and its rows; out: `links.rs` (LinkPurpose, link_token, issue_in, live_link), `ceremony.rs` (Ceremony, Authority, challenge_message, spend_challenge, stored_key, spent_horizon) |
| `identity/accounts/oauth.rs` | `identity/accounts/oauth/mod.rs` | S5 | 647 | The hub as its own OAuth 2.1 authorization server for AI clients (D25 §14, increment |
| `identity/accounts/oauth/device.rs` |  |  | 289 | Device authorization (RFC 8628) for the hub's own accounts: how `devagotchi login` |
| `identity/accounts/oauth/tests.rs` |  |  | 683 | (no module doc) |
| `identity/accounts/passkey.rs` | `identity/accounts/passkey/mod.rs` | S5 | 339 | Passkeys (WebAuthn), verified by the hub itself on `aws-lc-rs` (D25, §14). |
| `identity/accounts/passkey/soft.rs` |  |  | 188 | A software authenticator for tests: it holds a real key, builds the authenticator data |
| `identity/accounts/passkey/tests.rs` |  |  | 489 | A software authenticator (`soft.rs`) stands in for a phone. Every check the verifier |
| `identity/accounts/passwords.rs` | `identity/accounts/passwords/mod.rs` | S5 | 412 | Email and password sign-in on a hosted hub (spec first-connect, A1). Sign-up makes the |
| `identity/accounts/passwords/tests.rs` |  |  | 92 | (no module doc) |
| `identity/accounts/sessions.rs` | `identity/accounts/sessions/mod.rs` | S5 | 235 | Browser sessions on the hub's own accounts (32a-2): the browser holds a random id in a |
| `identity/accounts/sessions/tests.rs` |  |  | 327 | (no module doc) |
| `identity/accounts/sign_in_settings.rs` |  |  | 109 | Sign-in providers set from the hub's settings page, kept in the store so a person configures |
| `identity/accounts/tenants.rs` | `identity/accounts/tenants/mod.rs` | S5 | 544 | Tenants: each person's own space on a hub (the design adopted 2 Nov, "each user |
| `identity/accounts/tenants/tests.rs` |  |  | 768 | Tenants and accounts made on first connect, on the test Postgres (`DEVAGOTCHI_TEST_POSTGRES_URL`; CI sets it). |
| `identity/accounts/tests.rs` | `identity/accounts/tests/mod.rs` | S9 | 919 | (no module doc); over 800 lines: a `tests/` folder, a file per subject (rule 6) |
| `identity/auth.rs` |  |  | 686 | Bearer authentication for the hub: who is calling, proven by the identity |
| `identity/cli/mod.rs` |  |  | 181 | The identity domain's operator commands, run on the hub's own host with its config and |
| `identity/cli/tests.rs` |  |  | 128 | (no module doc) |
| `identity/connect/mod.rs` |  |  | 374 | `devagotchi hub connect-nomen`: the hub joins a Nomen with its owner's approval. |
| `identity/connect/tests.rs` |  |  | 415 | Against a fake Nomen that plays the device grant and the management API as Nomen does. |
| `identity/mod.rs` |  |  | 51 | Identity: who someone is on this hub and how they prove it: accounts, sign-in (email and |
| `identity/nomen/kept.rs` |  |  | 114 | Sign-ins Nomen confirmed, kept in the hub's Postgres (`auth_nomen_sign_ins`, migration |
| `identity/nomen/mod.rs` |  |  | 696 | Shippin sign-in: checking Nomen access tokens. |
| `identity/nomen/tests/kept.rs` |  |  | 88 | Sign-ins Nomen confirmed are kept in the hub's Postgres and honoured across a restart |
| `identity/nomen/tests/mod.rs` |  |  | 711 | Against a fake Nomen that checks the hub's private-key JWT the way Nomen does. |
| `identity/preferences/mod.rs` |  |  | 345 | What a person told the hub they want (increment 119): what to call them, how the app |
| `identity/preferences/tests.rs` |  |  | 239 | A person's preferences on SQLite, and on Postgres under row-level security when the test |
| `identity/sealed.rs` |  |  | 115 | Secrets the hub keeps in its database, sealed with AES-256-GCM under a key that lives only |
| `identity/tests/keys.rs` |  |  | 215 | API keys end to end over HTTP, the whole router as it serves: a person makes one on the |
| `identity/tests/me.rs` |  |  | 277 | The person's own page end to end over HTTP, the whole router as it serves (`web/app/me.rs`, |
| `identity/tests/mod.rs` |  |  | 3 | (no module doc) |
| `identity/tests/providers.rs` |  |  | 764 | Sign in with Google, GitHub, Shippin (Nomen as a generic OpenID Connect provider) and an |
| `lib.rs` |  | S6 | 1942 | `devagotchi hub`: the bastion. Authenticates every caller through the identity; keeps the `mod` lines and the three entry points (`cli`, `board_cli`, `serve_with`). Out: `USAGE`, `Command`, `parse_*`, `manage`, `read_config`, `prepare_state_dir` to `cli/mod.rs`; `AppState`, `run`, `app`, `accounts_at`, `configured_accounts` to `serve/mod.rs`; `require_bearer`, `challenge`, `protected_resource*`, `resource_metadata`, `resource_documents`, `log_refusal` to `identity/bearer.rs`; `link_upgrade`, `EnrollBody`, `enroll_with_code` to `machines/link/http.rs`; `healthz`, `reachable`, `log_unavailable` to `ops/health.rs`; `operator_event` to `ops/audit.rs`; its 560 lines of tests to each of those homes |
| `login/client.rs` |  |  | 391 | One MCP session with a hub, as the account on file: the client side of Streamable |
| `login/front.rs` |  |  | 147 | `devagotchi mcp --hub [URL]`: the account carried into an agent's MCP session (14c). |
| `login/mod.rs` |  | S9 | 1153 | The account this machine signs in with: `devagotchi login`, `logout`, `whoami` (D22).; keeps cli, usage, login, logout, whoami; out: `stored.rs` (Stored, Account, Standing, read, write, remove), `issuer.rs` (Issuer and its device and token answers), `hub.rs` (the hub_* calls) |
| `login/tests.rs` |  |  | 478 | Against a fake Nomen that plays the device-authorization grant the way Nomen does. |
| `machines/cli.rs` |  |  | 74 | `devagotchi hub enroll \| revoke \| devices`: the operator's machine commands, run on the |
| `machines/devices/mod.rs` |  |  | 470 | Machines enrolled to reach this hub over the link (plan N20), kept in the hub's Postgres |
| `machines/devices/tests.rs` |  |  | 320 | (no module doc) |
| `machines/downloads.rs` |  |  | 269 | The builds this hub hands to machines that join it (increment 29b): a directory laid |
| `machines/files.rs` |  |  | 460 | `read_file` and `list_directory`: a machine's files through its broker, one call each |
| `machines/join/mod.rs` |  |  | 492 | `/join/<code>`: the one line that adds a machine (increment 29b). |
| `machines/link/mod.rs` |  | S9 | 1142 | The hub's end of the machine link (plan N20). A broker on another machine is reached; keeps Links, Session, Waiter, Job; out: `admit.rs` (Admitted, fingerprint, wire_refusal, offline, RefusalLog), `frames.rs` (send, decode, revoked_frame, command_of, UpdateAsked) |
| `machines/link/registry.rs` |  |  | 136 | What the link offers the rest of the hub about machines' records: a machine's record, an |
| `machines/link/tests.rs` |  |  | 405 | (no module doc) |
| `machines/mod.rs` |  |  | 43 | Machines: the computers, servers and devspaces that reach this hub, and everything the hub |
| `machines/run.rs` |  |  | 183 | `run`: one call that starts a command on a machine, waits a bounded time for it, and |
| `machines/update.rs` |  |  | 154 | Machines update themselves to the build this hub hands out, and on request |
| `mcp/callers.rs` | `mcp/callers/mod.rs` | S5 | 183 | Who a broker is told is calling (the signed-in subject, in this MCP session), and the |
| `mcp/callers/tests.rs` |  |  | 42 | (no module doc) |
| `mcp/mod.rs` |  |  | 38 | MCP: the hub's tools as AI clients call them, at `/mcp` (Streamable HTTP): the machines |
| `mcp/output_schema.rs` |  |  | 295 | Test-only: a tool result's `structuredContent` checked against the output schema the tool |
| `mcp/surface.rs` | `mcp/surface/mod.rs` | S9 | 1080 | The hub's MCP surface: the broker's four handle-based tools, each routed to a; split: `mod.rs` (Hub, ServerHandler), `shapes.rs` (Routed, Machine, Machines, the *Routed inputs, MachineAdd/Update/Added, OrgRole, WhoAmI), `machines.rs` (the machine tools); `schema_tests` becomes `tests.rs` |
| `mcp/tests.rs` |  |  | 170 | The plugins in `plugins/` (Claude Code, Codex, and Grok through Claude's format), held to |
| `notify/mail.rs` |  |  | 539 | Sending one email over SMTP (RFC 5321): the hub's sign-in links (`web/email.rs`). Enough |
| `notify/mod.rs` |  |  | 36 | Notify: how the hub reaches people and other systems when something happens: Web Push to |
| `notify/push/mod.rs` |  |  | 477 | Web Push to the phones and browsers people turned notifications on from (doc 39e): |
| `notify/push/subscriptions.rs` |  |  | 78 | The phones and browsers a person turned notifications on from (Web Push), in |
| `notify/signals/mod.rs` |  |  | 588 | The signal contract, version 1 (increment 117, `docs/signals.md`): one stream of what |
| `notify/signals/tests.rs` |  |  | 343 | The signal contract: its published schema, and a sink over real sources in Postgres (the |
| `notify/tests.rs` |  |  | 27 | (no module doc) |
| `offers/mcp.rs` |  |  | 298 | `offers` and `offers_accept`: what the AI offers its person once, and their yes. Listed |
| `offers/mod.rs` |  |  | 137 | The site's offers (increment 73): the network a site belongs to and its free offerings |
| `offers/nomen.rs` |  |  | 235 | Making or finding a person's account on Nomen, the network's identity provider, through |
| `offers/stored.rs` |  |  | 304 | The site's offers in the store (migration 0012): one row per tenant whose person said yes, |
| `offers/tests.rs` | `offers/tests/mod.rs` | S9 | 844 | The site's offers end to end over HTTP on a hosted hub, the whole router as it serves; over 800 lines: a `tests/` folder, a file per subject (rule 6) |
| `ops/analytics/funnel.rs` |  |  | 98 | A tenant's milestones: each recorded once, with the tenant's id, where it happens. The |
| `ops/analytics/mod.rs` |  |  | 235 | First-party analytics for a hosted hub's operator (increment 66): the hub counts what it |
| `ops/analytics/report.rs` |  |  | 322 | What the operator's analytics page shows (`web/analytics.rs`), read from the store: each |
| `ops/analytics/tests.rs` |  |  | 574 | What a visit keeps and what is never counted, the day's visitor key, the daily totals and |
| `ops/analytics/visit.rs` |  |  | 196 | Which requests are visits, and what of one is kept (`analytics.rs` says why each). |
| `ops/analytics/writer.rs` |  |  | 325 | The one task that writes what is counted: batches of visits and milestones off the |
| `ops/analytics_settings.rs` |  |  | 97 | Where AgentHub's board events go for statistics, set from the hub's settings page and kept in |
| `ops/audit.rs` |  |  | 328 | Append-only, hash-chained audit of every routed call, in the hub's Postgres |
| `ops/mod.rs` |  |  | 38 | Ops: what the hub records about itself and reports to its operator: the audit chain of |
| `ops/tests.rs` |  |  | 452 | First-party analytics on a hosted hub end to end over HTTP, the whole router as it serves |
| `ops/warehouse/mod.rs` |  |  | 155 | AgentHub's statistics over HTTP, from ClickHouse. The export itself is |
| `ops/warehouse/tests.rs` |  |  | 327 | The export against a stand-in ClickHouse that can be up, down, or take a send and lose |
| `site.rs` | `site/mod.rs` | S9 | 717 | The hub's site: every word, name, link, colour and picture its pages, its sign-in email; after #221; its `include_str!` paths change with it |
| `site/analytics_copy.rs` |  |  | 143 | The analytics page's copy (`content/messages.json` `analytics`), each message with what |
| `site/assets.rs` |  |  | 201 | The site's files in `assets/`: pictures, the video, fonts. Each is served from this hub |
| `site/brand_tests.rs` | `site/tests/brand.rs` | S9 | 392 | No brand, domain or operator in the hub's code: every name a person or an AI reads comes |
| `site/discovery.rs` |  |  | 410 | What search engines and AI agents read about a hub, made from its site and its public |
| `site/markdown.rs` |  |  | 258 | The public pages (`content/*.md`): a small, strict Markdown. Each page opens with its |
| `site/model.rs` | `site/model/mod.rs` | S9 | 1651 | What the site's JSON files hold, and the checks a value must pass before the hub; split by the JSON it reads: `mod.rs` (SiteFile, Front, Variants, AssetNames), `product.rs` (Software, Offer, Product, Operator, SignIn, Mcp), `plans.rs` (Plan, Limit, Unlimited, Service, EarlyAccess, SignUp), `layout.rs` (Layout, SectionEntry), `landing.rs` (Landing to Footer and the popups), `messages.rs` (Messages, DelegateCopy) |
| `site/offers.rs` |  |  | 134 | `offers` in `site.json`: the network a site belongs to and the free offerings it links an |
| `site/plans.rs` |  |  | 287 | The site's plans and early access as the hub enforces them (`tenancy/access/`), and |
| `site/style.rs` |  |  | 275 | `tokens.json`: the design tokens the stylesheet reads as custom properties, and the |
| `site/tests.rs` | `site/tests/mod.rs` | S9 | 843 | The site (`site.rs`): the built-in one checks out and says what the hub always said; a |
| `site/text.rs` |  |  | 448 | The markup a line of the site's copy may carry, and the placeholders it may name. |
| `site/watch.rs` |  |  | 113 | The running hub follows its site directory: a change to it is read and checked, and |
| `site/workflows.rs` |  |  | 574 | `content/workflows.json`: the step-by-step setups an AI can start for its person and hand |
| `store.rs` | `store/mod.rs` | S5 | 94 | The hub's store: the Postgres engine `devagotchi-store` (`platform/store`), re-exported so the |
| `store/tests.rs` |  |  | 269 | (no module doc) |
| `tenancy/access/mod.rs` |  |  | 403 | Who is let in, and on which plan (increment 63). A hosted hub (`tenancy.mode multi`) |
| `tenancy/access/plans.rs` |  |  | 123 | What a tenant's plan lets it hold: setting a tenant's plan (the operator), the room left on |
| `tenancy/access/tests.rs` |  |  | 495 | Early access and plans in the store, on the test Postgres (`DEVAGOTCHI_TEST_POSTGRES_URL`; CI sets it). |
| `tenancy/gate.rs` |  |  | 104 | Early access and plans where a caller acts (`tenancy/access/` keeps them): a tenant |
| `tenancy/machines.rs` |  |  | 195 | Machine names on a hosted hub (`tenancy.mode multi`) are each tenant's own: two people |
| `tenancy/mod.rs` |  |  | 40 | Tenancy: many people's spaces on one hub, run for others as a hosted service: who is let |
| `tenancy/operator/mod.rs` |  |  | 138 | What the operator page reads about each tenant (increment 64, `web/operator.rs`): who is |
| `tenancy/operator/tests.rs` |  |  | 111 | What the operator page reads, on the test Postgres (`DEVAGOTCHI_TEST_POSTGRES_URL`; CI sets it). |
| `tenancy/orgs/api/mod.rs` |  |  | 521 | `/api/v1/orgs/…`, `/api/v1/invites/accept` and `/api/v1/projects/…/members`: the |
| `tenancy/orgs/api/projects.rs` |  |  | 271 | A project of an organisation: making one, its visibility, and who works on it, by |
| `tenancy/orgs/mod.rs` |  | S9 | 1548 | Organisations, their members and teams, and who works which project (increment 114).; keeps Orgs; out: `model.rs` (Role, Org, Member, Team, Principal, ProjectMember, Visibility, Project, Invite, NewInvite, ProjectRef), `checks.rs` (check_*, role_of, token_hash), and the `impl Orgs` by subject into `members.rs`, `teams.rs`, `projects.rs`, `invites.rs` |
| `tenancy/orgs/tests.rs` |  |  | 434 | Organisations on the test Postgres (`DEVAGOTCHI_TEST_POSTGRES_URL`, as CI's |
| `tenancy/tests/access.rs` |  |  | 423 | Early access and plans on a hosted hub end to end over HTTP, the whole router as it |
| `tenancy/tests/mod.rs` |  |  | 3 | (no module doc) |
| `tenancy/tests/orgs.rs` |  |  | 435 | Organisations end to end over HTTP, the whole router as it serves (`orgs`, `orgs::api`, |
| `tenancy/tests/tenants.rs` |  | S9 | 898 | A hosted hub (`tenancy.mode multi`) end to end over HTTP, the whole router as it serves:; over 800 lines: a `tests/` folder, a file per subject (rule 6) |
| `test_keys.rs` |  |  | 71 | A fresh RSA key per test run, so no private key is ever committed. |
| `web/analytics.rs` | `ops/web/analytics.rs` | S8 | 607 | `/operator/analytics`: what a hosted hub counted for its operator (`crate::analytics`), |
| `web/analytics_tests.rs` | `ops/web/tests.rs` | S8 | 235 | The operator's analytics page (`/operator/analytics`) on a hosted hub, the whole router |
| `web/app/agenthub/board.rs` | `agenthub/web/board.rs` | S8 | 272 | `/app/boards/<project>`: a board's open tickets, read with `Board::list`. Filters are |
| `web/app/agenthub/mod.rs` | `agenthub/web/mod.rs` | S8 | 10 | AgentHub's screens in the phone app: a board (`/app/boards/{project}`), a ticket |
| `web/app/agenthub/ticket.rs` | `agenthub/web/ticket.rs` | S8 | 414 | `/app/tickets/<project>/<id>`: one ticket — title, state, priority, labels, who holds it |
| `web/app/agenthub/ticket_actions.rs` | `agenthub/web/ticket_actions.rs` | S8 | 250 | What the ticket page's forms do (`ticket.rs`): `POST /app/tickets/<project>/<id>/<action>` |
| `web/app/agenthub/ticket_tests.rs` | `agenthub/web/tests/ticket.rs` | S8 | 510 | The board and ticket pages on a hosted hub over HTTP: filters and search, and each action |
| `web/app/approval_tests.rs` | `agenthub/web/tests/approvals.rs` | S8 | 143 | Approvals on an account's own board, on a hosted hub over HTTP (`tenancy/tests/tenants.rs`'s |
| `web/app/connections.rs` | `connections/web.rs` | S8 | 261 | `/app/connections`: the outside services this account's agents call through the hub. Add |
| `web/app/home.rs` |  | S8 | 340 | `/app`, the app's home: three cards — Needs you (approvals waiting, tickets assigned to; stays: cards drawn from several domains |
| `web/app/keys.rs` | `identity/web/keys.rs` | S8 | 302 | `/app/keys`: a person's API keys. Make one with what it may do, where, and for how long; see |
| `web/app/machine.rs` | `machines/web/machine.rs` | S8 | 192 | `/app/machines/<name>`: one machine — its status, build and when it was last seen, |
| `web/app/me.rs` | `identity/web/me.rs` | S8 | 413 | `/app/me`, the person's own page: who is signed in and in which organisations, this |
| `web/app/mod.rs` |  | S8 | 230 | The phone control app (`docs/spec-phone-app.md`): server-rendered pages in one fixed; stays: the phone app's shell |
| `web/app/tests.rs` |  | S8 | 433 | The phone app's pages on a hosted hub over HTTP (`tenancy/tests/tenants.rs`'s world): each page; stays: the phone app over HTTP; per-page cases move with their page |
| `web/approvals.rs` | `agenthub/web/approvals.rs` | S8 | 713 | `/approvals`: the requests agents are waiting on, one tap to approve or deny, laid out |
| `web/claim.rs` | `identity/web/claim.rs` | S8 | 150 | `/claim`: a new hub claimed from a browser (16c). Whoever has the claim code the hub |
| `web/claim_tests.rs` | `identity/web/tests/claim.rs` | S8 | 273 | A new hub claimed from a browser end to end over HTTP (16c): the landing page offers the |
| `web/connectors.rs` | `identity/connect/web.rs` | S8 | 233 | `/connect`: how to connect each AI client to this hub, with this hub's own address filled |
| `web/counted.rs` | `ops/web/counted.rs` | S8 | 57 | Counting visits to the public pages as the hub serves them (`crate::analytics`): no |
| `web/device.rs` | `identity/web/device.rs` | S8 | 367 | Device authorization (RFC 8628) on a hub with its own accounts: `devagotchi login` |
| `web/discovery.rs` | `site/web/discovery.rs` | S8 | 77 | The routes search engines and AI agents read (`site/discovery.rs` makes their words): |
| `web/discovery_tests.rs` | `site/web/tests/discovery.rs` | S8 | 696 | What search engines and AI agents read, over HTTP on a hosted hub with the whole router |
| `web/email.rs` | `identity/web/email.rs` | S8 | 458 | Sign in with a link sent by email (`auth.email`), on a hub with its own accounts. The link |
| `web/hosted.rs` | `tenancy/web/hosted.rs` | S8 | 130 | The way into a hosted hub (`tenancy.mode multi`): nobody claims it, and nobody fills in a |
| `web/hub_settings.rs` | `settings/web.rs` | S8 | 733 | `/admin`: the hub's own settings, for its owner, grouped by what they are for and guided |
| `web/hub_settings_tests.rs` | `settings/tests.rs` | S8 | 542 | The hub's settings page over HTTP, the whole router as it serves: only the owner sees it, a |
| `web/landing.rs` | `site/web/landing/mod.rs` | S8 | 677 | `/` for someone not signed in: the site's product page, served by every hub. Its words |
| `web/landing/compare.rs` | `site/web/landing/compare.rs` | S8 | 85 | Another product against this one, row by row, and each row's evidence in a popup |
| `web/landing/network.rs` | `site/web/landing/network.rs` | S8 | 36 | The network section (`network`): the network the site belongs to, its free offerings, and |
| `web/landing/popups.rs` | `site/web/landing/popups.rs` | S8 | 255 | The landing page's popups beside the comparison's evidence (`compare.rs`): how claiming |
| `web/landing_tests.rs` | `site/web/landing/tests.rs` | S8 | 495 | The landing page (`landing.rs`): every section is there for a claimed hub, a new hub |
| `web/machines.rs` | `machines/web/mod.rs` | S8 | 227 | "Add a machine" and "Revoke" on the Machines page (increment 29a). A signed-in person |
| `web/media.rs` | `site/web/media.rs` | S8 | 128 | The site's files under `/assets/` (`site/assets.rs`): its logo, icon, pictures, setup |
| `web/media_tests.rs` | `site/web/tests/media.rs` | S8 | 194 | The site's files (`media.rs`), here the built-in setup video and its poster: served whole |
| `web/mod.rs` |  | S8 | 232 | People's view of the hub in a browser: sign in, and see who you are signed in as and; stays: the shell: the bar, safe return paths, cookies |
| `web/nomen_login.rs` | `identity/web/nomen_login.rs` | S8 | 407 | Signing in on the hub's pages with Nomen (`auth.mode nomen` with a `web` section): the |
| `web/notify.rs` | `notify/web.rs` | S8 | 141 | Turning a phone into an approval notifier: the service worker, the one page script, |
| `web/oauth.rs` | `identity/web/oauth.rs` | S8 | 751 | The hub's own OAuth 2.1 authorization server for AI clients (32b): what Claude, |
| `web/offers.rs` | `offers/web.rs` | S8 | 177 | `/offers/<token>`: the one-tap link `offers_accept` hands an AI when the hub cannot link |
| `web/operator.rs` | `tenancy/web/operator.rs` | S8 | 428 | `/operator`: where a hosted hub's operator lets people in and sets their plans |
| `web/operator_tests.rs` | `tenancy/web/tests/operator.rs` | S8 | 294 | The operator page (`/operator`) on a hosted hub, the whole router as it serves: only the |
| `web/orgs.rs` | `tenancy/web/orgs/mod.rs` | S8 | 1082 | The organisation pages (increment 116): `/orgs` lists a person's organisations and makes; split: `mod.rs` (routes, list, create, show), `members.rs` (set_member, remove_member), `teams.rs` (create_team, delete_team, team_member), `projects.rs` (create_project, set_visibility, project_member), `invites.rs` (create_invite, revoke_invite, invite_page, accept) |
| `web/pages.rs` |  | S8 | 751 | The hub's few HTML pages: one stylesheet from the hub itself, every value escaped, and; stays: the HTML kit every page uses |
| `web/passkey_tests.rs` | `identity/web/tests/passkeys.rs` | S8 | 1534 | A hub with its own accounts end to end over HTTP: the owner's link makes a passkey,; split into `identity/web/tests/passkeys/` by the same four subjects |
| `web/passkeys.rs` | `identity/web/passkeys/mod.rs` | S8 | 1112 | Passkey sign-in on the hub's own pages (`auth.mode hub`, D25 §14): the owner's one-time; split: `mod.rs` (HubSignIn), `owner.rs` (owner_page, owner_password*), `register.rs` (register_options, unsecured_session, securing, register, secure_with), `sign_in.rs` (sign_in_options, sign_in, login_page, login) |
| `web/providers.rs` | `identity/web/providers.rs` | S8 | 949 | Sign in with Google, GitHub or an OpenID Connect provider (Nomen is one), on a hub with |
| `web/public.rs` | `site/web/public.rs` | S8 | 129 | The hub's public pages: what the site's product is and how to connect an AI (`/docs`), |
| `web/routes.rs` |  | S8 | 687 | The browser routes: the landing page and claiming a new hub (`/claim`), sign in, the; keeps `router`, which merges each domain's `web::routes()`; its sign-in handlers (login, callback, account, logout, signed_out, me, set_password) move to `identity/web/account.rs` |
| `web/session.rs` | `identity/web/session.rs` | S8 | 76 | Into an account once a sign-in proved it (`providers::finish`, `sign_in.rs`): the |
| `web/sessions.rs` | `identity/web/sessions.rs` | S8 | 395 | Browser sessions of the Nomen web login that outlast a restart, so a phone that turned on |
| `web/sign_in.rs` | `identity/web/sign_in.rs` | S8 | 353 | A hosted hub's sign-in page (spec first-connect, A2): Continue with Google (and any other |
| `web/site_tests.rs` | `site/web/tests/mod.rs` | S8 | 187 | A hub drawn with a neutral site (`tests/fixtures/neutral-site`): every public page, the |
| `web/tests/machines.rs` | `machines/web/tests.rs` | S8 | 355 | A person's own machines through the web: add one and revoke it, the one line that joins |
| `web/tests/mod.rs` |  | S8 | 1204 | The browser sign-in end to end against a fake Nomen token endpoint: the hub's real; keeps the whole-router harness; its sign-in cases move to `identity/web/tests/sign_in.rs` |
| `web/tests/nomen_login.rs` | `identity/web/tests/nomen_login.rs` | S8 | 444 | Signing in with Nomen on the hub's pages: the round trip, its refusals and outages, where |
| `web/workflow.rs` | `workflows/web/mod.rs` | S8 | 1086 | `/go/<token>`: the page a workflow's link opens (increment 68, `crate::workflows`). The AI; split: `mod.rs` (routes, page, View, status), `cookies.rs` (secure, code/browser cookies, browser_secret), `actions.rs` (command, phone, client, keep) |
| `workflows/mcp.rs` |  |  | 531 | The workflow tools: what an AI offers its person, starts for them as one link, follows |
| `workflows/mod.rs` |  |  | 541 | Workflows with direct links (increment 68): the AI does the work, its person just clicks. |
| `workflows/status.rs` |  |  | 53 | What a workflow and each of its steps look like to the AI that follows them |
| `workflows/stored/mod.rs` |  |  | 625 | Workflows in the store (migration 0010): one row per workflow an AI started for its |
| `workflows/stored/tests.rs` |  |  | 55 | (no module doc) |
| `workflows/tests.rs` | `workflows/tests/mod.rs` | S9 | 1766 | Workflows end to end over HTTP, the whole router as it serves (increment 68,; over 800 lines: a `tests/` folder, a file per subject (rule 6) |

## platform/ipc

| Now | Goes to | Step | Lines | Job, and what is split out |
|---|---|---|---|---|
| `lib.rs` |  |  | 119 | Private per-user local IPC. This authorizes an OS user, not individual applications. |
| `unix.rs` |  |  | 281 | Unix local IPC: a private 0700 state directory, an exclusive `flock` |
| `windows.rs` | `windows/mod.rs` | S10 | 415 | Windows local IPC: a protected state directory private to the operator's |
| `windows/security.rs` | `windows/security/mod.rs` | S10 | 516 | Win32 identity and security-descriptor checks for private broker state.; split: `sid.rs` (Sid, Operator, token_*, process_user), `descriptor.rs` (SecurityDescriptor, Ace, ObjectSecurity, object_security), `private.rs` (create_private_*, open_entry, Entry, private_entry_defect) |

## runtime/link

| Now | Goes to | Step | Lines | Job, and what is split out |
|---|---|---|---|---|
| `handoff.rs` |  |  | 192 | Handing a checked update to something that outlives this link and the broker it replaces |
| `idle.rs` |  |  | 61 | Waiting until the broker runs no jobs before an update replaces it (spec U3 step 1): |
| `lib.rs` |  |  | 288 | `devagotchi link`: lets a Devagotchi hub reach the broker on this machine without |
| `session.rs` | `session/mod.rs` | S10 | 283 | One connection to the hub: prove the device key over the hub's challenge, then serve |
| `session/tests.rs` |  |  | 36 | (no module doc) |
| `state.rs` |  |  | 359 | The link's private state: which hub, which device name, which local broker, and the |
| `update.rs` | `update/mod.rs` | S10 | 235 | The link updates its machine to the hub's build (docs/spec-machine-update.md, U3): wait |
| `update_tests.rs` | `update/tests.rs` | S10 | 122 | The updater's own logic: the idle wait against a fake broker, the hub's checksum list, |

## engine/machine-engine

| Now | Goes to | Step | Lines | Job, and what is split out |
|---|---|---|---|---|
| `ai/mod.rs` |  | S10 | 1017 | Private local development evidence. Never a replacement for the shared board.; keeps cli and run; out: `policy.rs` (Policy, Mode, Permission, Capture), `journal.rs` (Journal, entry_hash, authorize_transcript, validate_event, normalize), `contracts.rs` (schema, repo_root, check_contracts, git, delivery) |
| `ai/tests.rs` | `ai/tests/mod.rs` | S10 | 647 | (no module doc); by the same three subjects |
| `broker.rs` | `broker/mod.rs` | S10 | 1336 | Broker admission and the operation lifecycle: durable admissions,; split: `config.rs` (ExecPolicy, BrokerConfig, defaults), `operations.rs` (Operation, State), `input.rs` (key_bytes, input_steps, deliver, input_closed), `paths.rs` (canonical_existing, path_string, is_directly_executable), `shared.rs` (Shared); `mod.rs` keeps Broker |
| `broker/access.rs` |  |  | 100 | Who reaches which operation. Every request but a list names its operation, and only its |
| `broker/tests/access.rs` |  |  | 100 | Who reaches an operation: its own account (any of its conversations), this machine's own |
| `broker/tests/interactive.rs` |  |  | 444 | Interactive operations: the input rules on every platform; programs on a pipe and on a |
| `broker/tests/lifecycle.rs` |  |  | 416 | Admission and the operation lifecycle: output and exit status, the account policy, |
| `broker/tests/mod.rs` |  |  | 154 | (no module doc) |
| `broker/tests/waits.rs` |  |  | 143 | Waits: on settlement, on new output, timing out, the operations revision, the cap. |
| `execution.rs` | `execution/mod.rs` | S10 | 482 | Process orchestration for one admitted operation: spawn the exact argv,; after #222 |
| `execution/detached.rs` |  |  | 263 | Starting a process detached from this one: what `devagotchi mcp` uses to start this |
| `execution/input.rs` |  |  | 100 | An interactive job's input: the steps one `operation_input` becomes, and the task that |
| `execution/platform.rs` | `execution/platform/mod.rs` | S10 | 15 | Native ownership of one root's process tree: the Linux root's inherited; after #222 |
| `execution/platform/file_identity.rs` |  |  | 39 | Which file object an open file is, for a read's `identity`: device and inode on Unix, |
| `execution/platform/git_bash.rs` |  |  | 249 | Which Windows command lines run in Git Bash rather than PowerShell, and where Git for |
| `execution/platform/linux.rs` | `execution/platform/linux/mod.rs` | S10 | 519 | Linux process ownership: the root's inherited process group.; after #222 |
| `execution/platform/linux/terminal.rs` |  |  | 180 | A pseudo-terminal for an interactive operation (`InputMode::Terminal`): the broker holds |
| `execution/platform/linux/tests.rs` |  |  | 187 | (no module doc) |
| `execution/platform/windows.rs` | `execution/platform/windows/mod.rs` | S10 | 630 | Windows process ownership: a broker-owned Job Object per root.; after #222 |
| `execution/platform/windows/process.rs` |  |  | 288 | Creating a process by hand (`CreateProcessW`), for what `std::process::Command` cannot do |
| `execution/platform/windows/terminal.rs` |  |  | 340 | A pseudo-console (ConPTY) for an interactive operation (`InputMode::Terminal`): the broker |
| `execution/platform/windows/tests.rs` |  |  | 209 | (no module doc) |
| `files.rs` | `files/mod.rs` | S10 | 336 | Files on the broker's machine: bounded reads and directory listings, answered as direct |
| `files/lines.rs` | `files/lines/mod.rs` | S10 | 307 | Reading a text file by lines, as Desktop Commander's `read_file` does: from a 1-based |
| `files/lines/tests.rs` |  |  | 212 | (no module doc) |
| `files/manage.rs` | `files/manage/mod.rs` | S10 | 366 | Managing files: what a path is (`info`), making directories (`mkdir`, and the parents a |
| `files/manage/tests.rs` |  |  | 374 | (no module doc) |
| `files/tests.rs` |  |  | 237 | (no module doc) |
| `files/write.rs` |  |  | 382 | Changing files: write (overwrite, create, append) and edit (exact text replacement). |
| `journal.rs` | `journal/mod.rs` | S10 | 429 | Durable broker records. Every mutation commits with SQLite synchronous=FULL. |
| `journal/tests.rs` |  |  | 154 | (no module doc) |
| `lib.rs` |  |  | 19 | The Devagotchi machine engine: admission and operation lifecycle (`broker`), |
| `service/background.rs` |  |  | 349 | This user's broker in the background, so an AI client's whole setup is `devagotchi mcp` |
| `service/mod.rs` |  |  | 218 | The broker runs independently from the MCP client process. This library is its |

## interfaces/mcp

| Now | Goes to | Step | Lines | Job, and what is split out |
|---|---|---|---|---|
| `adapter.rs` | `adapter/mod.rs` | S10 | 561 | The tools a local AI client gets: the hub's own set on this machine (one-call `run`, the |
| `adapter/terminal.rs` |  |  | 74 | Typing into an interactive operation and resizing its terminal: an operation started |
| `adapter/tests.rs` |  |  | 19 | (no module doc) |
| `connect.rs` | `connect/mod.rs` | S10 | 802 | `devagotchi connect`: point this machine's AI clients at Devagotchi in one command.; split: `mod.rs` (Options, parse, cli, run, check, answer), `clients.rs` (Client, Target, claude_code, codex, claude_desktop, find_command), `config_files.rs` (codex_config, desktop_config, toml_string, replace, backed_up), `tests.rs` |
| `files.rs` |  |  | 147 | `read_multiple_files`: several files in one call, over whatever reaches the broker (this |
| `image.rs` | `image/mod.rs` | S10 | 265 | `read_image`: an image file handed to the AI as an image it can see, over whatever |
| `image/tests.rs` |  |  | 281 | (no module doc) |
| `lib.rs` |  |  | 117 | Thin MCP stdio adapter. The broker owns admission, validation, and job lifetime. |
| `run.rs` | `run/mod.rs` | S10 | 207 | `run`: one call that starts a command, waits a bounded time for it, and returns its |
| `run/tests.rs` |  |  | 319 | (no module doc) |

## platform/store

| Now | Goes to | Step | Lines | Job, and what is split out |
|---|---|---|---|---|
| `lib.rs` |  | S10 | 523 | The Devagotchi store (`devagotchi-store`): the Postgres engine the hub and AgentHub share; keeps the `mod` lines; Store, Lookup and `impl Store` to `store/mod.rs`, Failed and its conversions to `failed.rs`, write_private and random to `files.rs` |
| `settings.rs` |  |  | 76 | Where the store is, as a person or a deploy names it: its URL (the environment's, else the |
| `testing.rs` |  |  | 133 | Stores for a test: a fresh database on the test Postgres that |
| `tests.rs` |  |  | 73 | (no module doc) |

