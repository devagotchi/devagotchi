//! `max_lines`: how many lines each file in scope holds. The cap and the baseline are applied
//! by the checks, so a baselined file can be held to its own number.

use std::collections::BTreeMap;

use crate::tree::{Scope, Tree};

/// Lines as `wc -l` counts them, plus a last line that has no newline.
pub fn line_count(bytes: &[u8]) -> usize {
    let newlines = bytes.iter().filter(|byte| **byte == b'\n').count();
    match bytes.last() {
        Some(last) if *last != b'\n' => newlines + 1,
        _ => newlines,
    }
}

/// Every file in scope, with its line count.
pub fn measure(tree: &Tree, scope: &Scope) -> Result<BTreeMap<String, usize>, String> {
    tree.files()
        .filter(|path| scope.contains(path))
        .map(|path| Ok((path.to_owned(), line_count(&tree.read(path)?))))
        .collect()
}
