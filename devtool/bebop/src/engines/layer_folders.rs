//! `layer_folders`: the top-level folders in scope are only the agreed ones. A folder that is
//! the plural of an agreed one (`interfaces/` for `interface/`) is told its singular name.

use super::Finding;
use crate::tree::{Scope, Tree};

pub fn run(tree: &Tree, scope: &Scope, allowed: &[String]) -> Vec<Finding> {
    tree.child_dirs("")
        .into_iter()
        .filter(|path| scope.contains(path))
        .filter(|path| !allowed.iter().any(|name| name == path))
        .map(|path| {
            let singular = path
                .strip_suffix('s')
                .filter(|stem| allowed.iter().any(|name| name == stem));
            let suggestion = match singular {
                Some(stem) => format!(
                    "rename `{path}/` to `{stem}/`: layer folders are singular, and every path and \
                     document that names it moves with it"
                ),
                None => format!(
                    "move what `{path}/` holds into the layer it belongs to; a new top-level \
                     folder is added to this rule's `allowed` list only with the operator's approval"
                ),
            };
            Finding::new(path)
                .with("name", path)
                .with("allowed", allowed.join(", "))
                .with("suggestion", suggestion)
        })
        .collect()
}
