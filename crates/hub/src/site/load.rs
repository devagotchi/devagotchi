//! Reading a site: from its directory, or from the built-in default compiled into the
//! binary, into one checked [`Site`].

use super::{
    Asset, Landing, MAX_TEXT_BYTES, Messages, PAGES, Page, Site, SiteFile, TEXT_FILES,
    WorkflowsFile, assets, style,
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// The site's files as read: text by path, assets by file name.
#[derive(Clone, Default)]
pub(super) struct Files {
    text: BTreeMap<String, String>,
    assets: BTreeMap<String, axum::body::Bytes>,
}

macro_rules! builtin_text {
    ($($path:literal),* $(,)?) => {
        [$(($path, include_str!(concat!("../../site/default/", $path)))),*]
    };
}
macro_rules! builtin_assets {
    ($($name:literal),* $(,)?) => {
        [$(($name, &include_bytes!(concat!("../../site/default/assets/", $name))[..])),*]
    };
}

pub(super) fn builtin_files() -> Files {
    let text = builtin_text!(
        "site.json",
        "tokens.json",
        "content/landing.json",
        "content/messages.json",
        "content/workflows.json",
        "content/docs.md",
        "content/privacy.md",
        "content/terms.md",
        "content/support.md",
        "content/llms.md",
    );
    let assets = builtin_assets!(
        "logo.svg",
        "icon.svg",
        "icon-192.png",
        "icon-512.png",
        "icon-maskable-512.png",
        "badge-96.png",
        "pet.svg",
        "diagram.svg",
        "preview.png"
    );
    Files {
        text: text
            .into_iter()
            .map(|(path, body)| (path.to_string(), body.to_string()))
            .collect(),
        assets: assets
            .into_iter()
            .map(|(name, bytes)| (name.to_string(), axum::body::Bytes::from_static(bytes)))
            .collect(),
    }
}

impl Site {
    /// Read and check the site directory at `dir` (absolute). The error says which file
    /// and what in it is wrong.
    pub fn load(dir: &Path) -> Result<Self, String> {
        if !dir.is_absolute() {
            return Err(format!("{} is not an absolute path", dir.display()));
        }
        let meta = std::fs::metadata(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
        if !meta.is_dir() {
            return Err(format!("{} is not a directory", dir.display()));
        }
        if !dir.join("site.json").is_file() {
            return Err(format!(
                "{} has no site.json (the one file a site directory must have)",
                dir.display()
            ));
        }
        let mut files = builtin_files();
        for path in TEXT_FILES {
            let file = dir.join(path);
            if !file.exists() {
                continue;
            }
            let text = read_text(&file)?;
            let merged = match path {
                // Tokens and messages may give only what they change.
                "tokens.json" | "content/messages.json" => {
                    merge_json(&files.text[path], &text).map_err(|why| format!("{path}: {why}"))?
                }
                _ => text,
            };
            files.text.insert(path.to_string(), merged);
        }
        let content = dir.join("content");
        if content.is_dir() {
            for entry in read_dir(&content)? {
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.starts_with('.')
                    && !TEXT_FILES.contains(&format!("content/{name}").as_str())
                {
                    return Err(format!(
                        "content/{name} is not a file a site reads (landing.json, messages.json, workflows.json, docs.md, privacy.md, terms.md, support.md, llms.md)"
                    ));
                }
            }
        }
        let assets = dir.join("assets");
        if assets.is_dir() {
            let mut total = 0u64;
            for entry in read_dir(&assets)? {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }
                let path = entry.path();
                let meta =
                    std::fs::metadata(&path).map_err(|error| format!("assets/{name}: {error}"))?;
                if !meta.is_file() {
                    return Err(format!(
                        "assets/{name}: assets is one flat directory of files"
                    ));
                }
                assets::check_name(&name).map_err(|why| format!("assets/{name}: {why}"))?;
                if meta.len() > assets::MAX_ASSET_BYTES {
                    return Err(format!(
                        "assets/{name} is {} bytes; an asset is at most {}",
                        meta.len(),
                        assets::MAX_ASSET_BYTES
                    ));
                }
                total += meta.len();
                if total > assets::MAX_TOTAL_BYTES {
                    return Err(format!(
                        "assets/ holds more than {} bytes",
                        assets::MAX_TOTAL_BYTES
                    ));
                }
                let bytes =
                    std::fs::read(&path).map_err(|error| format!("assets/{name}: {error}"))?;
                files.assets.insert(name, bytes.into());
            }
        }
        Self::from_files(files, Some(dir.to_path_buf()))
    }

    pub(super) fn from_files(files: Files, dir: Option<PathBuf>) -> Result<Self, String> {
        let text = |path: &str| files.text.get(path).map(String::as_str).unwrap_or_default();
        let info: SiteFile = parse_json("site.json", text("site.json"))?;
        let landing: Landing = parse_json("content/landing.json", text("content/landing.json"))?;
        let messages: Messages =
            parse_json("content/messages.json", text("content/messages.json"))?;
        let workflows: WorkflowsFile =
            parse_json("content/workflows.json", text("content/workflows.json"))?;
        let tokens: style::Tokens = parse_json("tokens.json", text("tokens.json"))?;
        let mut pages = BTreeMap::new();
        for name in PAGES {
            let path = format!("content/{name}.md");
            let page = Page::parse(text(&path)).map_err(|why| format!("{path}: {why}"))?;
            pages.insert(name.to_string(), page);
        }
        let llms = Page::parse(text("content/llms.md"))
            .map_err(|why| format!("content/llms.md: {why}"))?;
        let mut loaded = BTreeMap::new();
        for (name, bytes) in files.assets {
            let asset = Asset::new(&name, bytes).map_err(|why| format!("assets/{name}: {why}"))?;
            loaded.insert(name, asset);
        }
        let style = style::Style::build(&tokens, &info, &loaded)?;
        // Only what the site uses is served: a file left in assets/ (or a built-in one this
        // site does not name) stays off the web.
        let used: Vec<&String> = (info.assets.all())
            .chain(tokens.font_files.iter().map(|font| &font.file))
            .collect();
        loaded.retain(|name, _| used.contains(&name));
        let site = Self {
            dir,
            info,
            landing,
            messages,
            workflows,
            pages,
            llms,
            assets: loaded,
            style,
        };
        site.check()?;
        Ok(site)
    }
}

fn read_dir(dir: &Path) -> Result<Vec<std::fs::DirEntry>, String> {
    let mut entries: Vec<std::fs::DirEntry> = std::fs::read_dir(dir)
        .map_err(|error| format!("{}: {error}", dir.display()))?
        .collect::<Result<_, _>>()
        .map_err(|error| format!("{}: {error}", dir.display()))?;
    entries.sort_by_key(|entry| entry.file_name());
    Ok(entries)
}

fn read_text(file: &Path) -> Result<String, String> {
    let meta = std::fs::metadata(file).map_err(|error| format!("{}: {error}", file.display()))?;
    if meta.len() > MAX_TEXT_BYTES {
        return Err(format!(
            "{} is {} bytes; a text file of a site is at most {MAX_TEXT_BYTES}",
            file.display(),
            meta.len()
        ));
    }
    std::fs::read_to_string(file).map_err(|error| format!("{}: {error}", file.display()))
}

fn parse_json<T: serde::de::DeserializeOwned>(path: &str, text: &str) -> Result<T, String> {
    serde_json::from_str(text).map_err(|error| format!("{path}: {error}"))
}

/// `over` on top of `base`: objects merge key by key, anything else replaces.
fn merge_json(base: &str, over: &str) -> Result<String, String> {
    fn merge(base: &mut serde_json::Value, over: serde_json::Value) {
        match (base, over) {
            (serde_json::Value::Object(base), serde_json::Value::Object(over)) => {
                for (key, value) in over {
                    match base.get_mut(&key) {
                        Some(slot) => merge(slot, value),
                        None => {
                            base.insert(key, value);
                        }
                    }
                }
            }
            (slot, value) => *slot = value,
        }
    }
    let mut base: serde_json::Value = serde_json::from_str(base).map_err(|e| e.to_string())?;
    let over: serde_json::Value = serde_json::from_str(over).map_err(|e| e.to_string())?;
    if !over.is_object() {
        return Err("must be a JSON object".into());
    }
    merge(&mut base, over);
    Ok(base.to_string())
}
