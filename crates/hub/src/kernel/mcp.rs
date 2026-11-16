//! The lexicon's and the dictionary's MCP tools. Each acts for the signed-in subject only:
//! the owner comes from the bearer token ([`super::owner_of`]), never from an argument, so
//! one person's words are never another's.
//!
//! - `dictionary_get` reads the dictionary Devagotchi ships: a concept, what a phrase names,
//!   or every concept.
//! - `lexicon_resolve` says what a phrase means for this person (and project), or finds every
//!   known phrase in a piece of text.
//! - `lexicon_propose` adds a phrase: proposed by the AI, parked as not understood yet, or
//!   confirmed when the person said what it means in their own words.
//! - `lexicon_review` lists entries and their history, and confirms, dismisses, retires,
//!   deletes or rewords one.

use devagotchi_contracts::BridgeError;
use devagotchi_kernel::{
    dictionary::{Concept, Dictionary, LabelKind, Rule},
    lexicon::{
        Change, Entry, Event, Filter, Found, LexiconError, Operation, Provenance, Resolution,
        Scope, Wording,
    },
};
use rmcp::{
    Json, RoleServer, handler::server::wrapper::Parameters, service::RequestContext, tool,
    tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::owner_of;
use crate::mcp::Hub;

fn refused(error: LexiconError) -> Json<BridgeError> {
    let message = match &error {
        LexiconError::Invalid(m) | LexiconError::Conflict(m) | LexiconError::Store(m) => m.clone(),
        LexiconError::NotFound => "no entry with that id is yours".to_string(),
    };
    Json(BridgeError::new(error.code(), message))
}

fn invalid(message: &str) -> Json<BridgeError> {
    Json(BridgeError::new("invalid_request", message))
}

fn scope_of(project_id: Option<String>) -> Scope {
    match project_id.filter(|p| !p.trim().is_empty()) {
        Some(project_id) => Scope::Project { project_id },
        None => Scope::Personal,
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
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
pub struct ProposeInput {
    /// Your id for this change: the same id again returns the first answer.
    pub request_id: String,
    pub phrase: String,
    /// What the phrase means, in the person's words.
    #[serde(default)]
    pub meaning: String,
    /// Dictionary concepts it means (`boundary/iam`); none when it is the person's own.
    #[serde(default)]
    pub concepts: Vec<String>,
    #[serde(default)]
    pub examples: Vec<String>,
    #[serde(default)]
    pub counterexamples: Vec<String>,
    /// The project it belongs to; omit for the person's own words.
    #[serde(default)]
    pub project_id: Option<String>,
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
pub struct ReviewInput {
    pub action: Action,
    /// The project whose entries these are; omit for the person's own.
    #[serde(default)]
    pub project_id: Option<String>,
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
    #[serde(default)]
    pub phrase: Option<String>,
    #[serde(default)]
    pub meaning: Option<String>,
    #[serde(default)]
    pub concepts: Vec<String>,
    #[serde(default)]
    pub examples: Vec<String>,
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

fn dictionary_answer(
    dictionary: &Dictionary,
    input: DictionaryInput,
) -> Result<DictionaryOutput, Json<BridgeError>> {
    let mut out = DictionaryOutput {
        source: dictionary.data().source.clone(),
        concepts: Vec::new(),
        rules: Vec::new(),
    };
    match (input.concept, input.phrase) {
        (Some(_), Some(_)) => return Err(invalid("give concept or phrase, not both")),
        (Some(id), None) => {
            let concept = dictionary
                .concept(&id)
                .ok_or_else(|| Json(BridgeError::new("not_found", format!("no concept {id}"))))?;
            out.concepts.push(Named {
                concept: concept.clone(),
                label: None,
            });
        }
        (None, Some(phrase)) => {
            out.concepts = dictionary
                .lookup(&phrase)
                .into_iter()
                .map(|(concept, kind)| Named {
                    concept: concept.clone(),
                    label: Some(kind),
                })
                .collect();
            out.rules = dictionary.rules_for(&phrase).cloned().collect();
        }
        (None, None) => {
            out.concepts = dictionary
                .concepts()
                .iter()
                .map(|concept| Named {
                    concept: concept.clone(),
                    label: None,
                })
                .collect();
        }
    }
    Ok(out)
}

impl Hub {
    /// One change for the signed-in subject, recorded in the audit chain with its outcome.
    async fn change(
        &self,
        tool: &str,
        context: &RequestContext<RoleServer>,
        build: impl FnOnce(String) -> Change,
    ) -> Result<Entry, Json<BridgeError>> {
        let subject = self.subject(context)?;
        let change = build(owner_of(&subject));
        let answer = self.lexicon().apply(&change).await;
        let (id, outcome) = match &answer {
            Ok(entry) => (entry.id.clone(), "ok"),
            Err(error) => (change.record_id.clone().unwrap_or_default(), error.code()),
        };
        self.record(&subject, tool, "", &id, outcome).await?;
        answer.map_err(refused)
    }
}

#[tool_router(router = lexicon_router, vis = "pub(crate)")]
impl Hub {
    #[tool(
        name = "dictionary_get",
        description = "Read the dictionary this hub ships: the boundaries a product is split into (identity, admin, messaging, AI) and the stacks a repository is built with, each with its canonical name, aliases, definition and sources. With `concept`, that concept; with `phrase`, the concepts it names and the rules that tell its meanings apart; with neither, every concept.",
        title = "Read the dictionary",
        annotations(
            title = "Read the dictionary",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn dictionary_get(
        &self,
        Parameters(input): Parameters<DictionaryInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<DictionaryOutput>, Json<BridgeError>> {
        self.subject(&context)?;
        dictionary_answer(Dictionary::seed(), input).map(Json)
    }

    #[tool(
        name = "lexicon_resolve",
        description = "What a phrase means to this person: their project's confirmed meaning first, then their own, then the dictionary's rules and concepts. Give `phrase` (with the sentence it was said in as `context`, which picks between meanings) for one answer, or `text` to find every known phrase in it. An `ask` outcome carries the question only the person can answer; ask it as written.",
        title = "Resolve a phrase",
        annotations(
            title = "Resolve a phrase",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn lexicon_resolve(
        &self,
        Parameters(input): Parameters<ResolveInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<ResolveOutput>, Json<BridgeError>> {
        let owner = owner_of(&self.subject(&context)?);
        let lexicon = self.lexicon();
        let project = input.project_id.as_deref().filter(|p| !p.trim().is_empty());
        let mut out = ResolveOutput {
            resolution: None,
            found: Vec::new(),
        };
        match (input.phrase, input.text) {
            (Some(phrase), None) if !phrase.trim().is_empty() => {
                let context = input.context.unwrap_or_default();
                out.resolution = Some(
                    lexicon
                        .resolve(&owner, project, &phrase, &context)
                        .await
                        .map_err(refused)?,
                );
            }
            (None, Some(text)) => {
                out.found = lexicon
                    .scan(&owner, project, &text, input.weak)
                    .await
                    .map_err(refused)?;
            }
            _ => return Err(invalid("give a phrase or text, one of them")),
        }
        Ok(Json(out))
    }

    #[tool(
        name = "lexicon_propose",
        description = "Add a phrase to this person's words (or their project's, with `project_id`). `start` is `proposed` (default: a meaning you suggest, which the person confirms with lexicon_review), `inconclusive` (a phrase you saw and did not understand; meaning may be empty) or `confirmed`, only when the person said what it means in their own words. One live entry per phrase in a scope. Pass a fresh `request_id`; sending it again returns the first answer.",
        title = "Add a phrase",
        annotations(
            title = "Add a phrase",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn lexicon_propose(
        &self,
        Parameters(input): Parameters<ProposeInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<EntryOutput>, Json<BridgeError>> {
        let scope = scope_of(input.project_id);
        let wording = Wording {
            phrase: input.phrase,
            meaning: input.meaning,
            concepts: input.concepts,
            examples: input.examples,
            counterexamples: input.counterexamples,
        };
        let (operation, source) = match input.start {
            Start::Proposed => (Operation::Propose { wording }, "conversation"),
            Start::Inconclusive => (Operation::Park { wording }, "conversation"),
            Start::Confirmed => (Operation::Define { wording }, "user"),
        };
        let entry = self
            .change("lexicon_propose", &context, |owner_id| Change {
                request_id: input.request_id,
                owner_id,
                provenance: provenance(source, &scope),
                scope,
                record_id: None,
                expected_revision: None,
                operation,
            })
            .await?;
        Ok(Json(EntryOutput { entry }))
    }

    #[tool(
        name = "lexicon_review",
        description = "Review this person's words (or their project's, with `project_id`). `list` gives the entries (`filter`: confirmed, reviewable (default) or all) and `history` one entry's changes. `confirm` (once the person agrees), `dismiss`, `retire` (no longer used; old text still resolves), `delete` (erases its words) and `revise` (new phrase, meaning, concepts or examples) change one: name its `id`, the `expected_revision` you read and a fresh `request_id`.",
        title = "Review your words",
        annotations(
            title = "Review your words",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn lexicon_review(
        &self,
        Parameters(input): Parameters<ReviewInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<ReviewOutput>, Json<BridgeError>> {
        let scope = scope_of(input.project_id.clone());
        let operation = match input.action {
            Action::List | Action::History => {
                let owner = owner_of(&self.subject(&context)?);
                let lexicon = self.lexicon();
                let mut out = ReviewOutput::default();
                if let Action::List = input.action {
                    let filter = match input.filter {
                        Which::Confirmed => Filter::Confirmed,
                        Which::Reviewable => Filter::Reviewable,
                        Which::All => Filter::All,
                    };
                    out.entries = lexicon
                        .list(&owner, &scope, filter)
                        .await
                        .map_err(refused)?;
                } else {
                    let id = input
                        .id
                        .ok_or_else(|| invalid("history names the entry's id"))?;
                    out.events = lexicon.history(&owner, &id).await.map_err(refused)?;
                }
                return Ok(Json(out));
            }
            Action::Confirm => Operation::Confirm,
            Action::Dismiss => Operation::Dismiss,
            Action::Retire => Operation::Retire,
            Action::Delete => Operation::Delete,
            Action::Revise => Operation::Revise {
                wording: Wording {
                    phrase: input
                        .phrase
                        .ok_or_else(|| invalid("revise gives the phrase, changed or not"))?,
                    meaning: input.meaning.unwrap_or_default(),
                    concepts: input.concepts,
                    examples: input.examples,
                    counterexamples: input.counterexamples,
                },
            },
        };
        let (Some(id), Some(expected), Some(request_id)) =
            (input.id, input.expected_revision, input.request_id)
        else {
            return Err(invalid(
                "a change names the entry's id, the expected_revision you read and a request_id",
            ));
        };
        let entry = self
            .change("lexicon_review", &context, |owner_id| Change {
                request_id,
                owner_id,
                provenance: provenance("user", &scope),
                scope,
                record_id: Some(id),
                expected_revision: Some(expected),
                operation,
            })
            .await?;
        Ok(Json(ReviewOutput {
            entry: Some(entry),
            ..ReviewOutput::default()
        }))
    }
}

fn provenance(source: &str, scope: &Scope) -> Provenance {
    Provenance {
        source_kind: source.to_string(),
        project_id: match scope {
            Scope::Project { project_id } => Some(project_id.clone()),
            Scope::Personal => None,
        },
        workflow_id: None,
        turn_id: None,
    }
}
