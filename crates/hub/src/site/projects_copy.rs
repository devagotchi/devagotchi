//! Browser project administration copy, provided by the site's messages.
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectsCopy {
    pub title: String,
    pub description: String,
    pub active: String,
    pub retired: String,
    pub new: String,
    pub name: String,
    pub id: String,
    pub id_help: String,
    pub save: String,
    pub remove: String,
    pub remove_help: String,
    pub empty: String,
    pub back: String,
    pub more: String,
    pub history: String,
    pub workflow: String,
    pub local_workflow: String,
    pub refused: String,
    pub retired_notice: String,
    pub open: String,
}
