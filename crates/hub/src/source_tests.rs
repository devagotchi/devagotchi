//! Shared compiler-shaped source walker for hub convention tests.
use std::path::{Path, PathBuf};

/// One string literal: where it is and what it says.
pub(crate) struct Literal {
    pub(crate) line: usize,
    pub(crate) text: String,
}

/// What one Rust file holds: its non-test string literals and the modules it declares.
pub(crate) struct Scanned {
    pub(crate) literals: Vec<Literal>,
    /// `(name, declared under #[cfg(test)], #[path])`.
    pub(crate) modules: Vec<(String, bool, Option<String>)>,
}

pub(crate) fn scan(text: &str) -> Scanned {
    let chars: Vec<char> = text.chars().collect();
    let at = |i: usize| chars.get(i).copied().unwrap_or('\0');
    let before = |i: usize| i.checked_sub(1).map(|j| chars[j]).unwrap_or(' ');
    let starts = |i: usize, s: &str| s.chars().enumerate().all(|(k, c)| at(i + k) == c);
    let mut out = Scanned {
        literals: Vec::new(),
        modules: Vec::new(),
    };
    let (mut i, mut line, mut depth) = (0usize, 1usize, 0usize);
    // Brackets and parentheses: a `;` inside them (`[&str; 2]`) ends no item.
    let mut nest = 0usize;
    // An item under #[cfg(test)]: the depth it started at, and whether its body opened.
    let mut test_item: Option<(usize, bool)> = None;
    let mut pending_path: Option<String> = None;
    while i < chars.len() {
        let c = chars[i];
        let hidden = test_item.is_some();
        if c == '\n' {
            line += 1;
            i += 1;
            continue;
        }
        if starts(i, "//") {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if starts(i, "/*") {
            let mut nest = 0;
            while i < chars.len() {
                if starts(i, "/*") {
                    nest += 1;
                    i += 2;
                } else if starts(i, "*/") {
                    nest -= 1;
                    i += 2;
                    if nest == 0 {
                        break;
                    }
                } else {
                    if chars[i] == '\n' {
                        line += 1;
                    }
                    i += 1;
                }
            }
            continue;
        }
        let word_edge = !before(i).is_alphanumeric() && before(i) != '_';
        // Raw strings: r"…", r#"…"#, br#"…"#.
        let raw_start = if word_edge && starts(i, "br") && matches!(at(i + 2), '"' | '#') {
            Some(i + 2)
        } else if word_edge && c == 'r' && matches!(at(i + 1), '"' | '#') {
            Some(i + 1)
        } else {
            None
        };
        if let Some(mut j) = raw_start {
            let mut hashes = 0;
            while at(j) == '#' {
                hashes += 1;
                j += 1;
            }
            if at(j) == '"' {
                let begin = line;
                j += 1;
                let mut body = String::new();
                while j < chars.len() {
                    if chars[j] == '"' && (1..=hashes).all(|k| at(j + k) == '#') {
                        j += 1 + hashes;
                        break;
                    }
                    if chars[j] == '\n' {
                        line += 1;
                    }
                    body.push(chars[j]);
                    j += 1;
                }
                if !hidden {
                    out.literals.push(Literal {
                        line: begin,
                        text: body,
                    });
                }
                i = j;
                continue;
            }
        }
        if c == '"' || (c == 'b' && word_edge && at(i + 1) == '"') {
            let begin = line;
            let mut j = if c == 'b' { i + 2 } else { i + 1 };
            let mut body = String::new();
            while j < chars.len() && chars[j] != '"' {
                if chars[j] == '\\' {
                    body.push(chars[j]);
                    j += 1;
                }
                if at(j) == '\n' {
                    line += 1;
                }
                body.push(at(j));
                j += 1;
            }
            if let Some(path) = pending_path.as_mut().filter(|p| p.is_empty()) {
                *path = body.clone();
            }
            if !hidden {
                out.literals.push(Literal {
                    line: begin,
                    text: body,
                });
            }
            i = j + 1;
            continue;
        }
        if c == '\'' {
            // A char literal ('x', '\n', '\'') or a lifetime ('a).
            if at(i + 1) == '\\' {
                let mut j = i + 3;
                while j < chars.len() && chars[j] != '\'' {
                    j += 1;
                }
                i = j + 1;
            } else if at(i + 2) == '\'' {
                i += 3;
            } else {
                i += 1;
            }
            continue;
        }
        if c == '#' && (at(i + 1) == '[' || (at(i + 1) == '!' && at(i + 2) == '[')) {
            let open = if at(i + 1) == '[' { i + 2 } else { i + 3 };
            let compact: String = chars[open..chars.len().min(open + 64)]
                .iter()
                .filter(|c| !c.is_whitespace())
                .collect();
            if compact.starts_with("cfg(test)]") || compact.starts_with("cfg(all(test") {
                if test_item.is_none() {
                    test_item = Some((depth, false));
                }
            } else if compact.starts_with("path=") {
                pending_path = Some(String::new());
            }
            i += 1;
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let mut j = i;
            while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_') {
                j += 1;
            }
            let word: String = chars[i..j].iter().collect();
            if word == "mod" {
                let mut k = j;
                while at(k).is_whitespace() {
                    k += 1;
                }
                let name_start = k;
                while at(k).is_alphanumeric() || at(k) == '_' {
                    k += 1;
                }
                let name: String = chars[name_start..k].iter().collect();
                while at(k).is_whitespace() {
                    k += 1;
                }
                if at(k) == ';' && !name.is_empty() {
                    let path = pending_path.take().filter(|p| !p.is_empty());
                    out.modules.push((name, test_item.is_some(), path));
                }
            }
            i = j;
            continue;
        }
        match c {
            '{' => {
                if let Some((start, false)) = test_item
                    && start == depth
                {
                    test_item = Some((start, true));
                }
                depth += 1;
            }
            '}' => {
                depth = depth.saturating_sub(1);
                if let Some((start, true)) = test_item
                    && depth == start
                {
                    test_item = None;
                }
            }
            '(' | '[' => nest += 1,
            ')' | ']' => nest = nest.saturating_sub(1),
            ';' if nest == 0 => {
                if let Some((start, false)) = test_item
                    && start == depth
                {
                    test_item = None;
                }
                pending_path = None;
            }
            _ => {}
        }
        i += 1;
    }
    out
}

/// Where `mod name;` in `file` lives.
pub(crate) fn child(file: &Path, name: &str, path: Option<&str>) -> PathBuf {
    let parent = file.parent().expect("a file has a directory");
    if let Some(path) = path {
        return parent.join(path);
    }
    let stem = file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    let dir = if matches!(stem, "lib" | "mod" | "main") {
        parent.to_path_buf()
    } else {
        parent.join(stem)
    };
    let flat = dir.join(format!("{name}.rs"));
    if flat.exists() {
        flat
    } else {
        dir.join(name).join("mod.rs")
    }
}


pub(crate) fn sources() -> Vec<(PathBuf, String, bool)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    let mut queue = vec![(root.join("lib.rs"), false)];
    while let Some((file, test_only)) = queue.pop() {
        let text = std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        for (name, in_test, path) in scan(&text).modules {
            queue.push((child(&file, &name, path.as_deref()), test_only || in_test));
        }
        files.push((file, text, test_only));
    }
    files
}
