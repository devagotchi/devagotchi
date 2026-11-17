//! File requests: answered directly (no operation, nothing journaled) where the broker's
//! policy lets files be reached, on the blocking pool so a large file or a long search never
//! stalls the broker's jobs, waits or other callers. A refusal goes to the broker log.

use super::*;
use crate::files::Scope;

impl Broker {
    pub(super) async fn on_files(&self, request: RequestBody) -> BridgeResult<ResponseBody> {
        let workspace = self.shared.workspace.clone();
        let policy = self.shared.config.exec_policy;
        let (what, done) = tokio::task::spawn_blocking(move || {
            let files = Scope {
                workspace: &workspace,
                policy,
            };
            answer(&files, request)
        })
        .await
        .map_err(|error| BridgeError::new("internal_error", error.to_string()))?;
        if let Err(error) = &done {
            trace::refused(what, error);
        }
        done
    }
}

/// The request's tool name, and its answer.
fn answer(files: &Scope<'_>, request: RequestBody) -> (&'static str, BridgeResult<ResponseBody>) {
    match request {
        RequestBody::FileRead(input) => ("read_file", files.read(input).map(ResponseBody::File)),
        RequestBody::FileList(input) => (
            "list_directory",
            files.list(input).map(ResponseBody::Listing),
        ),
        RequestBody::FileWrite(input) => {
            ("write_file", files.write(input).map(ResponseBody::Written))
        }
        RequestBody::FileEdit(input) => ("edit_file", files.edit(input).map(ResponseBody::Written)),
        RequestBody::FileInfo(input) => {
            ("get_file_info", files.info(input).map(ResponseBody::Info))
        }
        RequestBody::FileMkdir(input) => (
            "create_directory",
            files.mkdir(input).map(ResponseBody::Info),
        ),
        RequestBody::FileMove(input) => ("move_file", files.moved(input).map(ResponseBody::Info)),
        RequestBody::FileSearch(input) => (
            "search_files",
            files.search(input).map(ResponseBody::Search),
        ),
        _ => (
            "file request",
            Err(BridgeError::new("internal_error", "not a file request")),
        ),
    }
}
