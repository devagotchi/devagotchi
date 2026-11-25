//! What a phrase means here: the person's own words first, then the dictionary's rules and
//! names, then words they have retired.
//!
//! Order, first that answers wins:
//! 1. a `confirmed` entry for the phrase, the project's ahead of the person's;
//! 2. a dictionary rule for the phrase whose cue is in the surrounding words, then the
//!    phrase's rule with no cue;
//! 3. the dictionary's names, strongest kind first (canonical, alias, retired, informal,
//!    term); two concepts at the same strength are ambiguous unless the surrounding words
//!    name one of them more;
//! 4. a `retired` entry, marked retired;
//! 5. unknown: a phrase to park as `inconclusive` and ask about.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::model::{Entry, Scope, State};
use crate::{
    dictionary::{Concept, Dictionary, LabelKind, Route},
    words::{Word, contains_words, normalize, phrase_words, words},
};

/// What the phrase came to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Outcome {
    /// One concept.
    Resolved { concept: String },
    /// All of these at once.
    Split { concepts: Vec<String> },
    /// One of these; the words around it do not say which.
    Ambiguous { concepts: Vec<String> },
    /// The person's own meaning, which names no concept.
    Personal,
    /// Not a concept here: what it is instead.
    Elsewhere { what: String },
    /// Only the person can say.
    Ask { question: String },
    /// Nothing knows it.
    Unknown,
}

/// What answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    Project,
    Personal,
    Rule,
    Dictionary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Resolution {
    pub phrase: String,
    #[serde(flatten)]
    pub outcome: Outcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<Origin>,
    /// The lexicon entry that answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_id: Option<String>,
    /// The person's meaning, when an entry answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meaning: Option<String>,
    /// How the dictionary names it, when the dictionary answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<LabelKind>,
    /// The phrase is one the person retired.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub retired: bool,
    /// The surrounding words picked one of several concepts.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub by_context: bool,
    /// Why, in a sentence: a rule's note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl Resolution {
    fn new(phrase: &str, outcome: Outcome, origin: Option<Origin>) -> Self {
        Self {
            phrase: phrase.to_string(),
            outcome,
            origin,
            entry_id: None,
            meaning: None,
            label: None,
            retired: false,
            by_context: false,
            note: None,
        }
    }

    /// The concepts it brings in: one when resolved, all of a split, none otherwise.
    pub fn concepts(&self) -> Vec<String> {
        match &self.outcome {
            Outcome::Resolved { concept } => vec![concept.clone()],
            Outcome::Split { concepts } => concepts.clone(),
            _ => Vec::new(),
        }
    }
}

/// `phrase` as said among `context` (the words around it, the phrase included), with
/// `entries` the lexicon entries in reach (project and personal, any state).
pub fn resolve(
    dictionary: &Dictionary,
    entries: &[Entry],
    phrase: &str,
    context: &[Word],
) -> Resolution {
    let key = normalize(phrase);
    let entry = |state: State| {
        let find = |project: bool| {
            entries.iter().find(|e| {
                e.state == state
                    && matches!(e.scope, Scope::Project { .. }) == project
                    && normalize(&e.wording.phrase) == key
            })
        };
        find(true).or_else(|| find(false))
    };
    if let Some(e) = entry(State::Confirmed) {
        return from_entry(dictionary, e, phrase, context);
    }
    let rules: Vec<_> = dictionary.rules_for(phrase).collect();
    let cued = rules.iter().find(|r| {
        r.cues
            .iter()
            .any(|cue| contains_words(context, &phrase_words(cue)))
    });
    if let Some(rule) = cued.or_else(|| rules.iter().find(|r| r.cues.is_empty())) {
        let outcome = match &rule.route {
            Route::Concept { concept } => Outcome::Resolved {
                concept: concept.clone(),
            },
            Route::Split { concepts } => Outcome::Split {
                concepts: concepts.clone(),
            },
            Route::Elsewhere { what } => Outcome::Elsewhere { what: what.clone() },
            Route::Ask { question } => Outcome::Ask {
                question: question.clone(),
            },
        };
        let mut r = Resolution::new(phrase, outcome, Some(Origin::Rule));
        r.note = rule.note.clone();
        return r;
    }
    let found = dictionary.lookup(phrase);
    if let Some(&(_, strongest)) = found.first() {
        let candidates: Vec<&Concept> = found
            .iter()
            .filter(|(_, kind)| *kind == strongest)
            .map(|(c, _)| *c)
            .collect();
        let (outcome, by_context) = pick(&candidates, context);
        let mut r = Resolution::new(phrase, outcome, Some(Origin::Dictionary));
        r.label = Some(strongest);
        r.by_context = by_context;
        return r;
    }
    if let Some(e) = entry(State::Retired) {
        let mut r = from_entry(dictionary, e, phrase, context);
        r.retired = true;
        return r;
    }
    Resolution::new(phrase, Outcome::Unknown, None)
}

