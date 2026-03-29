#![deny(clippy::all)]

use napi_derive::napi;
use gws_adapter_mcp::{McpServer, ManageEmailTool, ManageCalendarTool, ManageDriveTool, ManageDocsTool, ManageSheetsTool, ManageSlidesTool, ManageFormsTool, ManageTasksTool, ManageMeetTool, ManagePhotosTool, ManageNotebookLmTool, ManageAccountsTool};
use gws_adapter_google::{GoogleClient, NotebookLmClient, TokenStore};
use std::sync::Arc;
use napi::{Result, Error, Status};
use tokio::runtime::Runtime;

#[napi]
pub struct ServerWrapper {
    server: McpServer,
    #[allow(dead_code)]
    rt: Runtime,
}

#[napi]
impl ServerWrapper {
    #[napi(constructor)]
    pub fn new(client_id: String, client_secret: String) -> Result<Self> {
        let token_store = Arc::new(
            TokenStore::new(client_id, client_secret)
                .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?
        );
        let client = Arc::new(GoogleClient::new(token_store.clone()));
        let mut server = McpServer::new();

        server.register_tool(Arc::new(ManageAccountsTool::new(token_store.clone())));
        server.register_tool(Arc::new(ManageEmailTool::new(client.clone())));
        server.register_tool(Arc::new(ManageCalendarTool::new(client.clone())));
        server.register_tool(Arc::new(ManageDriveTool::new(client.clone())));
        server.register_tool(Arc::new(ManageDocsTool::new(client.clone())));
        server.register_tool(Arc::new(ManageSheetsTool::new(client.clone())));
        server.register_tool(Arc::new(ManageSlidesTool::new(client.clone())));
        server.register_tool(Arc::new(ManageFormsTool::new(client.clone())));
        server.register_tool(Arc::new(ManageTasksTool::new(client.clone())));
        server.register_tool(Arc::new(ManageMeetTool::new(client.clone())));
        server.register_tool(Arc::new(ManagePhotosTool::new(client.clone())));
        server.register_tool(Arc::new(ManageNotebookLmTool::new(Arc::new(NotebookLmClient::new()))));

        let rt = Runtime::new().map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?;
        Ok(Self { server, rt })
    }

    #[napi]
    pub fn list_tools(&self) -> Result<String> {
        serde_json::to_string(&self.server.list_tools())
            .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))
    }

    #[napi]
    pub async fn call_tool(&self, name: String, args_json: String) -> Result<String> {
        let args = serde_json::from_str(&args_json)
             .map_err(|e| Error::new(Status::InvalidArg, e.to_string()))?;
        let res = self.server.call_tool(&name, args).await
            .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?;
        serde_json::to_string(&res)
            .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))
    }
}
