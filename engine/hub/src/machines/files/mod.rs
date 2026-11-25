//! `read_file` and `list_directory`: a machine's files through its broker, one call each
//! (box 1 of the 25 Oct order). The broker decides reach: inside its workspace when it runs
//! only granted programs, anywhere its account can read under the `account` policy.

use devagotchi_contracts::describe;

use devagotchi_contracts::{
    BridgeError, DirectoryInput, FileContent, FileDeleteInput as DeleteInput, FileDeleted,
    FileEditInput, FileInfo, FileInfoInput, FileIntent, FileInvocation, FileListInput, FileListing,
    FileMoveInput, FileReadInput, FileWriteInput, FileWritten, RequestBody, ResponseBody,
};
use devagotchi_mcp::{
    files::{Files, read_many, read_many_authorized},
    image::read_image,
};
use rmcp::{
    Json, RoleServer, handler::server::wrapper::Parameters, model::CallToolResult,
    service::RequestContext, tool, tool_router,
};

use crate::mcp::{FileRetryKey, Hub, read_key, unexpected_response};

mod inputs;
use inputs::*;

#[tool_router(router = files_router, vis = "pub(crate)")]
impl Hub {
    #[tool(
        name = "read_file",
        description = describe::READ_FILE,
        title = "Read a file on a machine",
        annotations(
            title = "Read a file on a machine",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn read_file(
        &self,
        Parameters(input): Parameters<ReadFileInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<FileContent>, Json<BridgeError>> {
        let body = RequestBody::FileRead(FileReadInput {
            path: input.path,
            offset: input.offset,
            max_bytes: input.max_bytes,
            start_line: input.start_line,
            line_count: input.line_count,
        });
        match self
            .call("read_file", &input.machine, body, context)
            .await?
        {
            ResponseBody::File(content) => Ok(Json(content)),
            other => Err(unexpected_response("file", &other)),
        }
    }

    #[tool(
        name = "list_directory",
        description = describe::LIST_DIRECTORY,
        title = "List a directory on a machine",
        annotations(
            title = "List a directory on a machine",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn list_directory(
        &self,
        Parameters(input): Parameters<ListDirectoryInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<FileListing>, Json<BridgeError>> {
        let body = RequestBody::FileList(FileListInput {
            path: input.path,
            depth: input.depth,
            max_entries: input.max_entries,
            cursor: input.cursor,
        });
        match self
            .call("list_directory", &input.machine, body, context)
            .await?
        {
            ResponseBody::Listing(listing) => Ok(Json(listing)),
            other => Err(unexpected_response("listing", &other)),
        }
    }
    #[tool(
        name = "write_file",
        description = describe::WRITE_FILE,
        title = "Write a file on a machine",
        annotations(
            title = "Write a file on a machine",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn write_file(
        &self,
        Parameters(input): Parameters<WriteFileInput>,
        mut context: RequestContext<RoleServer>,
    ) -> Result<Json<FileWritten>, Json<BridgeError>> {
        context
            .extensions
            .insert(FileRetryKey(input.idempotency_key));
        let body = RequestBody::FileWrite(FileWriteInput {
            path: input.path,
            text: input.text,
            bytes_base64: input.bytes_base64,
            mode: input.mode,
            expected_sha256: input.expected_sha256,
            create_dirs: input.create_dirs,
        });
        match self
            .call("write_file", &input.machine, body, context)
            .await?
        {
            ResponseBody::Written(written) => Ok(Json(written)),
            other => Err(unexpected_response("written", &other)),
        }
    }

    #[tool(
        name = "edit_file",
        description = describe::EDIT_FILE,
        title = "Edit a file on a machine",
        annotations(
            title = "Edit a file on a machine",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn edit_file(
        &self,
        Parameters(input): Parameters<EditFileInput>,
        mut context: RequestContext<RoleServer>,
    ) -> Result<Json<FileWritten>, Json<BridgeError>> {
        context
            .extensions
            .insert(FileRetryKey(input.idempotency_key));
        let body = RequestBody::FileEdit(FileEditInput {
            path: input.path,
            old_text: input.old_text,
            new_text: input.new_text,
            expected_replacements: input.expected_replacements,
        });
        match self
            .call("edit_file", &input.machine, body, context)
            .await?
        {
            ResponseBody::Written(written) => Ok(Json(written)),
            other => Err(unexpected_response("written", &other)),
        }
    }

    #[tool(
        name = "read_multiple_files",
        description = describe::READ_MULTIPLE_FILES,
        title = "Read several files on a machine",
        annotations(
            title = "Read several files on a machine",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn read_multiple_files(
        &self,
        Parameters(input): Parameters<ReadFilesInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<Files>, Json<BridgeError>> {
        let subject = self.subject(&context)?;
        // The audit names every path asked for, as read_file names its one.
        let asked = input.paths.join("; ");
        let route = match self.route(&subject, &input.machine).await {
            Ok(route) => route,
            Err(Json(error)) => {
                self.record_completion(
                    &subject,
                    "read_multiple_files",
                    &input.machine,
                    &asked,
                    &error.code,
                )
                .await;
                return Err(Json(error));
            }
        };
        let read = async {
            let session = self
                .file_session_for_route(
                    &route,
                    FileInvocation {
                        key: read_key()?,
                        intent: FileIntent::Batch {
                            paths: input.paths.clone(),
                            max_bytes: input
                                .max_bytes_each
                                .unwrap_or_else(devagotchi_contracts::default_file_read_bytes),
                        },
                    },
                    &context,
                )
                .await?;
            let result = if let Some(session) = &session {
                read_many_authorized(
                    |body| session.page(self, &context, body),
                    input.paths,
                    input.max_bytes_each,
                )
                .await
            } else {
                read_many(
                    |body| self.send(&route, body),
                    input.paths,
                    input.max_bytes_each,
                )
                .await
            };
            if let Some(session) = &session {
                session.finish(self, &context).await?;
            }
            result
        };
        let files = tokio::select! {
            biased;
            _ = context.ct.cancelled() => Err(BridgeError::new(
                "request_cancelled",
                "stopped reading; nothing was changed",
            )),
            files = read => files,
        };
        let code = match &files {
            Ok(_) => "ok",
            Err(error) => error.code.as_str(),
        };
        self.record_completion(
            &subject,
            "read_multiple_files",
            &input.machine,
            &asked,
            code,
        )
        .await;
        files.map(Json).map_err(Json)
    }

    #[tool(
        name = "get_file_info",
        description = describe::GET_FILE_INFO,
        title = "Get file info on a machine",
        annotations(
            title = "Get file info on a machine",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn get_file_info(
        &self,
        Parameters(input): Parameters<PathOnMachine>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<FileInfo>, Json<BridgeError>> {
        let body = RequestBody::FileInfo(FileInfoInput {
            path: input.path,
            sha256: input.sha256,
        });
        match self
            .call("get_file_info", &input.machine, body, context)
            .await?
        {
            ResponseBody::Info(info) => Ok(Json(info)),
            other => Err(unexpected_response("info", &other)),
        }
    }

    #[tool(
        name = "create_directory",
        description = describe::CREATE_DIRECTORY,
        title = "Make a directory on a machine",
        annotations(
            title = "Make a directory on a machine",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn create_directory(
        &self,
        Parameters(input): Parameters<DirectoryOnMachine>,
        mut context: RequestContext<RoleServer>,
    ) -> Result<Json<FileInfo>, Json<BridgeError>> {
        context
            .extensions
            .insert(FileRetryKey(input.idempotency_key));
        let body = RequestBody::FileMkdir(DirectoryInput { path: input.path });
        match self
            .call("create_directory", &input.machine, body, context)
            .await?
        {
            ResponseBody::Info(info) => Ok(Json(info)),
            other => Err(unexpected_response("info", &other)),
        }
    }

    #[tool(
        name = "move_file",
        description = describe::MOVE_FILE,
        title = "Move or rename on a machine",
        annotations(
            title = "Move or rename on a machine",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn move_file(
        &self,
        Parameters(input): Parameters<MoveFileInput>,
        mut context: RequestContext<RoleServer>,
    ) -> Result<Json<FileInfo>, Json<BridgeError>> {
        context
            .extensions
            .insert(FileRetryKey(input.idempotency_key));
        let body = RequestBody::FileMove(FileMoveInput {
            source: input.source,
            destination: input.destination,
            overwrite: input.overwrite,
            expected_sha256: input.expected_sha256,
        });
        match self
            .call("move_file", &input.machine, body, context)
            .await?
        {
            ResponseBody::Info(info) => Ok(Json(info)),
            other => Err(unexpected_response("info", &other)),
        }
    }

    #[tool(name = "delete_file", description = describe::DELETE_FILE, title = "Delete a file on a machine",
        annotations(title = "Delete a file on a machine", read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false))]
    async fn delete_file(
        &self,
        Parameters(input): Parameters<DeleteFileInput>,
        mut context: RequestContext<RoleServer>,
    ) -> Result<Json<FileDeleted>, Json<BridgeError>> {
        context
            .extensions
            .insert(FileRetryKey(input.idempotency_key));
        let body = RequestBody::FileDelete(DeleteInput {
            path: input.path,
            expected_sha256: input.expected_sha256,
        });
        match self
            .call("delete_file", &input.machine, body, context)
            .await?
        {
            ResponseBody::Deleted(deleted) => Ok(Json(deleted)),
            other => Err(unexpected_response("deleted", &other)),
        }
    }

    #[tool(
        name = "read_image",
        description = describe::READ_IMAGE,
        title = "Look at an image on a machine",
        annotations(
            title = "Look at an image on a machine",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn read_image(
        &self,
        Parameters(input): Parameters<ImageOnMachine>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, Json<BridgeError>> {
        let subject = self.subject(&context)?;
        let route = match self.route(&subject, &input.machine).await {
            Ok(route) => route,
            Err(Json(error)) => {
                self.record_completion(
                    &subject,
                    "read_image",
                    &input.machine,
                    &input.path,
                    &error.code,
                )
                .await;
                return Err(Json(error));
            }
        };
        let read = async {
            let session = self
                .file_session_for_route(
                    &route,
                    FileInvocation {
                        key: read_key()?,
                        intent: FileIntent::Image {
                            path: input.path.clone(),
                        },
                    },
                    &context,
                )
                .await?;
            let result = if let Some(session) = &session {
                read_image(
                    |body| session.page(self, &context, body),
                    input.path.clone(),
                )
                .await
            } else {
                read_image(|body| self.send(&route, body), input.path.clone()).await
            };
            if let Some(session) = &session {
                session.finish(self, &context).await?;
            }
            result
        };
        let image = tokio::select! {
            biased;
            _ = context.ct.cancelled() => Err(BridgeError::new(
                "request_cancelled",
                "stopped reading; nothing was changed",
            )),
            image = read => image,
        };
        let code = match &image {
            Ok(_) => "ok",
            Err(error) => error.code.as_str(),
        };
        self.record_completion(&subject, "read_image", &input.machine, &input.path, code)
            .await;
        image.map(|image| image.into_result()).map_err(Json)
    }
}
