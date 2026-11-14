# Passkeys only after the first connection works

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json) (alpha, pre-release)
- Issue / request: [spec-first-connect](../../../docs/spec-first-connect.md) (approved by Jesse
  2026-11-04: "No Connect screen, no passkey, no 'secure your account'"); operator,
  2026-11-14: "passkeys should not be on a new account, it should be after they have
  established the connection".
- PR: one (F1-F4)
- Branch / baseline: from `5c28f8c` (v0.1.0)
- Objective: nobody meets the word passkey before their first machine is online, on the hosted
  hub or on a hub they host themselves. After that, the account page offers one.
- Non-goals: removing passkeys (they stay a sign-in method once offered); the storage move
  ([postgres-only-hub](postgres-only-hub.md)).

## What is true today (checked 2026-11-14)

| Claim | How it was checked |
|---|---|
| Hosted sign-in follows the spec | `curl https://app.devagotchi.com/auth/login`: Sign in, Sign up, Forgot password?, no "passkey" |
| The spec's end-to-end test exists and runs | `tests/first_connect_browser.py`, `.github/workflows/ci.yml:251` |
| The spec's live check was never added | `tests/live_check.py` has no "passkey" or "Sign up" check |
| The public landing page still tells new people to secure their account with a passkey | `curl https://app.devagotchi.com/`: 16 matches, e.g. "you secure it with a passkey", "Until you add a passkey on your account page ... nothing can recover it" |
| The site copy carries it | `crates/hub/site/default/content/`: `landing.json` 21, `messages.json` 2, `docs.md` 2, `llms.md` 2 |
| A self-hosted hub is claimed passkey first | landing copy "Name the owner and make the passkey"; `deploy/compose/README.md` "Claim this hub and make my passkey" before "Use a local email and password instead"; the spec kept the passkey page for one-owner hubs, which the 2026-11-14 rule overrides |

## Milestones and tasks

| Milestone | Task | Dependencies | Acceptance evidence | State |
|---|---|---|---|---|
| F1 | Copy: the landing page, docs, llms and messages say what happens (sign in with Google or email and password; your AI connects; your machine joins). No passkey, no "secure", no "nothing can recover it" | none | site tests; `grep -i passkey` over `site/default/content` lists only the account-page offer (F3) | not started |
| F2 | Self-hosted claim: one form, name, email and password; the claim code ends when it is set. No passkey button on `/claim` or the owner link | none | `claim_tests`; `tests/deploy_scripts.sh` claim step | not started |
| F3 | The offer: the account page shows "Add a passkey" only once the account has a machine that has been online; never before, never as a banner or prompt elsewhere | none | web test: account without machine has no "passkey"; with one, the offer is there | not started |
| F4 | Guards: `tests/live_check.py` checks `/`, `/auth/login` and `/claim` hold no "passkey"; `first_connect_browser.py` asserts no "passkey" on every page before the machine is online, on a hosted and a self-hosted hub | F1-F3 | CI run; live check against app.devagotchi.com after deploy | not started |

## Risks and approved decisions

Approved by the spec and the operator's words above. Pre-release: owner accounts that exist
only with a passkey keep it; nothing is migrated.

## Verification receipts

None yet.

## Delivery

Implementation: not started. Validation: not started. Push: not_verified.
PR: not_created. Main: not_verified.
**DELIVERY INCOMPLETE: NOT STARTED**
