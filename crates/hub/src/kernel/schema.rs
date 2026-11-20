//! What the dictionary's and the lexicon's tools take and give, as the MCP schema clients see
//! them (`kernel/mcp.rs` serves them).

use devagotchi_kernel::{
    dictionary::{Concept, LabelKind, Rule},
    lexicon::{Entry, Event, Found, Resolution},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DictionaryInput {
    /// One concept by id (`boundary/iam`, `stack/nextjs`).
    #[serde(default)]
    pub concept: Option<String>,
    /// What this phrase names: its concepts, and the rules that tell its meanings apart.
    #[serde(default)]
    pub phrase: Option<String>,
}

/// A concept, and how the phrase asked about names it.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Named {
    pub concept: Concept,
    /// `canonical`, `alias`, `retired`, `informal` or `term`; only when a phrase was asked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<LabelKind>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DictionaryOutput {
    /// Where the dictionary's words come from.
    pub source: String,
    pub concepts: Vec<Named>,
    /// The rules for the phrase asked about, in the order they are tried.
    pub rules: Vec<Rule>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResolveInput {
    /// One phrase to resolve. Give this or `text`.
    #[serde(default)]
    pub phrase: Option<String>,
    /// The sentence the phrase was said in: its words pick between meanings.
    #[serde(default)]
    pub context: Option<String>,
    /// Free text to scan for every phrase this person, their project or the dictionary knows.
    #[serde(default)]
    pub text: Option<String>,
    /// With `text`: also find weak words (`users`, `settings`) that only a rule or the
    /// context can place.
    #[serde(default)]
    pub weak: bool,
    /// The project whose words come first; omit for the person's own words only.
    #[serde(default)]
    pub project_id: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ResolveOutput {
    /// The phrase's meaning, when a phrase was asked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<Resolution>,
    /// Every known phrase in the text, in order, when text was given.
    pub found: Vec<Found>,
}

/// How a new entry starts.
#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Start {
    /// A meaning you suggest; the person confirms it.
    #[default]
    Proposed,
    /// A phrase seen and not understood yet; meaning may be empty.
    Inconclusive,
    /// The person said what it means, in their own words.
    Confirmed,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProposeInput {
    /// Your id for this change: the same id again returns the first answer.
    pub request_id: String,
    /// The phrase as the person says it (`the admin page`, `IAM`).
    pub phrase: String,
    /// What the phrase means, in the person's words.
    #[serde(default)]
    pub meaning: String,
    /// Dictionary concepts it means (`boundary/iam`); none when it is the person's own.
    #[serde(default)]
    pub concepts: Vec<String>,
    /// Sentences where the phrase means this, in the person's words.
    #[serde(default)]
    pub examples: Vec<String>,
    /// Sentences where the same words mean something else.
    #[serde(default)]
    pub counterexamples: Vec<String>,
    /// The project it belongs to; omit for the person's own words.
    #[serde(default)]
    pub project_id: Option<String>,
    /// How the entry starts: `proposed` (default), `inconclusive` or `confirmed`.
    #[serde(default)]
    pub start: Start,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// The entries of one scope (`filter`: confirmed, reviewable or all).
    List,
    /// One entry's changes, oldest first.
    History,
    Confirm,
    Dismiss,
    Retire,
    Delete,
    /// New words for a live entry: `phrase`, `meaning`, `concepts`, `examples`.
    Revise,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Which {
    Confirmed,
    /// Every state but deleted.
    #[default]
    Reviewable,
    All,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReviewInput {
    /// What to do: `list`, `history`, `confirm`, `dismiss`, `retire`, `delete` or `revise`.
    pub action: Action,
    /// The project whose entries these are; omit for the person's own.
    #[serde(default)]
    pub project_id: Option<String>,
    /// With `list`: `confirmed`, `reviewable` (default: every state but deleted) or `all`.
    #[serde(default)]
    pub filter: Which,
    /// The entry, for every action but `list`.
    #[serde(default)]
    pub id: Option<String>,
    /// The revision you read, for a change.
    #[serde(default)]
    pub expected_revision: Option<u64>,
    /// Your id for a change: the same id again returns the first answer.
    #[serde(default)]
    pub request_id: Option<String>,
    /// With `revise`: the phrase, changed or not.
    #[serde(default)]
    pub phrase: Option<String>,
    /// With `revise`: what it means, in the person's words.
    #[serde(default)]
    pub meaning: Option<String>,
    /// With `revise`: the dictionary concepts it means (`boundary/iam`).
    #[serde(default)]
    pub concepts: Vec<String>,
    /// With `revise`: sentences where the phrase means this.
    #[serde(default)]
    pub examples: Vec<String>,
    /// With `revise`: sentences where the same words mean something else.
    #[serde(default)]
    pub counterexamples: Vec<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct EntryOutput {
    pub entry: Entry,
}

#[derive(Debug, Default, Serialize, JsonSchema)]
pub struct ReviewOutput {
    /// The entry as the change left it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<Entry>,
    /// What `list` found.
    pub entries: Vec<Entry>,
    /// What `history` found.
    pub events: Vec<Event>,
}
