use pyo3::prelude::*;
use pyo3::exceptions::PyRuntimeError;
use gws_adapter_mcp::{McpServer, ManageEmailTool, ManageCalendarTool, ManageDriveTool, ManageDocsTool, ManageSheetsTool, ManageSlidesTool, ManageFormsTool, ManageTasksTool, ManageMeetTool, ManagePhotosTool, ManageNotebookLmTool, ManageAccountsTool};
use gws_adapter_google::{GoogleClient, TokenStore};
use std::sync::Arc;
use tokio::runtime::Runtime;

#[pyclass]
pub struct ServerWrapper {
    server: McpServer,
    rt: Runtime,
}

#[pymethods]
impl ServerWrapper {
    #[new]
    pub fn new(client_id: String, client_secret: String) -> PyResult<Self> {
        let token_store = Arc::new(
            TokenStore::new(client_id, client_secret)
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))?
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
        server.register_tool(Arc::new(ManageNotebookLmTool::new(client.clone())));

        let rt = Runtime::new()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        Ok(Self { server, rt })
    }

    pub fn list_tools(&self) -> PyResult<String> {
        serde_json::to_string(&self.server.list_tools())
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    pub fn call_tool(&self, name: String, args_json: String) -> PyResult<String> {
        let args: serde_json::Value = serde_json::from_str(&args_json)
             .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        let res = self.rt.block_on(async {
            self.server.call_tool(&name, args).await
        }).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        serde_json::to_string(&res)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }
}

#[pymodule]
fn gws_python(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<ServerWrapper>()?;
    Ok(())
}
