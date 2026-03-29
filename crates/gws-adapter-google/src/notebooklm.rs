//! NotebookLM adapter — reverse-engineered batchexecute RPC client.
//!
//! NotebookLM has no public REST API. This module replicates the browser's
//! internal RPC protocol using stored browser cookies.
//!
//! **Auth flow**: user must first authenticate via browser (Playwright or
//! manual login) and the cookie state is persisted to
//! `~/.notebooklm/storage_state.json`.
//!
//! To remove NotebookLM support entirely:
//!   1. Delete this file
//!   2. Remove `pub mod notebooklm;` from lib.rs
//!   3. Remove tool registration in main.rs / FFI bindings

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use gws_ports::*;
use reqwest::Client;
use serde_json::json;
use std::path::PathBuf;

const BATCHEXECUTE_URL: &str = "https://notebooklm.google.com/_/LabsTailwindUi/data/batchexecute";
const CHAT_URL: &str = "https://notebooklm.google.com/_/LabsTailwindUi/data/google.internal.labs.tailwind.orchestration.v1.LabsTailwindOrchestrationService/GenerateFreeFormStreamed";
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

// RPC method IDs (reverse-engineered from the web app)
mod rpc {
    pub const LIST_NOTEBOOKS: &str = "wXbhsf";
    pub const CREATE_NOTEBOOK: &str = "CCqFvf";
    pub const DELETE_NOTEBOOK: &str = "WWINqb";
    pub const ADD_SOURCE: &str = "izAoDd";
    pub const SUMMARIZE: &str = "VfAZjd";
}

/// Session tokens extracted from the NotebookLM homepage.
struct SessionTokens {
    csrf_token: String,
    session_id: String,
    cookie_header: String,
}

/// Standalone NotebookLM client, isolated from the OAuth-based GoogleClient.
pub struct NotebookLmClient {
    http: Client,
}

impl NotebookLmClient {
    pub fn new() -> Self {
        Self { http: Client::new() }
    }

    // ========================================================================
    // COOKIE MANAGEMENT
    // ========================================================================

