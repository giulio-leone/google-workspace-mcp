use async_trait::async_trait;

/// Represents a Tool that can be exported to the MCP system
#[async_trait]
pub trait McpTool: Send + Sync {
    /// The name of the tool as exposed to LLMs
    fn name(&self) -> &'static str;

    /// The description of the tool
    fn description(&self) -> &'static str;

    /// The JSON schema for the input parameters
    fn input_schema(&self) -> serde_json::Value;

    /// Execute the tool with a given JSON payload
    async fn call(&self, arguments: serde_json::Value) -> anyhow::Result<serde_json::Value>;
}
