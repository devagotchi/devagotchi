# Dormant machines and signing back in by a link

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json) (alpha = pre-release)
- Issue / request: operator, 2026-11-16: "renew login session where the user can ride a link to
  approve re-auth if they have already established that machine before on devagotchi"; and, on
  how: "the UI should allow the user to configure this aspect: dormant automatically removed
  after X or only moved to inactive; should be able to have a link to phone push, email, and
  email link".
- PR: this plan, then one PR per milestone (M1 onward), each reviewed.
- Branch / baseline: `increment-174/dormancy-plan`, from `main` at `5104f35`
- Objective: something the person already set up (a machine, a terminal they signed in on, an
  AI app they connected) that went unused is never a dead end and never a silent risk: it goes
  inactive or away as they chose, and when it comes back, getting it working again is one
  approval they receive where they chose (phone push, email, or an email link), with no
  re-enrolling and no codes to type.
- Non-goals: changing how a machine is first enrolled or an AI app first connected; recovering
  an account whose owner lost every sign-in (that is `devagotchi hub set-password`, #212).
- Canonical architecture / related plan: [free edition](free-edition.md) (D1 sign-in),
  [mobile experience](mobile-experience.md) (push), [hub settings](hub-settings.md),
  [layout pattern](../../patterns/layout.md).

## What exists today (read in the code, 2026-11-16)

| What signs in | How long it lasts | What happens when it lapses | Where |
|---|---|---|---|
| A machine (its link: its own key, enrolled once) | Forever, until revoked | Nothing lapses; an unused machine stays enrolled with full reach | `machines/devices` (`machine_devices`: name, key, enrolled, revoked, owner; no last-seen kept) |
| A terminal (`devagotchi login`, device code) | Access 15 minutes, refreshed; refresh 7 days unused | The CLI says expired; the person runs `devagotchi login` again and types a code | `login/`, `identity/accounts/oauth/device.rs`, `auth.tokens` |
| An AI app (OAuth, "Connect Claude?") | Refresh 7 days unused | The AI app asks the person to connect again from scratch | `web/oauth.rs`, `auth.tokens` |
| A browser session | 30 days unused, 90 at most | Sign in again with the passkey or password | `identity/accounts/sessions.rs` |
| A hosted account never secured with a passkey | `tenancy.unsecured.idle_days` (14), deploy-time only | It ends | `config/tenancy.rs`, `identity/accounts/tenants.rs` |

So: a machine never goes dormant (a stolen laptop's link keeps working until someone revokes it
by hand), and everything else that lapses makes the person start over.

## The design (operator to approve)

### One rule per account, set on a page

On the account's settings (the phone app's own page and `/account`), a section "When something
goes unused", with a line per kind (machines, terminals, AI apps):

- **After** N days unused (choices 7, 14, 30, 90; default 30 for machines, 7 for terminals and
  AI apps, as today).
- **Then:** *Make it inactive* (default: kept, reaches nothing, comes back with one approval) or
  *Remove it* (revoked and deleted; coming back means setting it up again).
- **Ask me by:** any of *phone push*, *email*, *email link* (default: push, and email when no
  phone is set up).

An organization's owner can set the rule for the organization's machines; a member cannot loosen
it. The hub operator sets the bounds (`/admin`), so a hosted hub can, for example, force
removal of free machines after 90 days.

### Coming back

1. Something inactive tries again: a machine's link connects, the CLI refreshes, an AI app
   calls. The hub knows it (the machine's key, the terminal's grant, the AI app's client), so it
   does not ask it to start over: it answers "waiting for approval" and makes one approval
   request.
2. The person gets it where they chose:
   - **Phone push:** "server wants to work again. Last used 34 days ago from 203.0.113.7."
     Tap: Approve or Remove.
   - **Email:** the same words, and a link to the approval on the hub (signed-in approval).
   - **Email link:** the same, and the link itself approves (see the decision below).
3. Approved: it is active again at once (the machine's jobs and settings were kept; the
   terminal gets new tokens; the AI app's next call goes through). Removed or not answered in a
   day: it stays inactive (or is removed, by the rule).
4. Every step is on the audit: went inactive, asked, approved or removed, by which channel.

The CLI says, while it waits: "This machine has been inactive. An approval was sent to your
phone and email; approve it there." It never asks the person to type a code for something the
hub already knows.

## Milestones and tasks

| Milestone | Task | Dependencies | Acceptance evidence | State |
|---|---|---|---|---|
| M1 | Last used, kept: a machine's last contact, a terminal grant's and an AI app grant's last use, written durably (migration, `machine_` and `oauth_` prefixes) | none | Tests; the phone app's "last seen" reads it | pending |
| M2 | The rule: the account's setting (kinds, days, inactive or remove, channels), the organization's and the operator's bounds, a page for each, defaults as above | M1 | Settings tests; pages at phone and desktop size | pending |
| M3 | Going inactive or away: a sweep that applies the rule, inactive meaning it reaches nothing (machine calls refused as `machine_inactive`, grants refused), removal revoking and deleting | M1, M2 | Tests per kind; audit rows | pending |
| M4 | Asking: the approval request a returning inactive thing makes, sent by push and email through the existing notify channels, approved or removed from the phone, `/approvals` or the email | M3 | Tests; e2e: a machine goes inactive, comes back, is approved by push, runs a command | pending |
| M5 | The email link, under the decision below | M4 | Tests: single use, expiry, bound to the one request | pending |
| M6 | The CLI and the link say what is happening while they wait, and resume by themselves once approved | M4 | e2e: `devagotchi` waits, the approval lands, it carries on | pending |

## Risks and approved decisions

- **Approved (operator, 2026-11-16):** configurable in the UI; inactive or removed after a
  chosen time; re-approval by phone push, email, and email link.
- **To decide, the email link:** a link that approves with no sign-in means whoever reads the
  email can bring the machine back. Proposed: the link opens the approval with the request's
  details and approves it with one press only when the browser is signed in, or after the
  passkey otherwise; a person may choose "approve straight from the link" for a kind, with the
  page saying what that allows. The link is single-use, expires in 30 minutes, and names one
  request.
- **To decide, defaults:** 30 days then inactive for machines; 7 days then inactive for
  terminals and AI apps; push, then email.
- **Risk:** an inactive machine's running jobs. Proposed: a machine with a job running is never
  made inactive; the sweep waits until it settles.
- **Risk:** removal is irreversible. The page says so and asks once; removal of a machine with
  jobs in its history keeps the audit and drops the key.

## Verification receipts

None yet: this is the plan.

## Delivery

Implementation: pending. Validation: pending. Push: not_verified.
PR: not_created. Main: not_verified.
**DELIVERY INCOMPLETE: NOT PUSHED / NOT MERGED TO MAIN**
