use gws_ports::McpTool;
use std::collections::HashMap;
use std::sync::Arc;

pub struct McpServer {
    tools: HashMap<String, Arc<dyn McpTool>>,
}

impl McpServer {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn register_tool(&mut self, tool: Arc<dyn McpTool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub fn list_tools(&self) -> Vec<serde_json::Value> {
        self.tools.values().map(|t| {
            serde_json::json!({
                "name": t.name(),
                "description": t.description(),
                "inputSchema": t.input_schema()
            })
        }).collect()
    }

    pub async fn call_tool(&self, name: &str, args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        if let Some(tool) = self.tools.get(name) {
            tool.call(args).await
        } else {
            Err(anyhow::anyhow!("Tool not found: {}", name))
        }
    }
}
