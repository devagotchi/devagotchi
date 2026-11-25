//! Text as the dictionary and the lexicon compare it: a phrase is normalized once, the same
//! way everywhere, and free text is cut into words that keep their spelling beside their
//! normalized form.

/// A phrase as stored and compared: whitespace collapsed, lower case, typographic quotes made
/// plain. Clyffy normalizes the same way (collapse and lower case), so a phrase keeps its key
/// across a handoff.
pub fn normalize(phrase: &str) -> String {
    phrase
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
        .replace(['\u{2018}', '\u{2019}'], "'")
        .replace(['\u{201C}', '\u{201D}'], "\"")
}

/// One word of free text: as written, and normalized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    pub written: String,
    pub normal: String,
}

/// The words of `text`. A word is a run of letters and digits, with `.`, `+`, `#`, `'` and
/// `-` kept inside it (`Next.js`, `C++`, `C#`, `who's`, `e-mail`) and dropped at its ends.
pub fn words(text: &str) -> Vec<Word> {
    let text = text.replace(['\u{2018}', '\u{2019}'], "'");
    let mut out = Vec::new();
    let mut current = String::new();
    let flush = |current: &mut String, out: &mut Vec<Word>| {
        let trimmed = current.trim_end_matches(['.', '\'', '-']);
        // `C++` and `C#` keep their ending; a sentence's full stop does not stay on a word.
        if !trimmed.is_empty() {
            out.push(Word {
                written: trimmed.to_string(),
                normal: trimmed.to_lowercase(),
            });
        }
        current.clear();
    };
    for c in text.chars() {
        let inner = matches!(c, '.' | '+' | '#' | '\'' | '-');
        if c.is_alphanumeric() || (inner && !current.is_empty()) {
            current.push(c);
        } else {
            flush(&mut current, &mut out);
        }
    }
    flush(&mut current, &mut out);
    out
}

/// The normalized words of a phrase, for matching it inside free text.
pub fn phrase_words(phrase: &str) -> Vec<String> {
    words(phrase).into_iter().map(|w| w.normal).collect()
}

/// Whether `needle` (normalized words) occurs in `haystack` as consecutive words.
pub fn contains_words(haystack: &[Word], needle: &[String]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window.iter().zip(needle).all(|(w, n)| &w.normal == n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_phrase_normalizes_like_clyffys() {
        assert_eq!(normalize("  Log   In\twith X "), "log in with x");
        assert_eq!(normalize("Who\u{2019}s logged in"), "who's logged in");
    }

    #[test]
    fn words_keep_inner_marks_and_drop_trailing_ones() {
        let got: Vec<_> = words("Use Next.js, C++ and C#. Who's in? e-mail.")
            .into_iter()
            .map(|w| w.written)
            .collect();
        assert_eq!(
            got,
            [
                "Use", "Next.js", "C++", "and", "C#", "Who's", "in", "e-mail"
            ]
        );
    }

    #[test]
    fn a_phrase_is_found_as_consecutive_words() {
        let text = words("Can we add chat with my docs?");
        assert!(contains_words(&text, &phrase_words("chat with my docs")));
        assert!(!contains_words(&text, &phrase_words("chat my docs")));
        assert!(!contains_words(&text, &[]));
    }
}
