//! Every commit on main is built and attested; only a tag publishes; a pull request builds
//! nothing here (dist-workspace.toml `pr-run-mode = "skip"`; ci.yml builds and tests every
//! platform on a pull request). From 2b3feec to 86b9d8b the release workflow passed on main
//! with every build job skipped, so no hub image or archive of main existed to deploy, and
//! nothing failed. This reads `.github/workflows/release.yml` and fails if that can recur.

mod support;

use std::fs;

const BUILD_LOCAL: &str = "if: ${{ fromJson(needs.plan.outputs.val).ci.github.artifacts_matrix.include != null && (needs.plan.outputs.building == 'true' ||";
const BUILDING: &str =
    "building: ${{ startsWith(github.ref, 'refs/tags/') || github.ref == 'refs/heads/main' }}";
const PUBLISHING: &str = "publishing: ${{ startsWith(github.ref, 'refs/tags/') }}";
const ATTEST_IF: &str = "if: ${{ needs.plan.outputs.building == 'true' }}";
const HOST_IF: &str = "needs.plan.outputs.publishing == 'true'";

/// The lines of the job `name` (two-space indented key) up to the next job.
fn job<'a>(workflow: &'a str, name: &str) -> Option<&'a str> {
    let start = workflow.find(&format!("\n  {name}:\n"))? + 1;
    let rest = &workflow[start..];
    let body = rest.find('\n').map_or("", |line_end| &rest[line_end..]);
    let end = body
        .match_indices("\n  ")
        .find(|(at, _)| {
            let next = &body[at + 3..];
            next.starts_with(|c: char| c.is_ascii_alphabetic()) && !next.starts_with(' ')
        })
        .map_or(body.len(), |(at, _)| at);
    Some(&rest[..rest.len() - body.len() + end])
}

/// Why `workflow` would leave main unbuilt, unattested, or published, if it would.
fn main_builds(workflow: &str) -> Result<(), String> {
    let workflow = workflow.replace("\r\n", "\n");
    let on = workflow.find("\non:\n").ok_or("no `on:` block")?;
    let jobs = workflow.find("\njobs:\n").ok_or("no `jobs:` block")?;
    let trigger = &workflow[on..jobs];
    if !trigger.contains("branches: [main]") {
        return Err("a push to main does not start the release workflow".into());
    }
    if trigger
        .lines()
        .any(|line| line.trim_start().starts_with("pull_request"))
    {
        return Err("pull requests start the release build (pr-run-mode is skip)".into());
    }
    let plan = job(&workflow, "plan").ok_or("no plan job")?;
    for (output, what) in [(BUILDING, "building"), (PUBLISHING, "publishing")] {
        if !plan.contains(output) {
            return Err(format!("the plan job's `{what}` output is not `{output}`"));
        }
    }
    let local = job(&workflow, "build-local-artifacts").ok_or("no build-local-artifacts job")?;
    if !local.contains(BUILD_LOCAL) {
        return Err("build-local-artifacts does not run when `building` (main or a tag)".into());
    }
    let attest = local.find("- name: Attest").ok_or("no Attest step")?;
    if !local[attest..].contains(ATTEST_IF) {
        return Err("the Attest step does not run on main".into());
    }
    let host = job(&workflow, "host").ok_or("no host job")?;
    if !host.contains(HOST_IF) {
        return Err("host is not limited to a tag (publishing)".into());
    }
    // The hub image and the install gate follow the local build: no condition of their own
    // may skip them on main.
    for name in [
        "build-global-artifacts",
        "custom-hub-image",
        "custom-install-check",
    ] {
        let body = job(&workflow, name).ok_or_else(|| format!("no {name} job"))?;
        if body.lines().any(|line| line.starts_with("    if:")) {
            return Err(format!(
                "{name} has its own condition, which may skip it on main"
            ));
        }
    }
    Ok(())
}

fn release_workflow() -> String {
    let path = support::workspace_root().join(".github/workflows/release.yml");
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn every_main_commit_is_built_and_attested_and_only_a_tag_publishes() {
    if let Err(why) = main_builds(&release_workflow()) {
        panic!(".github/workflows/release.yml: {why}");
    }
}

/// The check fails on the workflow as it stood from 2b3feec to 86b9d8b, and on each other way
/// main could stop building.
#[test]
fn the_check_fails_where_main_would_not_build() {
    let good = release_workflow().replace("\r\n", "\n");
    let broken = [
        // 2b3feec: the build ran only for a tag, or for pr-run-mode "upload".
        good.replace(
            "needs.plan.outputs.building == 'true' ||",
            "needs.plan.outputs.publishing == 'true' ||",
        ),
        // e94174a: main built but was never attested.
        good.replace(
            ATTEST_IF,
            "if: ${{ needs.plan.outputs.publishing == 'true' }}",
        ),
        good.replace(
            BUILDING,
            "building: ${{ startsWith(github.ref, 'refs/tags/') }}",
        ),
        good.replace("branches: [main]", "branches: [release]"),
        good.replace(HOST_IF, "needs.plan.outputs.building == 'true'"),
        good.replace(
            "  custom-hub-image:\n",
            "  custom-hub-image:\n    if: ${{ startsWith(github.ref, 'refs/tags/') }}\n",
        ),
    ];
    for (n, workflow) in broken.iter().enumerate() {
        assert_ne!(
            workflow, &good,
            "case {n} changed nothing: the workflow moved"
        );
        assert!(main_builds(workflow).is_err(), "case {n} passed the check");
    }
}
