//! Load the real site's full localized catalog on the executable's native main stack.
use std::{path::Path, process::Command};

#[test]
fn hub_loads_builtin_and_overridden_site_in_the_real_binary() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for site in ["crates/hub/site/default", "tests/fixtures/neutral-site"] {
        let output = Command::new(env!("CARGO_BIN_EXE_devagotchi"))
            .args(["hub", "check-site"])
            .arg(root.join(site).canonicalize().unwrap())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{site}: {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("checks out"));
    }
}
