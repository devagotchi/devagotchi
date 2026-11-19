//! The file tools: read, list, search, write, edit, inspect, make directories and move, each
//! one request to this user's broker; `read_multiple_files` and `read_image` are several.

use devagotchi_contracts::describe;

use devagotchi_contracts::{
    BridgeError, DirectoryInput, FileContent, FileEditInput, FileInfo, FileInfoInput,
    FileListInput, FileListing, FileMoveInput, FileReadInput, FileSearchInput, FileWriteInput,
    FileWritten, RequestBody, ResponseBody, SearchPage,
};
use rmcp::{
    Json, RoleServer, handler::server::wrapper::Parameters, model::CallToolResult,
    service::RequestContext, tool, tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;

use super::BridgeAdapter;
use crate::{files, image, unexpected};

/// What `search_files` says on this machine (the hub says the same of a named machine).

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadFilesInput {
    /// Paths (absolute, `~/...`, or relative to the workspace) of the files, 1 to 20.
    pub paths: Vec<String>,
    /// Bytes to read from the start of each, 1 to 24576 (default 24576). Read on from a
    /// file's `next_offset` with read_file.
    #[serde(default)]
    pub max_bytes_each: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImageInput {
    /// Path (absolute, `~/...`, or relative to the workspace) of a PNG, JPEG, GIF or WebP image, up to 3.75 MB.
    pub path: String,
}

#[tool_router(router = files_router, vis = "pub(super)")]
impl BridgeAdapter {
    #[tool(
        name = "read_file",
        description = describe::READ_FILE,
        title = "Read a file",
        annotations(title = "Read a file", read_only_hint = true, open_world_hint = false)
    )]
    async fn read_file(
        &self,
        Parameters(input): Parameters<FileReadInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<FileContent>, Json<BridgeError>> {
        match self.request(RequestBody::FileRead(input), context).await? {
            ResponseBody::File(content) => Ok(Json(content)),
            other => Err(Json(unexpected("file", &other))),
        }
    }

    #[tool(
        name = "list_directory",
        description = describe::LIST_DIRECTORY,
        title = "List a directory",
        annotations(
            title = "List a directory",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn list_directory(
        &self,
        Parameters(input): Parameters<FileListInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<FileListing>, Json<BridgeError>> {
        match self.request(RequestBody::FileList(input), context).await? {
            ResponseBody::Listing(listing) => Ok(Json(listing)),
            other => Err(Json(unexpected("listing", &other))),
        }
    }

    #[tool(
        name = "search_files",
        description = describe::SEARCH_FILES,
        title = "Search files",
        annotations(title = "Search files", read_only_hint = true, open_world_hint = false)
    )]
    async fn search_files(
        &self,
        Parameters(input): Parameters<FileSearchInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<SearchPage>, Json<BridgeError>> {
        match self
            .request(RequestBody::FileSearch(input), context)
            .await?
        {
            ResponseBody::Search(page) => Ok(Json(page)),
            other => Err(Json(unexpected("search", &other))),
        }
    }

    #[tool(
        name = "write_file",
        description = describe::WRITE_FILE,
        title = "Write a file",
        annotations(
            title = "Write a file",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn write_file(
        &self,
        Parameters(input): Parameters<FileWriteInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<FileWritten>, Json<BridgeError>> {
        match self.request(RequestBody::FileWrite(input), context).await? {
            ResponseBody::Written(written) => Ok(Json(written)),
            other => Err(Json(unexpected("written", &other))),
        }
    }

    #[tool(
        name = "edit_file",
        description = describe::EDIT_FILE,
        title = "Edit a file",
        annotations(
            title = "Edit a file",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn edit_file(
        &self,
        Parameters(input): Parameters<FileEditInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<FileWritten>, Json<BridgeError>> {
        match self.request(RequestBody::FileEdit(input), context).await? {
            ResponseBody::Written(written) => Ok(Json(written)),
            other => Err(Json(unexpected("written", &other))),
        }
    }

    #[tool(
        name = "read_multiple_files",
        description = describe::READ_MULTIPLE_FILES,
        title = "Read several files",
        annotations(
            title = "Read several files",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn read_multiple_files(
        &self,
        Parameters(input): Parameters<ReadFilesInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<files::Files>, Json<BridgeError>> {
        let read = files::read_many(
            |body| devagotchi_ipc::request(&self.socket, body),
            input.paths,
            input.max_bytes_each,
        );
        tokio::select! {
            biased;
            _ = context.ct.cancelled() => Err(Json(BridgeError::new(
                "request_cancelled",
                "stopped reading; nothing was changed",
            ))),
            files = read => files.map(Json).map_err(Json),
        }
    }

    #[tool(
        name = "read_image",
        description = describe::READ_IMAGE,
        title = "Look at an image",
        annotations(
            title = "Look at an image",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn read_image(
        &self,
        Parameters(input): Parameters<ImageInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, Json<BridgeError>> {
        let read = image::read_image(
            |body| devagotchi_ipc::request(&self.socket, body),
            input.path,
        );
        tokio::select! {
            biased;
            _ = context.ct.cancelled() => Err(Json(BridgeError::new(
                "request_cancelled",
                "stopped reading; nothing was changed",
            ))),
            image = read => image.map(image::Image::into_result).map_err(Json),
        }
    }

    #[tool(
        name = "get_file_info",
        description = describe::GET_FILE_INFO,
        title = "Get file info",
        annotations(
            title = "Get file info",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn get_file_info(
        &self,
        Parameters(input): Parameters<FileInfoInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<FileInfo>, Json<BridgeError>> {
        match self.request(RequestBody::FileInfo(input), context).await? {
            ResponseBody::Info(info) => Ok(Json(info)),
            other => Err(Json(unexpected("info", &other))),
        }
    }

    #[tool(
        name = "create_directory",
        description = describe::CREATE_DIRECTORY,
        title = "Make a directory",
        annotations(
            title = "Make a directory",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn create_directory(
        &self,
        Parameters(input): Parameters<DirectoryInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<FileInfo>, Json<BridgeError>> {
        match self.request(RequestBody::FileMkdir(input), context).await? {
            ResponseBody::Info(info) => Ok(Json(info)),
            other => Err(Json(unexpected("info", &other))),
        }
    }

    #[tool(
        name = "move_file",
        description = describe::MOVE_FILE,
        title = "Move or rename",
        annotations(
            title = "Move or rename",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn move_file(
        &self,
        Parameters(input): Parameters<FileMoveInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<FileInfo>, Json<BridgeError>> {
        match self.request(RequestBody::FileMove(input), context).await? {
            ResponseBody::Info(info) => Ok(Json(info)),
            other => Err(Json(unexpected("info", &other))),
        }
    }
}
