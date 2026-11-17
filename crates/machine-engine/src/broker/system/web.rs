//! `read_url`: a web address fetched by the machine, so what it reaches (a dev server on its
//! localhost, a service on its network, the internet through its proxy) is what an agent
//! reads. Each page fetches again from `offset` (with a `Range` request, which a server may
//! ignore, so the bytes before `offset` are skipped by hand then), and nothing stays open.

use super::*;
use crate::files::text_prefix;
use reqwest::header::{CONTENT_RANGE, CONTENT_TYPE, RANGE};
use std::sync::OnceLock;

/// How long one fetch may take, connecting through the last byte.
const FETCH_TIME: Duration = Duration::from_secs(20);
const REDIRECTS: usize = 5;

fn client() -> BridgeResult<&'static reqwest::Client> {
    static CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .user_agent(concat!("devagotchi/", env!("CARGO_PKG_VERSION")))
                .redirect(reqwest::redirect::Policy::limited(REDIRECTS))
                .timeout(FETCH_TIME)
                .build()
                .map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(|error| BridgeError::new("internal_error", format!("No web client: {error}")))
}

pub(super) async fn read(input: UrlReadInput) -> BridgeResult<UrlContent> {
    let most = input.max_bytes.unwrap_or(MAX_FILE_READ_BYTES);
    if !(1..=MAX_FILE_READ_BYTES).contains(&most) {
        return Err(invalid(format!(
            "max_bytes must be 1..={MAX_FILE_READ_BYTES}"
        )));
    }
    if input.offset > MAX_URL_SCAN_BYTES {
        return Err(invalid(format!(
            "offset must be at most {MAX_URL_SCAN_BYTES}"
        )));
    }
    let url = reqwest::Url::parse(&input.url)
        .map_err(|error| invalid(format!("{} is not a web address: {error}", input.url)))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(invalid(format!(
            "read_url reads http and https addresses, not {}:",
            url.scheme()
        )));
    }
    let mut request = client()?.get(url);
    if input.offset > 0 {
        request = request.header(RANGE, format!("bytes={}-", input.offset));
    }
    let mut response = request.send().await.map_err(failed)?;
    let status = response.status();
    let header = |name| {
        response
            .headers()
            .get(name)
            .and_then(|value: &reqwest::header::HeaderValue| value.to_str().ok())
            .map(str::to_owned)
    };
    let content_type = header(CONTENT_TYPE);
    // 206 starts the body at `offset`; anything else starts at 0 and is skipped through.
    let (mut skip, size) = if status == reqwest::StatusCode::PARTIAL_CONTENT {
        let total =
            header(CONTENT_RANGE).and_then(|range| range.rsplit('/').next()?.parse::<u64>().ok());
        (0, total)
    } else if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
        let total =
            header(CONTENT_RANGE).and_then(|range| range.rsplit('/').next()?.parse::<u64>().ok());
        return Ok(page(
            &response,
            status.as_u16(),
            content_type,
            total,
            input.offset,
            Vec::new(),
            true,
        ));
    } else {
        (input.offset, response.content_length())
    };
    // One byte past the page tells whether the body goes on.
    let mut bytes = Vec::with_capacity(most + 1);
    let mut ended = false;
    while bytes.len() <= most {
        let Some(chunk) = response.chunk().await.map_err(failed)? else {
            ended = true;
            break;
        };
        let mut chunk = &chunk[..];
        if skip > 0 {
            let dropped = (skip as usize).min(chunk.len());
            skip -= dropped as u64;
            chunk = &chunk[dropped..];
        }
        bytes.extend_from_slice(&chunk[..chunk.len().min(most + 1 - bytes.len())]);
    }
    let eof = ended && bytes.len() <= most;
    bytes.truncate(most);
    let content = page(
        &response,
        status.as_u16(),
        content_type,
        size,
        input.offset,
        bytes,
        eof,
    );
    trace::line(
        "read_url",
        &format!(
            "{} -> {} ({} bytes from {})",
            input.url,
            content.status,
            content.next_offset - content.offset,
            content.offset
        ),
    );
    Ok(content)
}

/// A page of `bytes` from `offset`: text when it reads as text (a character cut by the page's
/// end waits for the next page), base64 otherwise.
fn page(
    response: &reqwest::Response,
    status: u16,
    content_type: Option<String>,
    size: Option<u64>,
    offset: u64,
    bytes: Vec<u8>,
    eof: bool,
) -> UrlContent {
    let (text, bytes_base64, used) = match text_prefix(&bytes) {
        Some(length) if length > 0 || bytes.is_empty() => (
            Some(String::from_utf8_lossy(&bytes[..length]).into_owned()),
            None,
            length,
        ),
        _ => (None, Some(BASE64.encode(&bytes)), bytes.len()),
    };
    let next_offset = offset + used as u64;
    UrlContent {
        url: response.url().to_string(),
        status,
        content_type,
        size,
        offset,
        text,
        bytes_base64,
        next_offset,
        eof: eof && used == bytes.len(),
    }
}

fn failed(error: reqwest::Error) -> BridgeError {
    let code = if error.is_timeout() {
        "timeout"
    } else {
        "fetch_failed"
    };
    let mut message = error.to_string();
    let mut source = std::error::Error::source(&error);
    while let Some(cause) = source {
        message = format!("{message}: {cause}");
        source = cause.source();
    }
    BridgeError::new(code, message)
}
