use anyhow::{Context, Result};
use gws_adapter_google::{GoogleClient, TokenStore};
use gws_adapter_mcp::*;
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::sync::Arc;

/// MCP JSON-RPC 2.0 stdio server for Google Workspace.
///
/// Reads GOOGLE_CLIENT_ID / GOOGLE_CLIENT_SECRET from env.
/// Manages OAuth tokens per-account in ~/.config/google-workspace-mcp/accounts/.
/// Implements: initialize, tools/list, tools/call.
#[tokio::main]
async fn main() -> Result<()> {
    let client_id = std::env::var("GOOGLE_CLIENT_ID")
        .context("GOOGLE_CLIENT_ID env var required")?;
    let client_secret = std::env::var("GOOGLE_CLIENT_SECRET")
        .context("GOOGLE_CLIENT_SECRET env var required")?;

    // Create the shared token store (loads persisted tokens from disk)
    let token_store = Arc::new(
        TokenStore::new(client_id, client_secret)
            .context("Failed to initialize token store")?
    );

    // Create the multi-account Google API client
    let client = Arc::new(GoogleClient::new(token_store.clone()));

    let mut server = McpServer::new();

    // Register all 12 tools (11 Workspace + 1 account management)
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

    let stdin = io::stdin();
    let stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line.context("Failed to read stdin")?;
        if line.trim().is_empty() { continue; }

        let request: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let err_resp = json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": { "code": -32700, "message": format!("Parse error: {}", e) }
                });
                writeln!(stdout.lock(), "{}", err_resp)?;
                continue;
            }
        };

        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = request.get("params").cloned().unwrap_or(json!({}));

        let result = match method {
            "initialize" => {
                Ok(json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": { "tools": {} },
                    "serverInfo": {
                        "name": "google-workspace-mcp",
                        "version": "3.0.0"
                    }
                }))
            }
            "notifications/initialized" => continue,
            "tools/list" => {
                let tools = server.list_tools();
                Ok(json!({ "tools": tools }))
            }
            "tools/call" => {
                let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
                match server.call_tool(name, arguments).await {
                    Ok(val) => Ok(json!({
                        "content": [{ "type": "text", "text": val.to_string() }]
                    })),
                    Err(e) => Ok(json!({
                        "content": [{ "type": "text", "text": format!("Error: {}", e) }],
                        "isError": true
                    })),
                }
            }
            _ => Err(anyhow::anyhow!("Method not found: {}", method)),
        };

        let response = match result {
            Ok(res) => json!({ "jsonrpc": "2.0", "id": id, "result": res }),
            Err(e) => json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": e.to_string() } }),
        };

        writeln!(stdout.lock(), "{}", response)?;
        stdout.lock().flush()?;
    }

    Ok(())
}
