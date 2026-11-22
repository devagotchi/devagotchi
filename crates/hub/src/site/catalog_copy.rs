//! Owner catalog editor text supplied by the site's localized messages.
use serde::Deserialize;
use std::collections::BTreeMap;
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogCopy {
    pub title: String,
    pub description: String,
    pub back: String,
    pub new: String,
    pub empty: String,
    pub more: String,
    pub default: String,
    pub choose_default: String,
    pub confirm_default: String,
    pub confirm_default_help: String,
    pub id: String,
    pub id_help: String,
    pub structured_help: String,
    pub save: String,
    pub remove: String,
    pub remove_help: String,
    pub refused: String,
    pub kinds: BTreeMap<String, String>,
    pub fields: BTreeMap<String, String>,
}
impl CatalogCopy {
    pub(super) fn check(&self) -> Result<(), String> {
        for kind in crate::catalog::Kind::ALL {
            if !self.kinds.contains_key(kind.name()) {
                return Err(format!(
                    "content/messages.json: app.catalog.kinds lacks {}",
                    kind.name()
                ));
            }
            for field in kind.fields() {
                if !self.fields.contains_key(field.name) {
                    return Err(format!(
                        "content/messages.json: app.catalog.fields lacks {}",
                        field.name
                    ));
                }
            }
        }
        Ok(())
    }
}
