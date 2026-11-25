//! The dictionary: the concepts Devagotchi routes to, each with one canonical label and the
//! other names that lead to it.
//!
//! The model is SKOS (W3C, 2009) with ISO 25964-1 practice: a concept has one canonical label
//! (`prefLabel`), any number of aliases (`altLabel`) and informal phrasings (`hiddenLabel`:
//! they resolve, they are never shown), and retired names kept so old text still resolves.
//! `terms` is the concept's own vocabulary (the "canonical concept map" of the boundary
//! documents): a term points at its concept more weakly than a name does, and a term two
//! concepts share is ambiguous until context picks. Disambiguation [`Rule`]s route a phrase
//! by the words around it, as the boundary documents' disambiguation lines say.
//!
//! The shipped seed ([`Dictionary::seed`]) is the Boundary Question Bank's "Domain
//! Boundaries v0.1" and "Stack Category Routing v0.2".

mod markers;
#[cfg(test)]
mod tests;

use std::{
    collections::{HashMap, HashSet},
    sync::OnceLock,
};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::words::{normalize, phrase_words};

/// Which list a concept belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Scheme {
    /// A domain boundary a change crosses (identity, administration, messaging, AI, ...).
    Boundary,
    /// The runtime a change is built in (Rust, Next.js, Go, ...).
    Stack,
}

/// One concept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Concept {
    /// `<scheme>/<key>`, for example `boundary/iam` or `stack/rust`. Never reused.
    pub id: String,
    pub scheme: Scheme,
    /// The canonical label: one per concept, unique in the dictionary.
    pub label: String,
    pub definition: String,
    /// Other names that mean exactly this concept.
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Plain-word phrasings that resolve here but are never offered as the name.
    #[serde(default)]
    pub informal: Vec<String>,
    /// Names no longer used, kept so older text still resolves.
    #[serde(default)]
    pub retired: Vec<String>,
    /// The concept's own vocabulary: a weaker pointer than a name.
    #[serde(default)]
    pub terms: Vec<String>,
    /// Names that match free text only in exactly this spelling (`Go`, `C`), because in
    /// lower case they are ordinary words.
    #[serde(default)]
    pub exact_case: Vec<String>,
    /// The standards and curricula the concept is phrased from.
    #[serde(default)]
    pub sources: Vec<String>,
    /// Stack only: files whose presence decides it (`Cargo.toml`, `next.config.*`, `app/`,
    /// `*.sql`).
    #[serde(default)]
    pub markers: Vec<String>,
    /// Stack only: dependency names whose presence decides it (`next`, `drizzle-orm`).
    #[serde(default)]
    pub packages: Vec<String>,
    /// Concepts that, when also detected, replace this one (React yields to Next.js).
    #[serde(default)]
    pub yields_to: Vec<String>,
    /// Concepts this one depends on (administration depends on identity).
    #[serde(default)]
    pub depends_on: Vec<String>,
}

/// How a name points at its concept, strongest first.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum LabelKind {
    Canonical,
    Alias,
    Retired,
    Informal,
    Term,
}

impl LabelKind {
    /// Whether free-text scanning counts it without being asked for weak matches.
    pub fn strong(self) -> bool {
        matches!(self, Self::Canonical | Self::Alias | Self::Retired)
    }
}

/// Where a disambiguation rule sends a phrase.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "to", rename_all = "snake_case", deny_unknown_fields)]
pub enum Route {
    /// This one concept.
    Concept { concept: String },
    /// Every one of these concepts applies at once ("admin user": the privilege model and
    /// the credential).
    Split { concepts: Vec<String> },
    /// Not a concept of this dictionary: what it is instead.
    Elsewhere { what: String },
    /// Only the person can say: the question to ask them.
    Ask { question: String },
}

/// A disambiguation line: `phrase` near any of `cues` goes to `route`. A rule with no cues is
/// the phrase's route when no cue of its other rules is present.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub phrase: String,
    #[serde(default)]
    pub cues: Vec<String>,
    pub route: Route,
    /// Why, in a sentence, for the person reading the resolution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// A dictionary as written: the seed, or one a host keeps.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DictionaryData {
    /// Where it came from and which version, for the person reading a resolution.
    pub source: String,
    pub concepts: Vec<Concept>,
    #[serde(default)]
    pub rules: Vec<Rule>,
}

/// A checked dictionary with its name index.
#[derive(Debug, Clone)]
pub struct Dictionary {
    data: DictionaryData,
    /// Normalized name -> (concept index, how it names it), strongest first.
    names: HashMap<String, Vec<(usize, LabelKind)>>,
    exact: HashMap<String, Vec<usize>>,
    by_id: HashMap<String, usize>,
}

/// Why a dictionary was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictionaryError(pub String);

impl std::fmt::Display for DictionaryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid dictionary: {}", self.0)
    }
}

impl std::error::Error for DictionaryError {}

const SEED: &str = include_str!("seed.json");

