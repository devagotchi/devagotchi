//! `module_home`: a Rust module has one home. Every `<x>.rs` in scope with a folder `<x>/`
//! beside it that holds Rust is one module in two places (layout rule 2), except `main`, `lib`,
//! `build` and `mod`, whose folders are not their modules. A folder holds its module as
//! `mod.rs`.

use super::Finding;
use crate::tree::{self, Scope, Tree};

const NOT_MODULES: [&str; 4] = ["main", "lib", "build", "mod"];

pub fn run(tree: &Tree, scope: &Scope) -> Vec<Finding> {
    tree.files()
        .filter(|path| scope.contains(path))
        .filter_map(|path| {
            let folder = path.strip_suffix(".rs")?;
            let module = tree::name(folder);
            let holds_rust =
                tree.is_dir(folder) && tree.files_under(folder).any(|file| file.ends_with(".rs"));
            (holds_rust && !NOT_MODULES.contains(&module)).then(|| {
                Finding::new(path)
                    .with("module", module)
                    .with("folder", folder)
                    .with("home", format!("{folder}/mod.rs"))
            })
        })
        .collect()
}
