//! Closed file invocation contracts; pages do not create separate admissions.
//! Owns: file intent, page bounds and result vocabulary; no persistence.
//! Serves: authenticated hub/broker file preparation, execution and receipts.
//! Settings: shared file/image size bounds; no operator configuration.
//! Depends on: file DTOs and grant actions. Native path enforcement belongs to the broker.
use super::{GrantAction, authority::token};
use crate::*;
use serde::{Deserialize, Serialize};

pub const REMOTE_FILE_ACCESS_VERSION: u32 = 1;
pub const MAX_FILE_BATCH: usize = 20;
pub const MAX_IMAGE_BYTES: u64 = 3_750_000;
pub const MAX_IMAGE_SIDE: u32 = 8000;
pub const MAX_FILE_PATH_BYTES: usize = 4096;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "method",
    content = "params",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum FileOperation {
    Read(FileReadInput),
    List(FileListInput),
    Info(FileInfoInput),
    Search(FileSearchInput),
    Write(FileWriteInput),
    Edit(FileEditInput),
    Mkdir(DirectoryInput),
    Move(FileMoveInput),
    Delete(FileDeleteInput),
}
impl FileOperation {
    pub fn from_request(request: RequestBody) -> BridgeResult<Self> {
        match request {
            RequestBody::FileRead(v) => Ok(Self::Read(v)),
            RequestBody::FileList(v) => Ok(Self::List(v)),
            RequestBody::FileInfo(v) => Ok(Self::Info(v)),
            RequestBody::FileSearch(v) => Ok(Self::Search(v)),
            RequestBody::FileWrite(v) => Ok(Self::Write(v)),
            RequestBody::FileEdit(v) => Ok(Self::Edit(v)),
            RequestBody::FileMkdir(v) => Ok(Self::Mkdir(v)),
            RequestBody::FileMove(v) => Ok(Self::Move(v)),
            RequestBody::FileDelete(v) => Ok(Self::Delete(v)),
            other => Err(invalid(format!("not a file operation: {}", other.target()))),
        }
    }
    pub fn request(self) -> RequestBody {
        match self {
            Self::Read(v) => RequestBody::FileRead(v),
            Self::List(v) => RequestBody::FileList(v),
            Self::Info(v) => RequestBody::FileInfo(v),
            Self::Search(v) => RequestBody::FileSearch(v),
            Self::Write(v) => RequestBody::FileWrite(v),
            Self::Edit(v) => RequestBody::FileEdit(v),
            Self::Mkdir(v) => RequestBody::FileMkdir(v),
            Self::Move(v) => RequestBody::FileMove(v),
            Self::Delete(v) => RequestBody::FileDelete(v),
        }
    }
    pub fn action(&self) -> GrantAction {
        match self {
            Self::Read(_) | Self::List(_) | Self::Info(_) | Self::Search(_) => {
                GrantAction::FileRead
            }
            Self::Write(_) | Self::Edit(_) | Self::Mkdir(_) | Self::Move(_) | Self::Delete(_) => {
                GrantAction::FileWrite
            }
        }
    }
    pub fn paths(&self) -> Vec<&str> {
        match self {
            Self::Read(v) => vec![&v.path],
            Self::List(v) => vec![&v.path],
            Self::Info(v) => vec![&v.path],
            Self::Search(v) => vec![&v.path],
            Self::Write(v) => vec![&v.path],
            Self::Edit(v) => vec![&v.path],
            Self::Mkdir(v) => vec![&v.path],
            Self::Move(v) => vec![&v.source, &v.destination],
            Self::Delete(v) => vec![&v.path],
        }
    }
    /// Account grants require explicit guards. Local trusted IPC retains its own contract.
    pub fn require_guard(&self) -> BridgeResult<()> {
        let guarded = match self {
            Self::Write(v) => match &v.expected_sha256 {
                Some(sha) => whole_sha(sha),
                None => v.mode == WriteMode::Create,
            },
            // A move that overwrites would replace a destination nobody guarded: under an
            // account grant, delete the destination with its own sha256 first, then move.
            Self::Move(v) => !v.overwrite && v.expected_sha256.as_deref().is_some_and(whole_sha),
            Self::Delete(v) => whole_sha(&v.expected_sha256),
            Self::Edit(v) => !v.old_text.is_empty() && v.expected_replacements > 0,
            Self::Read(_) | Self::List(_) | Self::Info(_) | Self::Search(_) | Self::Mkdir(_) => {
                true
            }
        };
        if !guarded {
            return Err(BridgeError::new(
                "file_guard_required",
                "get_file_info with sha256=true supplies a whole-file guard; create uses mode=create, an edit needs nonempty exact text and a positive replacement count, and a move never overwrites (delete the destination with its sha256 first)",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FileIntent {
    Operation {
        operation: FileOperation,
    },
    Batch {
        paths: Vec<String>,
        max_bytes: usize,
    },
    Image {
        path: String,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileInvocation {
    pub key: String,
    pub intent: FileIntent,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "step", rename_all = "snake_case", deny_unknown_fields)]
pub enum FileStep {
    Operation {},
    Read { input: FileReadInput },
}

/// A durable mutation result includes refusals so a retry cannot change its outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "status",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum FileOutcome {
    Ok(ResponseBody),
    Error(BridgeError),
}
impl FileOutcome {
    pub fn into_result(self) -> BridgeResult<ResponseBody> {
        match self {
            Self::Ok(response) => Ok(response),
            Self::Error(error) => Err(error),
        }
    }
    pub fn error_code(&self) -> Option<&str> {
        match self {
            Self::Ok(_) => None,
            Self::Error(error) => Some(&error.code),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum FileExecutionState {
    Active {
        expires_at: i64,
    },
    Finished {
        error_code: Option<String>,
    },
    /// A durable pending mutation has no provable result. Never execute it again.
    Uncertain {},
    Retired {},
}
impl FileInvocation {
    pub fn action(&self) -> GrantAction {
        match &self.intent {
            FileIntent::Operation { operation } => operation.action(),
            FileIntent::Batch { .. } | FileIntent::Image { .. } => GrantAction::FileRead,
        }
    }
    pub fn paths(&self) -> Vec<&str> {
        match &self.intent {
            FileIntent::Operation { operation } => operation.paths(),
            FileIntent::Batch { paths, .. } => paths.iter().map(String::as_str).collect(),
            FileIntent::Image { path } => vec![path],
        }
    }
    pub fn max_pages(&self) -> usize {
        match &self.intent {
            FileIntent::Operation { .. } => 1,
            FileIntent::Batch { paths, .. } => paths.len(),
            // The existing image helper may realign one UTF-8 boundary per page.
            FileIntent::Image { .. } => {
                2 * MAX_IMAGE_BYTES.div_ceil(MAX_FILE_READ_BYTES as u64) as usize + 1
            }
        }
    }
    pub fn validate(&self) -> BridgeResult<()> {
        if !token(&self.key, MAX_OPERATION_KEY_BYTES) {
            return Err(invalid(
                "file invocation needs a bounded retry key without control characters",
            ));
        }
        if self
            .paths()
            .iter()
            .any(|p| !token(p, MAX_FILE_PATH_BYTES) || p.trim().is_empty())
        {
            return Err(invalid(
                "file paths must be bounded nonempty names without control characters",
            ));
        }
        match &self.intent {
            FileIntent::Operation { operation } => operation.require_guard(),
            FileIntent::Batch { paths, max_bytes } => {
                if !(1..=MAX_FILE_BATCH).contains(&paths.len())
                    || !(1..=MAX_FILE_READ_BYTES).contains(max_bytes)
                {
                    return Err(invalid("file batch count or page size exceeds its bound"));
                }
                Ok(())
            }
            FileIntent::Image { .. } => Ok(()),
        }
    }
    /// Derive the actual request; a page cannot widen its immutable invocation.
    pub fn request_for(&self, step: &FileStep) -> BridgeResult<FileOperation> {
        self.validate()?;
        match (&self.intent, step) {
            (FileIntent::Operation { operation }, FileStep::Operation {}) => Ok(operation.clone()),
            (FileIntent::Batch { paths, max_bytes }, FileStep::Read { input })
                if paths.contains(&input.path)
                    && input.offset == 0
                    && input.max_bytes == *max_bytes
                    && input.start_line.is_none()
                    && input.line_count.is_none() =>
            {
                Ok(FileOperation::Read(input.clone()))
            }
            (FileIntent::Image { path }, FileStep::Read { input })
                if input.path == *path
                    && input.offset >= 0
                    && (input.offset as u64) < MAX_IMAGE_BYTES
                    && input.max_bytes == MAX_FILE_READ_BYTES
                    && input.start_line.is_none()
                    && input.line_count.is_none() =>
            {
                let mut input = input.clone();
                input.max_bytes = input
                    .max_bytes
                    .min((MAX_IMAGE_BYTES - input.offset as u64) as usize);
                Ok(FileOperation::Read(input))
            }
            _ => Err(BridgeError::new(
                "authority_mismatch",
                "file page differs from its admitted intent",
            )),
        }
    }
}

pub fn whole_sha(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
fn invalid(message: impl Into<String>) -> BridgeError {
    BridgeError::new("invalid_request", message)
}

#[cfg(test)]
mod tests;
