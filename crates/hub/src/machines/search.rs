//! `search_files` on one of the user's machines: its broker searches (names, or lines inside
//! files) and answers one page at a time, with a cursor for the next.

use devagotchi_contracts::describe;

use devagotchi_contracts::{
    BridgeError, FileSearchInput, RequestBody, ResponseBody, SearchPage, SearchTarget,
};
use rmcp::{
    Json, RoleServer, handler::server::wrapper::Parameters, service::RequestContext, tool,
    tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::mcp::{Hub, unexpected_response};

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchFilesInput {
    /// The machine, by its name from `machines`.
    pub machine: String,
    /// Directory to search (absolute, `~/...`, or relative to the workspace).
    pub path: String,
    /// What to look for: plain text, or a regular expression with `regex`.
    pub pattern: String,
    /// `names` (default) or `contents`.
    #[serde(default)]
    pub target: SearchTarget,
    /// Read `pattern` as a regular expression (Rust syntax, like ripgrep's).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub regex: bool,
    /// Match case exactly; by default case is ignored.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub case_sensitive: bool,
    /// Only files whose name matches this glob, such as `*.rs` or `*.{ts,tsx}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glob: Option<String>,
    /// Also search hidden files and what .gitignore excludes (both are skipped by default,
    /// as git and ripgrep skip them).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub include_ignored: bool,
    /// Lines of context before and after each content match, 0..=5.
    #[serde(default)]
    pub context_lines: u32,
    /// Most matches on this page, 1..=500 (default 100).
    #[serde(default = "devagotchi_contracts::default_search_results")]
    pub max_results: u32,
    /// Where to go on from: the previous page's `next_cursor`. Absent for the first page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

#[tool_router(router = search_router, vis = "pub(crate)")]
impl Hub {
    #[tool(
        name = "search_files",
        description = describe::SEARCH_FILES,
        title = "Search files on a machine",
        annotations(
            title = "Search files on a machine",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn search_files(
        &self,
        Parameters(input): Parameters<SearchFilesInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<SearchPage>, Json<BridgeError>> {
        let body = RequestBody::FileSearch(FileSearchInput {
            path: input.path,
            pattern: input.pattern,
            target: input.target,
            regex: input.regex,
            case_sensitive: input.case_sensitive,
            glob: input.glob,
            include_ignored: input.include_ignored,
            context_lines: input.context_lines,
            max_results: input.max_results,
            cursor: input.cursor,
        });
        match self
            .call("search_files", &input.machine, body, context)
            .await?
        {
            ResponseBody::Search(page) => Ok(Json(page)),
            other => Err(unexpected_response("search", &other)),
        }
    }
}

