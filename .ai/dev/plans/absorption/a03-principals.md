# Principal administration source review

Pinned S1 `46419c697964016eb547c60416a8f2f5e854a95f`: read all of
`server/internal/httpapi/admin.go`, `store/admin_people.go`, `store/principals.go`,
and the configuration/deletion dispatch in `admin_config.go` and `admin_misc.go`.
This is partial local authoring on the existing absorption branch, not source closure.

## Source operation mapping

| Source behavior | Current implementation / disposition |
| --- | --- |
| Principal list and role membership | `identity/accounts/agents/roster.rs`; owner sees the environment, other people see their account. Stable account/id cursor and role filter, 100 results with lookahead. `/api/v1/principals` and `/app/agents` share it. Human accounts remain in `/admin/users`, owned by `accounts/people`. |
| Principal create/update | `agenthub/http/principals/mod.rs` calls the existing receipt-aware `Accounts::put_agent_requested`. Current manager authority, catalog validation and CAS stay in identity. Caller cannot write `is_human`, login or credential identity. |
| Token issue/list/revoke | `identity/accounts/api_keys/managed.rs` uses the existing hash-only key store and immutable agent binding. Live manager/target and owner policy checked in the transaction. Cross-account owner actions record their actual actor. No receipt or request key for issuance; metadata reads never return the secret. Revocation of one's own key shares the browser operation/input for cross-surface replay. |
| Versioned roles/rules/settings | `agenthub/http/principals/configuration.rs` delegates to `agents/configuration.rs`; no SQL in HTTP. Strict fields, matching path/body IDs and header/body versions; deletion requires If-Match, and roles in use refuse. |
| Human login and authority | Password/passkey/provider browser login remains with existing identity. Agent OAuth/key tokens and seat headers are refused on administration routes. A kind named human is only a profile selector. No shared human bearer-login store is introduced. Tailnet/proxy-specific adapter disposition remains open. |
| Aggregate config, workflows, projects, other settings | Still open. The authored `/principals/configuration` response does not pretend to be source `/config`. Project retirement must preserve audit/history and invalidate cached handles; completed tasks also block deletion. |

The pinned `mcptools/server.go` registers read/lease/edit tools, with extension dispatch.
`mcptools/read.go` provides whoami/task reads; `ext.go` supplies extension registration.
Searching every pinned `server/internal` file for principal/role/policy mutation tool
names finds their store functions, not MCP registrations. Principal administration was
an HTTP surface. Do not invent new MCP admin tools merely to satisfy a presumed source
surface; the actual extension tool registries remain part of A04-A08 review.

The current edition keeps credential management human-only, including metadata reads,
consistent with the existing account key page. Source nonhuman self-token metadata reads
do not create a new credential-management permission. Human account editing remains
separate from registered-agent editing so an agent field cannot create a human identity.

## Authored checks and remaining acceptance

`agenthub/http/tests/principals.rs` exercises real HTTP plus PostgreSQL: versions,
request replay/conflict, editor visibility, forbidden human-field injection, origin
refusal with no mutation, once-only secret issuance, bound identity, metadata-only reads,
agent credential refusal, key revocation/replay, in-use role refusal, rule/settings edits,
same-ID agents across accounts, roster cursors, owner/member boundaries and browser links.

The combined local run covered 28 HTTP, profile-domain and browser checks: 27 passed;
one new test mistakenly required API no-store/JSON headers on an earlier middleware 403.
The fixture is corrected to assert that refusal and absence of a created profile.
The first compile also caught a SQL-error-to-transaction-error conversion, now fixed.
Final focused verification passed all four tests, including the corrected origin refusal
and cross-surface revocation replay. Only test fixtures changed after the 27 passing
regressions; runtime code is unchanged. Commands:

- `cargo test --locked -p devagotchi-hub --lib -- agenthub::http::tests web::app::agents identity::accounts::agents --test-threads=2`
- `cargo test --locked -p devagotchi-hub --lib agenthub::http::tests::principals -- --test-threads=2`

Both required real PostgreSQL. Retained logs are under
`target/absorption-principal-administration*`. The last roster check took longer than its
previous run but passed; the final run completed in 85.65 seconds with no ignored tests.
Eight layout checks and the 100-commit/907-row source-inventory Git-object check passed.

Still required: large-page boundaries, native browser viewport acceptance, full disabled
account/RLS concurrency coverage, remaining project/configuration operations and all later
packets. No branch is closed and no source commit is marked absorbed by these checks.