impl Dictionary {
    /// The dictionary Devagotchi ships.
    pub fn seed() -> &'static Dictionary {
        static SEEDED: OnceLock<Dictionary> = OnceLock::new();
        SEEDED.get_or_init(|| {
            let data: DictionaryData = serde_json::from_str(SEED).expect("the shipped seed parses");
            Dictionary::new(data).expect("the shipped seed is valid")
        })
    }

    /// Check `data` and index its names.
    pub fn new(data: DictionaryData) -> Result<Self, DictionaryError> {
        let fail = |m: String| Err(DictionaryError(m));
        let mut by_id = HashMap::new();
        let mut canonical = HashSet::new();
        for (i, c) in data.concepts.iter().enumerate() {
            let key = c.id.split_once('/');
            let scheme_ok = match key {
                Some(("boundary", k)) => c.scheme == Scheme::Boundary && valid_key(k),
                Some(("stack", k)) => c.scheme == Scheme::Stack && valid_key(k),
                _ => false,
            };
            if !scheme_ok {
                return fail(format!(
                    "{}: an id is <scheme>/<key> matching its scheme",
                    c.id
                ));
            }
            if by_id.insert(c.id.clone(), i).is_some() {
                return fail(format!("{}: the id is used twice", c.id));
            }
            if c.label.trim().is_empty() || c.definition.trim().is_empty() {
                return fail(format!(
                    "{}: a concept needs a label and a definition",
                    c.id
                ));
            }
            if !canonical.insert(normalize(&c.label)) {
                return fail(format!(
                    "{}: another concept has the label {:?}",
                    c.id, c.label
                ));
            }
            let named: Vec<&String> = [&c.aliases, &c.informal, &c.retired, &c.terms]
                .into_iter()
                .flatten()
                .collect();
            if named.iter().any(|n| phrase_words(n).is_empty()) {
                return fail(format!("{}: a name has no words", c.id));
            }
            if c.scheme == Scheme::Boundary && !(c.markers.is_empty() && c.packages.is_empty()) {
                return fail(format!("{}: only a stack has markers or packages", c.id));
            }
        }
        for c in &data.concepts {
            for other in c.yields_to.iter().chain(&c.depends_on) {
                if !by_id.contains_key(other) || other == &c.id {
                    return fail(format!("{}: names {other}, which it cannot", c.id));
                }
            }
        }
        for rule in &data.rules {
            if phrase_words(&rule.phrase).is_empty() {
                return fail("a rule has no phrase".into());
            }
            let concepts: Vec<&String> = match &rule.route {
                Route::Concept { concept } => vec![concept],
                Route::Split { concepts } if concepts.len() >= 2 => concepts.iter().collect(),
                Route::Split { .. } => {
                    return fail(format!("{}: a split names two or more", rule.phrase));
                }
                Route::Elsewhere { what: text } | Route::Ask { question: text } => {
                    if text.trim().is_empty() {
                        return fail(format!("{}: a route needs its words", rule.phrase));
                    }
                    vec![]
                }
            };
            if let Some(missing) = concepts.into_iter().find(|c| !by_id.contains_key(*c)) {
                return fail(format!(
                    "{}: routes to {missing}, not a concept",
                    rule.phrase
                ));
            }
        }
        let mut names: HashMap<String, Vec<(usize, LabelKind)>> = HashMap::new();
        let mut exact: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, c) in data.concepts.iter().enumerate() {
            let lists = [
                (LabelKind::Canonical, std::slice::from_ref(&c.label)),
                (LabelKind::Alias, &c.aliases[..]),
                (LabelKind::Retired, &c.retired[..]),
                (LabelKind::Informal, &c.informal[..]),
                (LabelKind::Term, &c.terms[..]),
            ];
            for (kind, list) in lists {
                for name in list {
                    let held = names.entry(normalize(name)).or_default();
                    if !held.iter().any(|(j, _)| *j == i) {
                        held.push((i, kind));
                    }
                }
            }
            for name in &c.exact_case {
                exact.entry(normalize(name)).or_default().push(i);
            }
        }
        for held in names.values_mut() {
            held.sort_by_key(|(i, kind)| (*kind, *i));
        }
        Ok(Self {
            data,
            names,
            exact,
            by_id,
        })
    }

    pub fn data(&self) -> &DictionaryData {
        &self.data
    }

    pub fn concepts(&self) -> &[Concept] {
        &self.data.concepts
    }

    pub fn concept(&self, id: &str) -> Option<&Concept> {
        self.by_id.get(id).map(|&i| &self.data.concepts[i])
    }

    /// The concepts `phrase` names, strongest names first (case is ignored here).
    pub fn lookup(&self, phrase: &str) -> Vec<(&Concept, LabelKind)> {
        self.names
            .get(&normalize(phrase))
            .map(|held| {
                held.iter()
                    .map(|&(i, kind)| (&self.data.concepts[i], kind))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Whether `written`, exactly as written in free text, may name `concept`: false for a
    /// name the concept keeps to one spelling when this is another.
    pub fn spelled_right(&self, concept: &Concept, written: &str) -> bool {
        match self.exact.get(&normalize(written)) {
            Some(held) if held.iter().any(|&i| self.data.concepts[i].id == concept.id) => concept
                .exact_case
                .iter()
                .any(|name| name.split_whitespace().eq(written.split_whitespace())),
            _ => true,
        }
    }

    /// The disambiguation rules for `phrase`, in the order written.
    pub fn rules_for<'a>(&'a self, phrase: &str) -> impl Iterator<Item = &'a Rule> + 'a {
        let key = normalize(phrase);
        self.data
            .rules
            .iter()
            .filter(move |r| normalize(&r.phrase) == key)
    }

    /// Every phrase the dictionary knows (names and rule phrases), normalized.
    pub fn phrases(&self) -> impl Iterator<Item = &str> {
        self.names
            .keys()
            .map(String::as_str)
            .chain(self.data.rules.iter().map(|r| r.phrase.as_str()))
    }

    /// The stacks a repository's files decide, with `packages` its dependency names: each
    /// concept a marker or package names, less those that yield to another one found.
    pub fn stacks_for(&self, files: &[&str], packages: &[&str]) -> Vec<&Concept> {
        markers::stacks_for(self, files, packages)
    }
}

fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
