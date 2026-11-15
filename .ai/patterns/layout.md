# Pattern: one boundary, one home

Where Rust code lives in this workspace. Follow it for every new file and every move.
The [structure cleanup plan](../dev/plans/structure-cleanup.md) brings the existing code to
it one step at a time; until a step lands, the code it covers stays where it is.

1. **A crate per boundary.** A boundary is something that can be built, tested and depended
   on without the rest: the broker, the link, the hub, AgentHub. A boundary never reaches
   into another crate's modules; it depends on that crate's public surface or on
   `crates/contracts`.
2. **A folder per domain.** Inside a crate, each domain is `src/<domain>/`, and
   `src/<domain>/mod.rs` is its front door: the module docs, the `mod` lines and the names
   the rest of the crate may use. A domain has exactly one home. No `<domain>.rs` beside a
   `<domain>/` folder, and no two homes whose names differ by a letter.
3. **Tests sit with their domain.** `src/<domain>/tests.rs`, declared from the domain's
   `mod.rs` as `#[cfg(test)] mod tests;`. No `<domain>_tests.rs` at the crate's top level.
   Tests that drive a built binary stay in the crate's `tests/` folder.
4. **The crate's top level is wiring only.** `lib.rs` or `main.rs`, the `mod` lines, and the
   few files that compose domains into the crate's entry points. Logic that belongs to one
   domain lives in that domain.
5. **Re-exports stay at the front door.** `pub use` belongs in a domain's `mod.rs`. There is
   no top-level barrel that re-exports several domains: a path says where the code lives.
6. **A new file holds at most 500 lines.** Past that, split by what the code is about, not
   by line count. Tests count: a long `tests.rs` becomes `tests/` with a file per subject.

## The size ratchet

Rule 6 is enforced by `crates/devagotchi/tests/layout.rs`. It walks every `.rs` file under
`crates/` and `apps/` (skipping `target`, `node_modules` and dot folders) and compares it
with `crates/devagotchi/tests/layout/oversize.txt`, the list of files that were already over
the cap when the rule arrived, each with its line count. Lines are counted as `wc -l` does.

- A listed file may shrink. It may never hold more lines than its entry.
- A file that is not listed may not exceed the cap.
- An entry whose file is gone fails: a move takes its entry along, a deletion removes it.

The list only gets shorter and its numbers only get smaller:

- **A file shrank.** Lower its number, or run
  `DEVAGOTCHI_WRITE_LAYOUT=1 cargo test -p devagotchi --test layout`, which lowers every
  entry to today's count and drops entries now at or under the cap. It never adds an entry
  and never raises a number.
- **A file moved.** Change the path on its line by hand and keep the number, so the review
  shows the same count at the new path.
- **A file needs to grow, or a new file needs more than 500 lines.** Split it. Raising a
  number or adding a line to the list is a change to this pattern, agreed with the operator
  first, never a way to make the test pass.

What the test does not check: rules 1 to 5 are held by review until the plan's later steps
give them tests of their own.
