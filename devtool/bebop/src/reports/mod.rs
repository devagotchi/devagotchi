//! Reports: one finding, two forms. XML for an AI, readable text for a person, with the same
//! content: the rule's statement and why, what good looks like for this finding, the fix, and
//! an example in the code.
//!
//! # Owns
//!
//! The report's shape (program §6, "Output") and its escaping. The XML is written by hand:
//! `&`, `<`, `>`, `"` and `'` are escaped, and characters XML 1.0 cannot hold become U+FFFD.
//!
//! # Serves
//!
//! [`Report`] and [`Violation`] to the checks and the command line.
//!
//! # Settings
//!
//! None.
//!
//! # Depends on
//!
//! Nothing else in the crate.

#[cfg(test)]
mod tests;

/// One violation, everything its reader needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub rule: String,
    pub category: String,
    pub severity: String,
    pub file: String,
    pub line: Option<usize>,
    pub statement: String,
    pub why: String,
    pub expected: String,
    pub fix: String,
    pub example: String,
    /// The path the violation is about, for `--changed`: the file itself, or for a stale
    /// baseline entry the listed path (its `file` is the baseline).
    pub subject: String,
}

/// What a check found: how many rules it ran and every violation, grouped by rule.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Report {
    pub rules: usize,
    pub violations: Vec<Violation>,
}

impl Report {
    pub fn is_clean(&self) -> bool {
        self.violations.is_empty()
    }

    pub fn xml(&self) -> String {
        let mut out = format!(
            "<bebop-report rules=\"{}\" violations=\"{}\">\n",
            self.rules,
            self.violations.len()
        );
        for v in &self.violations {
            out.push_str(&format!(
                "  <violation rule=\"{}\" category=\"{}\" severity=\"{}\" file=\"{}\"",
                escape(&v.rule),
                escape(&v.category),
                escape(&v.severity),
                escape(&v.file)
            ));
            if let Some(line) = v.line {
                out.push_str(&format!(" line=\"{line}\""));
            }
            out.push_str(">\n");
            for (tag, text) in [
                ("statement", &v.statement),
                ("why", &v.why),
                ("expected", &v.expected),
                ("fix", &v.fix),
            ] {
                out.push_str(&format!("    <{tag}>{}</{tag}>\n", escape(text)));
            }
            out.push_str(&format!("    <example path=\"{}\"/>\n", escape(&v.example)));
            out.push_str("  </violation>\n");
        }
        out.push_str("</bebop-report>\n");
        out
    }

    pub fn text(&self) -> String {
        if self.is_clean() {
            return format!("bebop: {} rules checked, no violations.\n", self.rules);
        }
        let mut out = format!(
            "bebop: {} rules checked, {} {}.\n",
            self.rules,
            self.violations.len(),
            plural(self.violations.len(), "violation", "violations")
        );
        let mut at = 0;
        while at < self.violations.len() {
            let first = &self.violations[at];
            let group: Vec<&Violation> = self.violations[at..]
                .iter()
                .take_while(|v| v.rule == first.rule)
                .collect();
            at += group.len();
            out.push_str(&format!(
                "\n{} ({}, {}): {} {}\n",
                first.rule,
                first.category,
                first.severity,
                group.len(),
                plural(group.len(), "violation", "violations")
            ));
            out.push_str(&format!("  Rule: {}\n", one_line(&first.statement)));
            out.push_str(&format!("  Why: {}\n", one_line(&first.why)));
            for v in group {
                match v.line {
                    Some(line) => out.push_str(&format!("  - {}:{line}\n", v.file)),
                    None => out.push_str(&format!("  - {}\n", v.file)),
                }
                out.push_str(&format!("      Expected: {}\n", one_line(&v.expected)));
                out.push_str(&format!("      Fix: {}\n", one_line(&v.fix)));
            }
            out.push_str(&format!("  Example: {}\n", first.example));
        }
        out
    }
}

fn plural<'a>(n: usize, one: &'a str, many: &'a str) -> &'a str {
    if n == 1 { one } else { many }
}

/// A text on one line, its runs of space and line breaks made single spaces.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `text` as XML content or an attribute value.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if (c as u32) < 0x20 || c == '\u{FFFE}' || c == '\u{FFFF}' => out.push('\u{FFFD}'),
            c => out.push(c),
        }
    }
    out
}