    fn storage_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".notebooklm")
            .join("storage_state.json")
    }

    fn load_cookies() -> Result<String> {
        let path = Self::storage_path();
        let data = std::fs::read_to_string(&path)
            .with_context(|| format!("No NotebookLM auth state at {}. Run authenticate first.", path.display()))?;

        let state: serde_json::Value = serde_json::from_str(&data)?;
        let cookies = state["cookies"].as_array()
            .context("No cookies array in storage state")?;

        // Deduplicate, preferring .google.com domain
        let mut map = std::collections::HashMap::new();
        for c in cookies {
            let domain = c["domain"].as_str().unwrap_or("");
            let name = c["name"].as_str().unwrap_or("");
            let value = c["value"].as_str().unwrap_or("");
            if domain.contains("google.com") || domain.contains(".google.") {
                if !map.contains_key(name) || domain == ".google.com" {
                    map.insert(name.to_string(), value.to_string());
                }
            }
        }

        if !map.contains_key("SID") {
            bail!("SID cookie not found. NotebookLM auth may have expired.");
        }

        Ok(map.iter().map(|(k, v)| format!("{}={}", k, v)).collect::<Vec<_>>().join("; "))
    }

    async fn fetch_session_tokens(&self) -> Result<SessionTokens> {
        let cookie_header = Self::load_cookies()?;

        let html = self.http.get("https://notebooklm.google.com/")
            .header("Cookie", &cookie_header)
            .header("User-Agent", USER_AGENT)
            .send().await?
            .error_for_status().context("Failed to load NotebookLM homepage")?
            .text().await?;

        let csrf_token = extract_js_key(&html, "SNlM0e")
            .context("CSRF token (SNlM0e) not found. Auth may have expired.")?;
        let session_id = extract_js_key(&html, "FdrFJe")
            .context("Session ID (FdrFJe) not found. Auth may have expired.")?;

        Ok(SessionTokens { csrf_token, session_id, cookie_header })
    }

    // ========================================================================
    // RPC ENGINE
    // ========================================================================

    async fn rpc_call(&self, method: &str, params: &serde_json::Value, source_path: &str) -> Result<serde_json::Value> {
        let tokens = self.fetch_session_tokens().await?;

        let params_json = serde_json::to_string(params)?;
        let inner = json!([[method, params_json, serde_json::Value::Null, "generic"]]);
        let f_req = serde_json::to_string(&inner)?;

        let mut body = format!("f.req={}", urlencoding::encode(&f_req));
        if !tokens.csrf_token.is_empty() {
            body.push_str(&format!("&at={}", urlencoding::encode(&tokens.csrf_token)));
        }
        body.push('&');

        let url = format!(
            "{}?rpcids={}&source-path={}&f.sid={}&rt=c",
            BATCHEXECUTE_URL,
            urlencoding::encode(method),
            urlencoding::encode(source_path),
            urlencoding::encode(&tokens.session_id),
        );

        let resp = self.http.post(&url)
            .header("Content-Type", "application/x-www-form-urlencoded;charset=UTF-8")
            .header("Cookie", &tokens.cookie_header)
            .header("User-Agent", USER_AGENT)
            .body(body)
            .send().await?
            .error_for_status()
            .context("NotebookLM RPC failed")?
            .text().await?;

        let chunks = parse_chunked_response(&resp);
        extract_rpc_result(&chunks, method)
    }

    // ========================================================================
    // CHAT (streaming endpoint, different protocol)
    // ========================================================================

    async fn chat_internal(&self, notebook_id: &str, question: &str) -> Result<String> {
        let tokens = self.fetch_session_tokens().await?;

        let params = json!([
            [], question, serde_json::Value::Null,
            [2, serde_json::Value::Null, [1], [1]],
            serde_json::Value::Null, serde_json::Value::Null, serde_json::Value::Null,
            notebook_id, 1
        ]);
        let f_req = json!([serde_json::Value::Null, serde_json::to_string(&params)?]);
        let f_req_str = serde_json::to_string(&f_req)?;

        let mut body = format!("f.req={}", urlencoding::encode(&f_req_str));
        if !tokens.csrf_token.is_empty() {
            body.push_str(&format!("&at={}", urlencoding::encode(&tokens.csrf_token)));
        }
        body.push('&');

        let reqid: u32 = rand::random::<u32>() % 1_000_000;
        let url = format!(
            "{}?hl=en&_reqid={}&rt=c&f.sid={}",
            CHAT_URL, reqid, urlencoding::encode(&tokens.session_id)
        );

        let resp = self.http.post(&url)
            .header("Content-Type", "application/x-www-form-urlencoded;charset=UTF-8")
            .header("Cookie", &tokens.cookie_header)
            .header("User-Agent", USER_AGENT)
            .body(body)
            .send().await?
            .error_for_status()
            .context("NotebookLM chat failed")?
            .text().await?;

        let chunks = parse_chunked_response(&resp);

        // Find the longest answer text from streaming chunks
        let mut best = String::new();
        for chunk in &chunks {
            if let Some(items) = chunk.as_array() {
                for item in items {
                    if let Some(arr) = item.as_array() {
                        if arr.len() >= 3 && arr[0].as_str() == Some("wrb.fr") {
                            if let Some(inner_json) = arr[2].as_str() {
                                if let Ok(inner) = serde_json::from_str::<serde_json::Value>(inner_json) {
                                    if let Some(first) = inner.get(0).and_then(|v| v.get(0)).and_then(|v| v.as_str()) {
                                        if first.len() > best.len() {
                                            best = first.to_string();
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if best.is_empty() {
            Ok("No answer received.".into())
        } else {
            Ok(best)
        }
    }
}

// ============================================================================
// PORT IMPLEMENTATION
// ============================================================================

#[async_trait]
impl NotebookLmPort for NotebookLmClient {
    async fn list_notebooks(&self, _email: &str) -> Result<Vec<NotebookLmEntry>> {
        let result = self.rpc_call(rpc::LIST_NOTEBOOKS, &json!([null, 1, null, [2]]), "/").await?;
        let mut notebooks = Vec::new();
        if let Some(outer) = result.as_array() {
            let items = if outer.first().and_then(|v| v.as_array()).is_some() {
                outer.first().unwrap().as_array().unwrap()
            } else {
                outer
            };
            for nb in items {
                if let Some(arr) = nb.as_array() {
                    notebooks.push(NotebookLmEntry {
                        id: arr.get(0).and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        title: arr.get(1).and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        created_at: arr.get(2).and_then(|v| v.as_str()).map(String::from),
                    });
                }
            }
        }
        Ok(notebooks)
    }

    async fn create_notebook(&self, _email: &str, title: &str) -> Result<NotebookLmEntry> {
        let result = self.rpc_call(rpc::CREATE_NOTEBOOK, &json!([title, null, null, [2], [1]]), "/").await?;
        let arr = result.as_array().context("Unexpected create response")?;
        Ok(NotebookLmEntry {
            id: arr.get(0).and_then(|v| v.as_str()).unwrap_or("").to_string(),
            title: arr.get(1).and_then(|v| v.as_str()).unwrap_or(title).to_string(),
            created_at: None,
        })
    }

    async fn delete_notebook(&self, _email: &str, notebook_id: &str) -> Result<()> {
        self.rpc_call(rpc::DELETE_NOTEBOOK, &json!([notebook_id]), "/").await?;
        Ok(())
    }

    async fn get_summary(&self, _email: &str, notebook_id: &str) -> Result<String> {
        let source_path = format!("/notebook/{}", notebook_id);
        let result = self.rpc_call(rpc::SUMMARIZE, &json!([notebook_id, [2]]), &source_path).await?;
        // Navigate: result[0][0][0]
        if let Some(text) = result.get(0).and_then(|v| v.get(0)).and_then(|v| v.get(0)).and_then(|v| v.as_str()) {
            Ok(text.to_string())
        } else {
            Ok("No summary available.".into())
        }
    }

    async fn add_source_url(&self, _email: &str, notebook_id: &str, url: &str) -> Result<serde_json::Value> {
        let source_path = format!("/notebook/{}", notebook_id);
        let result = self.rpc_call(rpc::ADD_SOURCE, &json!([notebook_id, 1, null, [url]]), &source_path).await?;
        Ok(result)
    }

    async fn chat(&self, _email: &str, notebook_id: &str, question: &str) -> Result<String> {
        self.chat_internal(notebook_id, question).await
    }
}

// ============================================================================
// RESPONSE PARSING HELPERS
// ============================================================================

/// Extract a JavaScript config key like `"SNlM0e":"value"` from HTML.
fn extract_js_key(html: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{}\":\"", key);
    let start = html.find(&pattern)? + pattern.len();
    let end = start + html[start..].find('"')?;
    Some(html[start..end].to_string())
}

/// Parse Google's chunked batchexecute response format.
fn parse_chunked_response(text: &str) -> Vec<serde_json::Value> {
    let text = if text.starts_with(")]}'") {
        &text[text.find('\n').unwrap_or(0) + 1..]
    } else {
        text
    };

    let mut chunks = Vec::new();
    let lines: Vec<&str> = text.trim().lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();
        if line.is_empty() { i += 1; continue; }
        if line.parse::<usize>().is_ok() {
            i += 1;
            if i < lines.len() {
                if let Ok(v) = serde_json::from_str(lines[i]) {
                    chunks.push(v);
                }
            }
            i += 1;
        } else {
            if let Ok(v) = serde_json::from_str(line) {
                chunks.push(v);
            }
            i += 1;
        }
    }
    chunks
}

/// Extract the RPC result for a given method ID from parsed chunks.
fn extract_rpc_result(chunks: &[serde_json::Value], rpc_id: &str) -> Result<serde_json::Value> {
    for chunk in chunks {
        let items = match chunk.as_array() {
            Some(arr) if arr.first().and_then(|v| v.as_array()).is_some() => arr.clone(),
            Some(_) => vec![chunk.clone()],
            None => continue,
        };
        for item in &items {
            if let Some(arr) = item.as_array() {
                if arr.len() >= 3 {
                    if arr[0].as_str() == Some("er") && arr[1].as_str() == Some(rpc_id) {
                        bail!("NotebookLM RPC error: {:?}", arr.get(2));
                    }
                    if arr[0].as_str() == Some("wrb.fr") && arr[1].as_str() == Some(rpc_id) {
                        if let Some(data) = arr[2].as_str() {
                            return Ok(serde_json::from_str::<serde_json::Value>(data)
                                .unwrap_or_else(|_| serde_json::Value::String(data.to_string())));
                        }
                        return Ok(arr[2].clone());
                    }
                }
            }
        }
    }
    Ok(serde_json::Value::Null)
}