fn from_entry(dictionary: &Dictionary, e: &Entry, phrase: &str, context: &[Word]) -> Resolution {
    let concepts: Vec<&Concept> = e
        .wording
        .concepts
        .iter()
        .filter_map(|c| dictionary.concept(c))
        .collect();
    let (outcome, by_context) = if concepts.is_empty() {
        (Outcome::Personal, false)
    } else {
        pick(&concepts, context)
    };
    let origin = match e.scope {
        Scope::Project { .. } => Origin::Project,
        Scope::Personal => Origin::Personal,
    };
    let mut r = Resolution::new(phrase, outcome, Some(origin));
    r.entry_id = Some(e.id.clone());
    r.meaning = Some(e.wording.meaning.clone());
    r.by_context = by_context;
    r
}

/// One concept, or the one the context names most, or ambiguous.
fn pick(candidates: &[&Concept], context: &[Word]) -> (Outcome, bool) {
    if let [only] = candidates {
        return (
            Outcome::Resolved {
                concept: only.id.clone(),
            },
            false,
        );
    }
    let score = |c: &Concept| {
        std::iter::once(&c.label)
            .chain(&c.aliases)
            .chain(&c.informal)
            .chain(&c.terms)
            .filter(|name| contains_words(context, &phrase_words(name)))
            .count()
    };
    let scores: Vec<usize> = candidates.iter().map(|c| score(c)).collect();
    let best = scores.iter().copied().max().unwrap_or(0);
    if best > 0 && scores.iter().filter(|&&s| s == best).count() == 1 {
        let winner = candidates[scores.iter().position(|&s| s == best).unwrap_or(0)];
        return (
            Outcome::Resolved {
                concept: winner.id.clone(),
            },
            true,
        );
    }
    (
        Outcome::Ambiguous {
            concepts: candidates.iter().map(|c| c.id.clone()).collect(),
        },
        false,
    )
}

/// One phrase found in free text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Found {
    /// As written in the text.
    pub written: String,
    /// Its place, in words from the start.
    pub start: usize,
    pub end: usize,
    pub resolution: Resolution,
}

/// The longest known phrases in `text`, left to right, each resolved with the whole text as
/// its context. Unknown phrases are left out, and with `weak` false so are dictionary
/// matches only through an informal phrasing or a term.
pub fn scan(dictionary: &Dictionary, entries: &[Entry], text: &str, weak: bool) -> Vec<Found> {
    let text_words = words(text);
    let mut known: std::collections::HashSet<Vec<String>> = dictionary
        .phrases()
        .map(phrase_words)
        .filter(|w| !w.is_empty())
        .collect();
    known.extend(
        entries
            .iter()
            .filter(|e| matches!(e.state, State::Confirmed | State::Retired))
            .map(|e| phrase_words(&e.wording.phrase))
            .filter(|w| !w.is_empty()),
    );
    let longest = known.iter().map(Vec::len).max().unwrap_or(0);
    let mut found = Vec::new();
    let mut i = 0;
    while i < text_words.len() {
        let mut matched = None;
        for len in (1..=longest.min(text_words.len() - i)).rev() {
            let span = &text_words[i..i + len];
            let normal: Vec<String> = span.iter().map(|w| w.normal.clone()).collect();
            if !known.contains(&normal) {
                continue;
            }
            let written = span
                .iter()
                .map(|w| w.written.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            let r = resolve(dictionary, entries, &written, &text_words);
            if keep(dictionary, &r, &written, weak) {
                matched = Some(Found {
                    written,
                    start: i,
                    end: i + len,
                    resolution: r,
                });
                break;
            }
        }
        match matched {
            Some(f) => {
                i = f.end;
                found.push(f);
            }
            None => i += 1,
        }
    }
    found
}

fn keep(dictionary: &Dictionary, r: &Resolution, written: &str, weak: bool) -> bool {
    if r.outcome == Outcome::Unknown {
        return false;
    }
    if r.origin != Some(Origin::Dictionary) {
        return true;
    }
    if !weak && !r.label.is_some_and(LabelKind::strong) {
        return false;
    }
    let concepts: Vec<String> = match &r.outcome {
        Outcome::Resolved { concept } => vec![concept.clone()],
        Outcome::Ambiguous { concepts } => concepts.clone(),
        _ => Vec::new(),
    };
    concepts.iter().all(|id| {
        dictionary
            .concept(id)
            .is_none_or(|c| dictionary.spelled_right(c, written))
    })
}
