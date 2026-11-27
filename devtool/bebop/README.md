# bebop, the rules devtool

bebop holds the workspace to its rules. A rule is data in the repository, every finding says
what good looks like and how to get there, and today's violations sit on a baseline that only
shrinks. It is a devtool: `publish = false` and `dist = false`, never in the official build
([the program](../../.ai/dev/plans/program.md), §6). This is phase B0: the rule format, the
structure engines, the two output forms, examples as tests, and the layout and naming rules.

## The commands

```text
bebop check [--all | --changed <path>...] [--format text|xml] [--root <dir>]
bebop rule test [<rule-id>]
bebop baseline tighten [<rule-id>]
bebop baseline seed <rule-id>
```

From the repository: `cargo run -p devagotchi-bebop -- check --all`. bebop finds the
repository as the nearest folder up from the working directory with `.ai/rules/`, or takes
`--root`.

- `check` runs every enforced rule over the tree, `--all` by default. `--changed` keeps the
  violations about the given paths (a hook passes the files an agent edited). `--format xml` is
  for an AI and `--format text`, the default, for a person.
- `rule test` runs each rule's `examples/bad/` (it must break the rule) and `examples/good/`
  (it must keep it). A rule without both fails.
- `baseline tighten` lowers each number to today's and drops entries that are fixed or gone.
  It never adds an entry and never raises a number.
- `baseline seed` writes a new rule's first baseline from today's tree, and refuses when the
  rule already has one.

Exit codes: 0 clean, 1 a violation or a failing example, 2 a malformed rule or baseline, or a
usage error. CI runs the same checks through `cargo test`: `tests/workspace.rs` has
`the_workspace_keeps_every_enforced_rule` and `every_rule_passes_its_examples`.

## The report

```xml
<bebop-report rules="8" violations="1">
  <violation rule="layout.file-size-cap" category="layout" severity="error" file="engine/hub/src/x.rs" line="501">
    <statement>A new file holds at most 500 lines. …</statement>
    <why>…</why>
    <expected>`engine/hub/src/x.rs` holds at most 500 lines (the cap is 500); it holds 612.</expected>
    <fix>Split `engine/hub/src/x.rs` by what the code is about, …</fix>
    <example path="engine/hub/src/settings"/>
  </violation>
</bebop-report>
```

The text form carries the same content, grouped by rule.

## A rule

`<boundary>/.ai/rules/<category>/<name>/` holds one rule:

```text
rule.toml        the rule
baseline.txt     today's violations, shrink-only (only when there are any)
examples/good/   a small tree that keeps the rule
examples/bad/    a small tree that breaks it
```

The rules that govern the whole workspace are under the repository root's `.ai/rules/`.
`rule.toml` is read strictly: every field is required unless marked, an unknown field is an
error, and a malformed rule names its file and field.

| Field | What |
|---|---|
| `id` | `<category>.<name>`, the folder's names |
| `category` | the category folder |
| `statement` | the operator's words |
| `why` | the reason, with the upstream practice it follows |
| `sources` | upstream references; empty only when `why` says there is no upstream source |
| `severity` | `"error"` (the one severity in B0) |
| `status` | `proposed`, `approved`, `enforced` or `retired`; only `enforced` fails `check` |
| `version` | an integer from 1 |
| `[scope] include`, `exclude` | globs from the repository root; `*` stays in a folder, `**` crosses folders, an excluded folder takes its contents |
| `[check] engine` | one engine below, with its parameters |
| `[guidance] expected`, `fix` | what good looks like and the correction; `{name}` is a value of the finding |
| `[guidance] example` | a real path that shows the rule kept (checked to exist) |
| `[approval] by`, `date` | required when the status is `approved` or `enforced` |

The engines, all native and structural in B0:

| Engine | Parameters | Finds |
|---|---|---|
| `max_lines` | `max` | a file over `max` lines, counted as `wc -l` does |
| `module_home` | | a module that is both `<x>.rs` and a folder `<x>/` of Rust |
| `forbidden_file_name` | `patterns` | a file whose name a glob matches (`*_tests.rs`) |
| `forbidden_folder_name` | `names` | a folder with one of the names |
| `folder_case` | `case = "kebab"` | a folder not in that case |
| `crate_name_matches_folder` | `prefix` | a `Cargo.toml` whose package is not prefix + folder |
| `layer_folders` | `allowed` | a top-level folder not in the list |
| `domain_role_files` | `roots`, `allowed_files`, `allowed_dirs` | a domain file that is not a role file, a sub-domain without `mod.rs` |

The walk honours `.gitignore`, never walks `.git`, and leaves the rules' own example trees out
of a repository check.

## How a baseline shrinks

A `max_lines` baseline holds `<lines><TAB><path>`; every other baseline holds `<path>`. Comment
lines sit in the header only, and the entries are sorted by path with none twice.

- A listed `max_lines` file may shrink and never grow. A listed violation that is gone fails
  until its line is removed, and an unlisted violation fails.
- A moved file takes its entry along: change the path on its line and keep its number.
- `bebop baseline tighten` lowers numbers and removes fixed or missing entries. Adding a line or
  raising a number is a change to the rule, agreed with the operator, never a way to pass.

## Later phases

B1 adds proposing and approving rules (`bebop rule propose`; only the operator approves, and a
loosened rule without an approval record is refused). B2 brings ast-grep and the code rules,
B3 the hooks, pre-commit and the build split, and B4 the UI rules.
