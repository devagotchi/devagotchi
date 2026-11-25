//! Read-only path projection for immutable remote admission material.
use super::{Scope, display, invalid, io_error};
use devagotchi_contracts::{BridgeResult, FileInvocation, GrantCall, GrantLevel};
use sha2::{Digest, Sha256};
use std::path::{Component, Path, PathBuf};

fn projected(path: &Path) -> BridgeResult<(PathBuf, Vec<PathBuf>)> {
    let mut ancestor = path;
    let mut missing = Vec::new();
    loop {
        match dunce::canonicalize(ancestor) {
            Ok(mut canonical) => {
                let mut created = Vec::new();
                for name in missing.iter().rev() {
                    canonical.push(name);
                    created.push(canonical.clone());
                }
                return Ok((canonical, created));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // A dangling link is not an absent name. Do not reinterpret its target.
                if std::fs::symlink_metadata(ancestor).is_ok_and(|m| m.file_type().is_symlink()) {
                    return Err(io_error("file target", error));
                }
                let Some(Component::Normal(name)) = ancestor.components().next_back() else {
                    return Err(invalid("missing path components must be plain names"));
                };
                missing.push(name.to_os_string());
                ancestor = ancestor
                    .parent()
                    .ok_or_else(|| invalid("path has no existing ancestor"))?;
            }
            Err(error) => return Err(io_error("file target", error)),
        }
    }
}

impl Scope<'_> {
    pub(crate) fn project(
        &self,
        invocation: &FileInvocation,
        level: GrantLevel,
        config: &str,
    ) -> BridgeResult<(GrantCall, String)> {
        invocation.validate()?;
        let mut call = GrantCall::new(invocation.action().capability(), level);
        let mut material = Vec::new();
        for path in invocation.paths() {
            let given = self.given(path)?;
            let given_text = display(&given)?;
            call.paths.push(given_text.clone());
            let resolved = match projected(&given) {
                Ok((canonical, missing)) => {
                    let text = display(&canonical)?;
                    call.paths.push(text.clone());
                    // Any intermediate directories a mutation could create need authority too.
                    if invocation.action() == devagotchi_contracts::GrantAction::FileWrite {
                        for parent in missing.iter().take(missing.len().saturating_sub(1)) {
                            call.paths.push(display(parent)?);
                        }
                    }
                    Ok(text)
                }
                Err(error) => Err(error.code),
            };
            material.push((given_text, resolved));
        }
        call.paths.sort();
        call.paths.dedup();
        // Sandbox candidate selection must not change the canonical input identity.
        let encoded = serde_json::to_vec(&(invocation, material, config))
            .map_err(|e| invalid(format!("cannot encode file projection: {e}")))?;
        Ok((call, hex::encode(Sha256::digest(encoded))))
    }
}
