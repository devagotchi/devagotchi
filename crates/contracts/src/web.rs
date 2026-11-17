//! `read_url`: a web address read from the machine, so a dev server on its localhost is as
//! reachable as the internet. Paged like `read_file`: each call fetches again and returns the
//! bytes from `offset`, so nothing is held open between calls.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The most of one response a read looks through: an offset past it is refused.
pub const MAX_URL_SCAN_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UrlReadInput {
    /// An http or https address.
    pub url: String,
    /// Where to start in the response body (the previous page's `next_offset`).
    #[serde(default)]
    pub offset: u64,
    /// Bytes to return, 1..=24576 (default 24576).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UrlContent {
    /// The address that answered, after redirects.
    pub url: String,
    /// The HTTP status code.
    pub status: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    /// The body's length, when the server said it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    pub offset: u64,
    /// The bytes as text, when they are UTF-8 text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// The raw bytes, base64, when they are not text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes_base64: Option<String>,
    pub next_offset: u64,
    /// Whether this page reached the end of the body.
    pub eof: bool,
}
