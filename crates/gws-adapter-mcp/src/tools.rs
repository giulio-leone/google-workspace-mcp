use async_trait::async_trait;
use gws_ports::*;
use gws_adapter_google::TokenStore;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use anyhow::Context;

// =============================================================================
// MANAGE EMAIL
// =============================================================================

#[derive(Serialize, Deserialize, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum EmailOperation {
    /// Read a single email message by ID. Requires: message_id.
    Read,
    /// Search messages using Gmail query syntax (e.g. "from:user@example.com is:unread"). Optional: query, max_results.
    Search,
    /// Send a new email. Requires: to, subject, body. Optional: cc, bcc.
    Send,
    /// Reply to a message. Requires: message_id, body.
    Reply,
    /// Forward a message. Requires: message_id, to.
    Forward,
    /// Move a message to trash. Requires: message_id.
    Trash,
    /// Restore a message from trash. Requires: message_id.
    Untrash,
    /// Add or remove labels from a message. Requires: message_id. Optional: add_labels, remove_labels.
    Modify,
    /// List all available Gmail labels.
    Labels,
    /// Get a summary of unread inbox messages (up to 20).
    Triage,
    /// List email threads. Optional: query, max_results.
    Threads,
    /// Get a full thread by ID. Requires: thread_id.
    GetThread,
    /// Download an attachment. Requires: message_id, attachment_id.
    GetAttachment,
    /// Set up Gmail push notifications via Pub/Sub. Requires: topic_name.
    Watch,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ManageEmailArgs {
    /// The operation to perform.
    pub operation: EmailOperation,
    /// Authenticated user email (or 'me').
    pub email: String,
    /// Message ID — required for: read, reply, forward, trash, untrash, modify, get_attachment.
    pub message_id: Option<String>,
    /// Gmail search query — for: search, threads.
    pub query: Option<String>,
    /// Recipient — for: send, forward.
    pub to: Option<String>,
    /// Subject — for: send.
    pub subject: Option<String>,
    /// Body text — for: send, reply.
    pub body: Option<String>,
    /// CC — for: send.
    pub cc: Option<String>,
    /// BCC — for: send.
    pub bcc: Option<String>,
    /// Labels to add — for: modify.
    pub add_labels: Option<Vec<String>>,
    /// Labels to remove — for: modify.
    pub remove_labels: Option<Vec<String>>,
    /// Thread ID — for: get_thread.
    pub thread_id: Option<String>,
    /// Attachment ID — for: get_attachment.
    pub attachment_id: Option<String>,
    /// Pub/Sub topic — for: watch.
    pub topic_name: Option<String>,
    /// Max results — for: search, threads.
    pub max_results: Option<u32>,
}

pub struct ManageEmailTool { port: Arc<dyn GmailPort> }
impl ManageEmailTool { pub fn new(port: Arc<dyn GmailPort>) -> Self { Self { port } } }

#[async_trait]
impl McpTool for ManageEmailTool {
    fn name(&self) -> &'static str { "manage_email" }
    fn description(&self) -> &'static str {
        "Search, read, send, reply, forward, trash, untrash, label, triage, and manage Gmail messages and threads."
    }
    fn input_schema(&self) -> serde_json::Value { serde_json::to_value(schemars::schema_for!(ManageEmailArgs)).unwrap() }

    async fn call(&self, arguments: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let a: ManageEmailArgs = serde_json::from_value(arguments).context("Invalid args for manage_email")?;
        match a.operation {
            EmailOperation::Read => {
                let id = a.message_id.context("message_id required")?;
                Ok(serde_json::to_value(self.port.get_message(&a.email, &id).await?)?)
            }
            EmailOperation::Search => Ok(serde_json::to_value(self.port.list_messages(&a.email, a.query.as_deref(), a.max_results).await?)?),
            EmailOperation::Send => {
                let to = a.to.context("to required")?;
                let subj = a.subject.context("subject required")?;
                let body = a.body.context("body required")?;
                Ok(serde_json::to_value(self.port.send_message(&a.email, &to, &subj, &body, a.cc.as_deref(), a.bcc.as_deref()).await?)?)
            }
            EmailOperation::Reply => {
                let id = a.message_id.context("message_id required")?;
                let body = a.body.context("body required")?;
                Ok(serde_json::to_value(self.port.reply(&a.email, &id, &body).await?)?)
            }
            EmailOperation::Forward => {
                let id = a.message_id.context("message_id required")?;
                let to = a.to.context("to required")?;
                Ok(serde_json::to_value(self.port.forward(&a.email, &id, &to).await?)?)
            }
            EmailOperation::Trash => {
                let id = a.message_id.context("message_id required")?;
                Ok(serde_json::to_value(self.port.trash_message(&a.email, &id).await?)?)
            }
            EmailOperation::Untrash => {
                let id = a.message_id.context("message_id required")?;
                Ok(serde_json::to_value(self.port.untrash_message(&a.email, &id).await?)?)
            }
            EmailOperation::Modify => {
                let id = a.message_id.context("message_id required")?;
                let add = a.add_labels.as_deref().map(|v| v.iter().map(|s| s.as_str()).collect());
                let rm = a.remove_labels.as_deref().map(|v| v.iter().map(|s| s.as_str()).collect());
                Ok(serde_json::to_value(self.port.modify_message(&a.email, &id, add, rm).await?)?)
            }
            EmailOperation::Labels => Ok(serde_json::to_value(self.port.list_labels(&a.email).await?)?),
            EmailOperation::Triage => Ok(serde_json::to_value(self.port.triage(&a.email).await?)?),
            EmailOperation::Threads => Ok(serde_json::to_value(self.port.list_threads(&a.email, a.query.as_deref(), a.max_results).await?)?),
            EmailOperation::GetThread => {
                let tid = a.thread_id.context("thread_id required")?;
                Ok(serde_json::to_value(self.port.get_thread(&a.email, &tid).await?)?)
            }
            EmailOperation::GetAttachment => {
                let mid = a.message_id.context("message_id required")?;
                let aid = a.attachment_id.context("attachment_id required")?;
                Ok(serde_json::to_value(self.port.get_attachment(&a.email, &mid, &aid).await?)?)
            }
            EmailOperation::Watch => {
                let topic = a.topic_name.context("topic_name required")?;
                Ok(serde_json::to_value(self.port.watch(&a.email, &topic, None).await?)?)
            }
        }
    }
}

// =============================================================================
// MANAGE CALENDAR
// =============================================================================

#[derive(Serialize, Deserialize, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum CalendarOperation {
    /// Get a single event by ID. Requires: event_id.
    Get,
    /// List events in a calendar. Optional: time_min, time_max, max_results, query.
    List,
    /// Create a new event. Requires: summary, start, end. Optional: description, attendees, location.
    Create,
    /// Create event from natural language (e.g. "Lunch at noon tomorrow"). Requires: text.
    QuickAdd,
    /// Update an existing event. Requires: event_id. Optional: summary, start, end, description.
    Update,
    /// Delete an event. Requires: event_id.
    Delete,
    /// List all calendars for the user.
    Calendars,
    /// Check availability (free/busy). Requires: time_min, time_max.
    Freebusy,
    /// Set up push notifications for calendar changes. Requires: webhook_address.
    Watch,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ManageCalendarArgs {
    /// The operation to perform.
    pub operation: CalendarOperation,
    /// Authenticated user email (or 'me').
    pub email: String,
    /// Calendar ID (default: 'primary'). Use 'primary' for the user's main calendar.
    pub calendar_id: Option<String>,
    /// Event ID — required for: get, update, delete.
    pub event_id: Option<String>,
    /// Event title/summary — required for: create.
    pub summary: Option<String>,
    /// Event description/notes.
    pub description: Option<String>,
    /// Physical or virtual location.
    pub location: Option<String>,
    /// Start time in ISO 8601 format (e.g. '2024-01-15T09:00:00-05:00'). Required for: create.
    pub start: Option<String>,
    /// End time in ISO 8601 format. Required for: create.
    pub end: Option<String>,
    /// Comma-separated attendee emails (e.g. 'alice@example.com,bob@example.com').
    pub attendees: Option<String>,
    /// Natural language event description for quick_add (e.g. 'Meeting with Bob tomorrow at 3pm').
    pub text: Option<String>,
    /// Filter events starting after this ISO 8601 timestamp.
    pub time_min: Option<String>,
    /// Filter events ending before this ISO 8601 timestamp.
    pub time_max: Option<String>,
    /// Free-text search query to filter events.
    pub query: Option<String>,
    /// Maximum number of results to return (default: 10).
    pub max_results: Option<u32>,
    /// HTTPS webhook URL for push notifications — required for: watch.
    pub webhook_address: Option<String>,
}

pub struct ManageCalendarTool { port: Arc<dyn CalendarPort> }
impl ManageCalendarTool { pub fn new(port: Arc<dyn CalendarPort>) -> Self { Self { port } } }

#[async_trait]
impl McpTool for ManageCalendarTool {
    fn name(&self) -> &'static str { "manage_calendar" }
    fn description(&self) -> &'static str {
        "List events, view agenda, create, quickAdd, update, delete events, list calendars, check free/busy, and set up push webhooks."
    }
    fn input_schema(&self) -> serde_json::Value { serde_json::to_value(schemars::schema_for!(ManageCalendarArgs)).unwrap() }

    async fn call(&self, arguments: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let a: ManageCalendarArgs = serde_json::from_value(arguments).context("Invalid args")?;
        let cal = a.calendar_id.as_deref().unwrap_or("primary");
        match a.operation {
            CalendarOperation::Get => {
                let eid = a.event_id.context("event_id required")?;
                Ok(serde_json::to_value(self.port.get_event(&a.email, cal, &eid).await?)?)
            }
            CalendarOperation::List => Ok(serde_json::to_value(self.port.list_events(&a.email, cal, a.time_min.as_deref(), a.time_max.as_deref(), a.max_results, a.query.as_deref()).await?)?),
            CalendarOperation::Create => {
                let sum = a.summary.context("summary required")?;
                let start = a.start.context("start required")?;
                let end = a.end.context("end required")?;
                let attendees: Option<Vec<&str>> = a.attendees.as_deref().map(|s| s.split(',').map(|e| e.trim()).collect());
                Ok(serde_json::to_value(self.port.create_event(&a.email, cal, &sum, &start, &end, a.description.as_deref(), attendees, a.location.as_deref()).await?)?)
            }
            CalendarOperation::QuickAdd => {
                let text = a.text.context("text required")?;
                Ok(serde_json::to_value(self.port.quick_add(&a.email, cal, &text).await?)?)
            }
            CalendarOperation::Update => {
                let eid = a.event_id.context("event_id required")?;
                Ok(serde_json::to_value(self.port.update_event(&a.email, cal, &eid, a.summary.as_deref(), a.start.as_deref(), a.end.as_deref(), a.description.as_deref()).await?)?)
            }
            CalendarOperation::Delete => {
                let eid = a.event_id.context("event_id required")?;
                self.port.delete_event(&a.email, cal, &eid).await?;
                Ok(serde_json::json!({"status": "deleted"}))
            }
            CalendarOperation::Calendars => Ok(serde_json::to_value(self.port.list_calendars(&a.email).await?)?),
            CalendarOperation::Freebusy => {
                let tmin = a.time_min.context("time_min required")?;
                let tmax = a.time_max.context("time_max required")?;
                Ok(serde_json::to_value(self.port.freebusy(&a.email, &tmin, &tmax, vec![cal]).await?)?)
            }
            CalendarOperation::Watch => {
                let addr = a.webhook_address.context("webhook_address required")?;
                let req = gws_domain::PushWatchRequest { id: uuid::Uuid::new_v4().to_string(), r#type: "web_hook".into(), address: addr, token: None, expiration: None };
                Ok(serde_json::to_value(self.port.watch(&a.email, cal, req).await?)?)
            }
        }
    }
}

// =============================================================================
// MANAGE DRIVE
// =============================================================================

#[derive(Serialize, Deserialize, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum DriveOperation {
    /// Get file metadata by ID. Requires: file_id.
    Get,
    /// Search files using Drive query syntax (e.g. "name contains 'report'"). Optional: query, max_results.
    Search,
    /// Upload a new file. Requires: name, mime_type, content_base64. Optional: parent_folder_id.
    Upload,
    /// Download file content as text. Requires: file_id.
    Download,
    /// Copy a file. Requires: file_id. Optional: new_name.
    Copy,
    /// Permanently delete a file. Requires: file_id.
    Delete,
    /// Export a Google Workspace file (Docs/Sheets/Slides) to another format. Requires: file_id, mime_type (e.g. 'application/pdf').
    Export,
    /// List who has access to a file. Requires: file_id.
    ListPermissions,
    /// Share a file with another user. Requires: file_id, share_email. Optional: role (default: 'reader').
    Share,
    /// Remove a user's access. Requires: file_id, permission_id.
    Unshare,
    /// Set up push notifications for file changes. Requires: file_id, webhook_address.
    Watch,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ManageDriveArgs {
    /// The operation to perform.
    pub operation: DriveOperation,
    /// Authenticated user email (or 'me').
    pub email: String,
    /// File ID — required for most operations.
    pub file_id: Option<String>,
    /// Drive search query (e.g. "mimeType='application/pdf'" or "name contains 'invoice'").
    pub query: Option<String>,
    /// File name — required for: upload.
    pub name: Option<String>,
    /// MIME type — required for: upload, export.
    pub mime_type: Option<String>,
    /// Base64-encoded file content — required for: upload.
    pub content_base64: Option<String>,
    /// Parent folder ID to upload into.
    pub parent_folder_id: Option<String>,
    /// New name for copied file.
    pub new_name: Option<String>,
    /// Email to share with — required for: share.
    pub share_email: Option<String>,
    /// Permission role: 'reader', 'writer', 'commenter', or 'owner'.
    pub role: Option<String>,
    /// Permission ID to revoke — required for: unshare.
    pub permission_id: Option<String>,
    /// Maximum number of results (default: 10).
    pub max_results: Option<u32>,
    /// HTTPS webhook URL — required for: watch.
    pub webhook_address: Option<String>,
}

pub struct ManageDriveTool { port: Arc<dyn DrivePort> }
impl ManageDriveTool { pub fn new(port: Arc<dyn DrivePort>) -> Self { Self { port } } }

#[async_trait]
impl McpTool for ManageDriveTool {
    fn name(&self) -> &'static str { "manage_drive" }
    fn description(&self) -> &'static str {
        "Search, upload, download, copy, delete, export, and share Google Drive files. Supports permissions management and push webhooks."
    }
    fn input_schema(&self) -> serde_json::Value { serde_json::to_value(schemars::schema_for!(ManageDriveArgs)).unwrap() }

    async fn call(&self, arguments: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let a: ManageDriveArgs = serde_json::from_value(arguments).context("Invalid args")?;
        match a.operation {
            DriveOperation::Get => { let fid = a.file_id.context("file_id required")?; Ok(serde_json::to_value(self.port.get_file(&a.email, &fid).await?)?) }
            DriveOperation::Search => Ok(serde_json::to_value(self.port.list_files(&a.email, a.query.as_deref(), a.max_results).await?)?),
            DriveOperation::Upload => {
                let name = a.name.context("name required")?;
                let mime = a.mime_type.context("mime_type required")?;
                let b64 = a.content_base64.context("content_base64 required")?;
                Ok(serde_json::to_value(self.port.upload_file(&a.email, &name, &mime, &b64, a.parent_folder_id.as_deref()).await?)?)
            }
            DriveOperation::Download => { let fid = a.file_id.context("file_id required")?; Ok(serde_json::json!({"content": self.port.download_file(&a.email, &fid).await?})) }
            DriveOperation::Copy => { let fid = a.file_id.context("file_id required")?; Ok(serde_json::to_value(self.port.copy_file(&a.email, &fid, a.new_name.as_deref()).await?)?) }
            DriveOperation::Delete => { let fid = a.file_id.context("file_id required")?; self.port.delete_file(&a.email, &fid).await?; Ok(serde_json::json!({"status": "deleted"})) }
            DriveOperation::Export => {
                let fid = a.file_id.context("file_id required")?;
                let mt = a.mime_type.context("mime_type required")?;
                Ok(serde_json::json!({"content": self.port.export_file(&a.email, &fid, &mt).await?}))
            }
            DriveOperation::ListPermissions => { let fid = a.file_id.context("file_id required")?; Ok(serde_json::to_value(self.port.list_permissions(&a.email, &fid).await?)?) }
            DriveOperation::Share => {
                let fid = a.file_id.context("file_id required")?;
                let se = a.share_email.context("share_email required")?;
                let role = a.role.as_deref().unwrap_or("reader");
                Ok(serde_json::to_value(self.port.share(&a.email, &fid, &se, role).await?)?)
            }
            DriveOperation::Unshare => {
                let fid = a.file_id.context("file_id required")?;
                let pid = a.permission_id.context("permission_id required")?;
                self.port.unshare(&a.email, &fid, &pid).await?; Ok(serde_json::json!({"status": "unshared"}))
            }
            DriveOperation::Watch => {
                let fid = a.file_id.context("file_id required")?;
                let addr = a.webhook_address.context("webhook_address required")?;
                let req = gws_domain::PushWatchRequest { id: uuid::Uuid::new_v4().to_string(), r#type: "web_hook".into(), address: addr, token: None, expiration: None };
                Ok(serde_json::to_value(self.port.watch(&a.email, &fid, req).await?)?)
            }
        }
    }
}

// =============================================================================
// MANAGE DOCS
// =============================================================================

#[derive(Serialize, Deserialize, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum DocsOperation {
    /// Get document content by ID. Requires: document_id.
    Get,
    /// Create a new empty document. Optional: title.
    Create,
    /// Append text to an existing document. Requires: document_id, text.
    Write,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ManageDocsArgs {
    /// The operation to perform.
    pub operation: DocsOperation,
    /// Authenticated user email (or 'me').
    pub email: String,
    /// Document ID — required for: get, write.
    pub document_id: Option<String>,
    /// Document title — for: create.
    pub title: Option<String>,
    /// Text content to append — required for: write.
    pub text: Option<String>,
}

pub struct ManageDocsTool { port: Arc<dyn DocsPort> }
impl ManageDocsTool { pub fn new(port: Arc<dyn DocsPort>) -> Self { Self { port } } }

#[async_trait]
impl McpTool for ManageDocsTool {
    fn name(&self) -> &'static str { "manage_docs" }
    fn description(&self) -> &'static str { "Read and write Google Docs documents." }
    fn input_schema(&self) -> serde_json::Value { serde_json::to_value(schemars::schema_for!(ManageDocsArgs)).unwrap() }
    async fn call(&self, arguments: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let a: ManageDocsArgs = serde_json::from_value(arguments)?;
        match a.operation {
            DocsOperation::Get => { let d = a.document_id.context("document_id required")?; Ok(serde_json::to_value(self.port.get_doc(&a.email, &d).await?)?) }
            DocsOperation::Create => { let t = a.title.unwrap_or("Untitled".into()); Ok(serde_json::to_value(self.port.create_doc(&a.email, &t).await?)?) }
            DocsOperation::Write => { let d = a.document_id.context("document_id required")?; let t = a.text.context("text required")?; self.port.append_text(&a.email, &d, &t).await?; Ok(serde_json::json!({"status": "written"})) }
        }
    }
}

// =============================================================================
// MANAGE SHEETS
// =============================================================================

#[derive(Serialize, Deserialize, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum SheetsOperation {
    /// Get spreadsheet metadata. Requires: spreadsheet_id.
    Get,
    /// Create a new spreadsheet. Optional: title.
    Create,
    /// Read cell values from a range. Requires: spreadsheet_id, range (e.g. 'Sheet1!A1:C10').
    Read,
    /// Append rows to a range. Requires: spreadsheet_id, range, values_json.
    Append,
    /// Overwrite cell values in a range. Requires: spreadsheet_id, range, values_json.
    UpdateValues,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ManageSheetsArgs {
    /// The operation to perform.
    pub operation: SheetsOperation,
    /// Authenticated user email (or 'me').
    pub email: String,
    /// Spreadsheet ID — required for: get, read, append, update_values.
    pub spreadsheet_id: Option<String>,
    /// Spreadsheet title — for: create.
    pub title: Option<String>,
    /// Cell range in A1 notation (e.g. 'Sheet1!A1:C10') — required for: read, append, update_values.
    pub range: Option<String>,
    /// JSON array of row arrays (e.g. '[["Name","Age"],["Alice",30]]') — required for: append, update_values.
    pub values_json: Option<String>,
}

pub struct ManageSheetsTool { port: Arc<dyn SheetsPort> }
impl ManageSheetsTool { pub fn new(port: Arc<dyn SheetsPort>) -> Self { Self { port } } }

#[async_trait]
impl McpTool for ManageSheetsTool {
    fn name(&self) -> &'static str { "manage_sheets" }
    fn description(&self) -> &'static str { "Read, write, create, and append to Google Sheets spreadsheets." }
    fn input_schema(&self) -> serde_json::Value { serde_json::to_value(schemars::schema_for!(ManageSheetsArgs)).unwrap() }
    async fn call(&self, arguments: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let a: ManageSheetsArgs = serde_json::from_value(arguments)?;
        match a.operation {
            SheetsOperation::Get => { let s = a.spreadsheet_id.context("spreadsheet_id required")?; Ok(serde_json::to_value(self.port.get_sheet(&a.email, &s).await?)?) }
            SheetsOperation::Create => { let t = a.title.unwrap_or("Untitled".into()); Ok(serde_json::to_value(self.port.create_sheet(&a.email, &t).await?)?) }
            SheetsOperation::Read => {
                let s = a.spreadsheet_id.context("spreadsheet_id required")?;
                let r = a.range.context("range required")?;
                Ok(serde_json::to_value(self.port.read_range(&a.email, &s, &r).await?)?)
            }
            SheetsOperation::Append => {
                let s = a.spreadsheet_id.context("spreadsheet_id required")?;
                let r = a.range.context("range required")?;
                let v = a.values_json.context("values_json required")?;
                self.port.append_cells(&a.email, &s, &r, &v).await?; Ok(serde_json::json!({"status": "appended"}))
            }
            SheetsOperation::UpdateValues => {
                let s = a.spreadsheet_id.context("spreadsheet_id required")?;
                let r = a.range.context("range required")?;
                let v = a.values_json.context("values_json required")?;
                self.port.update_values(&a.email, &s, &r, &v).await?; Ok(serde_json::json!({"status": "updated"}))
            }
        }
    }
}

// =============================================================================
// MANAGE SLIDES
// =============================================================================

#[derive(Serialize, Deserialize, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum SlidesOperation {
    /// Get presentation metadata and slides. Requires: presentation_id.
    Get,
    /// Create a new presentation. Optional: title.
    Create,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ManageSlidesArgs {
    /// The operation to perform.
    pub operation: SlidesOperation,
    /// Authenticated user email (or 'me').
    pub email: String,
    /// Presentation ID — required for: get.
    pub presentation_id: Option<String>,
    /// Presentation title — for: create.
    pub title: Option<String>,
}

pub struct ManageSlidesTool { port: Arc<dyn SlidesPort> }
impl ManageSlidesTool { pub fn new(port: Arc<dyn SlidesPort>) -> Self { Self { port } } }

#[async_trait]
impl McpTool for ManageSlidesTool {
    fn name(&self) -> &'static str { "manage_slides" }
    fn description(&self) -> &'static str { "Manage Google Slides presentations." }
    fn input_schema(&self) -> serde_json::Value { serde_json::to_value(schemars::schema_for!(ManageSlidesArgs)).unwrap() }
    async fn call(&self, arguments: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let a: ManageSlidesArgs = serde_json::from_value(arguments)?;
        match a.operation {
            SlidesOperation::Get => { let p = a.presentation_id.context("presentation_id required")?; Ok(serde_json::to_value(self.port.get_presentation(&a.email, &p).await?)?) }
            SlidesOperation::Create => { let t = a.title.unwrap_or("Untitled".into()); Ok(serde_json::to_value(self.port.create_presentation(&a.email, &t).await?)?) }
        }
    }
}

// =============================================================================
// MANAGE FORMS
// =============================================================================

#[derive(Serialize, Deserialize, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum FormsOperation {
    /// Get form structure (questions, sections). Requires: form_id.
    Get,
    /// List all submitted responses. Requires: form_id.
    ListResponses,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ManageFormsArgs {
    /// The operation to perform.
    pub operation: FormsOperation,
    /// Authenticated user email (or 'me').
    pub email: String,
    /// Form ID — required for all operations.
    pub form_id: Option<String>,
}

pub struct ManageFormsTool { port: Arc<dyn FormsPort> }
impl ManageFormsTool { pub fn new(port: Arc<dyn FormsPort>) -> Self { Self { port } } }

#[async_trait]
impl McpTool for ManageFormsTool {
    fn name(&self) -> &'static str { "manage_forms" }
    fn description(&self) -> &'static str { "Manage Google Forms." }
    fn input_schema(&self) -> serde_json::Value { serde_json::to_value(schemars::schema_for!(ManageFormsArgs)).unwrap() }
    async fn call(&self, arguments: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let a: ManageFormsArgs = serde_json::from_value(arguments)?;
        match a.operation {
            FormsOperation::Get => { let f = a.form_id.context("form_id required")?; Ok(serde_json::to_value(self.port.get_form(&a.email, &f).await?)?) }
            FormsOperation::ListResponses => { let f = a.form_id.context("form_id required")?; Ok(serde_json::to_value(self.port.list_responses(&a.email, &f).await?)?) }
        }
    }
}

// =============================================================================
// MANAGE TASKS
// =============================================================================

#[derive(Serialize, Deserialize, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum TasksOperation {
    /// List all task lists.
    ListTaskLists,
    /// Create a new task list. Requires: title.
    CreateTaskList,
    /// Delete a task list. Requires: task_list_id.
    DeleteTaskList,
    /// List tasks in a task list. Requires: task_list_id.
    List,
    /// Get a single task. Requires: task_list_id, task_id.
    Get,
    /// Create a task. Requires: task_list_id, title. Optional: notes, due.
    Create,
    /// Update a task. Requires: task_list_id, task_id. Optional: title, notes, due.
    Update,
    /// Mark a task as completed. Requires: task_list_id, task_id.
    Complete,
    /// Delete a task. Requires: task_list_id, task_id.
    Delete,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ManageTasksArgs {
    /// The operation to perform.
    pub operation: TasksOperation,
    /// Authenticated user email (or 'me').
    pub email: String,
    /// Task list ID — required for most operations.
    pub task_list_id: Option<String>,
    /// Task ID — required for: get, update, complete, delete.
    pub task_id: Option<String>,
    /// Task or list title.
    pub title: Option<String>,
    /// Task notes/description.
    pub notes: Option<String>,
    /// Due date in RFC 3339 format (e.g. '2024-01-15T00:00:00Z').
    pub due: Option<String>,
}

pub struct ManageTasksTool { port: Arc<dyn TasksPort> }
impl ManageTasksTool { pub fn new(port: Arc<dyn TasksPort>) -> Self { Self { port } } }

#[async_trait]
impl McpTool for ManageTasksTool {
    fn name(&self) -> &'static str { "manage_tasks" }
    fn description(&self) -> &'static str { "Manage task lists and tasks in Google Tasks." }
    fn input_schema(&self) -> serde_json::Value { serde_json::to_value(schemars::schema_for!(ManageTasksArgs)).unwrap() }
    async fn call(&self, arguments: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let a: ManageTasksArgs = serde_json::from_value(arguments)?;
        match a.operation {
            TasksOperation::ListTaskLists => Ok(serde_json::to_value(self.port.list_task_lists(&a.email).await?)?),
            TasksOperation::CreateTaskList => { let t = a.title.context("title required")?; Ok(serde_json::to_value(self.port.create_task_list(&a.email, &t).await?)?) }
            TasksOperation::DeleteTaskList => { let tl = a.task_list_id.context("task_list_id required")?; self.port.delete_task_list(&a.email, &tl).await?; Ok(serde_json::json!({"status":"deleted"})) }
            TasksOperation::List => { let tl = a.task_list_id.context("task_list_id required")?; Ok(serde_json::to_value(self.port.list_tasks(&a.email, &tl).await?)?) }
            TasksOperation::Get => { let tl = a.task_list_id.context("task_list_id required")?; let tid = a.task_id.context("task_id required")?; Ok(serde_json::to_value(self.port.get_task(&a.email, &tl, &tid).await?)?) }
            TasksOperation::Create => { let tl = a.task_list_id.context("task_list_id required")?; let t = a.title.context("title required")?; Ok(serde_json::to_value(self.port.create_task(&a.email, &tl, &t, a.notes.as_deref(), a.due.as_deref()).await?)?) }
            TasksOperation::Update => { let tl = a.task_list_id.context("task_list_id required")?; let tid = a.task_id.context("task_id required")?; Ok(serde_json::to_value(self.port.update_task(&a.email, &tl, &tid, a.title.as_deref(), a.notes.as_deref(), a.due.as_deref()).await?)?) }
            TasksOperation::Complete => { let tl = a.task_list_id.context("task_list_id required")?; let tid = a.task_id.context("task_id required")?; Ok(serde_json::to_value(self.port.complete_task(&a.email, &tl, &tid).await?)?) }
            TasksOperation::Delete => { let tl = a.task_list_id.context("task_list_id required")?; let tid = a.task_id.context("task_id required")?; self.port.delete_task(&a.email, &tl, &tid).await?; Ok(serde_json::json!({"status":"deleted"})) }
        }
    }
}

// =============================================================================
// MANAGE MEET
// =============================================================================

#[derive(Serialize, Deserialize, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum MeetOperation {
    /// List recent conference records. Optional: max_results.
    ListConferences,
    /// Get details of a specific conference. Requires: conference_id.
    GetConference,
    /// List participants in a conference. Requires: conference_id.
    ListParticipants,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ManageMeetArgs {
    /// The operation to perform.
    pub operation: MeetOperation,
    /// Authenticated user email (or 'me').
    pub email: String,
    /// Conference record name/ID (e.g. 'conferenceRecords/abc123') — required for: get_conference, list_participants.
    pub conference_id: Option<String>,
    /// Maximum number of results (default: 25).
    pub max_results: Option<u32>,
}

pub struct ManageMeetTool { port: Arc<dyn MeetPort> }
impl ManageMeetTool { pub fn new(port: Arc<dyn MeetPort>) -> Self { Self { port } } }

#[async_trait]
impl McpTool for ManageMeetTool {
    fn name(&self) -> &'static str { "manage_meet" }
    fn description(&self) -> &'static str { "Browse Google Meet conferences, participants, and recordings." }
    fn input_schema(&self) -> serde_json::Value { serde_json::to_value(schemars::schema_for!(ManageMeetArgs)).unwrap() }
    async fn call(&self, arguments: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let a: ManageMeetArgs = serde_json::from_value(arguments)?;
        match a.operation {
            MeetOperation::ListConferences => Ok(serde_json::to_value(self.port.list_conferences(&a.email, a.max_results).await?)?),
            MeetOperation::GetConference => { let c = a.conference_id.context("conference_id required")?; Ok(serde_json::to_value(self.port.get_conference(&a.email, &c).await?)?) }
            MeetOperation::ListParticipants => { let c = a.conference_id.context("conference_id required")?; Ok(serde_json::to_value(self.port.list_participants(&a.email, &c).await?)?) }
        }
    }
}

// =============================================================================
// MANAGE PHOTOS
// =============================================================================

#[derive(Serialize, Deserialize, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum PhotosOperation {
    /// List all photo albums.
    ListAlbums,
    /// List media items (photos/videos). Optional: album_id (filter by album), page_size.
    ListMedia,
    /// Get details of a single media item. Requires: media_item_id.
    GetMedia,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ManagePhotosArgs {
    /// The operation to perform.
    pub operation: PhotosOperation,
    /// Authenticated user email (or 'me').
    pub email: String,
    /// Album ID to filter media items by.
    pub album_id: Option<String>,
    /// Media item ID — required for: get_media.
    pub media_item_id: Option<String>,
    /// Number of items to return (default: 50, max: 100).
    pub page_size: Option<u32>,
}

pub struct ManagePhotosTool { port: Arc<dyn PhotosPort> }
impl ManagePhotosTool { pub fn new(port: Arc<dyn PhotosPort>) -> Self { Self { port } } }

#[async_trait]
impl McpTool for ManagePhotosTool {
    fn name(&self) -> &'static str { "manage_photos" }
    fn description(&self) -> &'static str { "Manage Google Photos: list albums, browse media items." }
    fn input_schema(&self) -> serde_json::Value { serde_json::to_value(schemars::schema_for!(ManagePhotosArgs)).unwrap() }
    async fn call(&self, arguments: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let a: ManagePhotosArgs = serde_json::from_value(arguments)?;
        match a.operation {
            PhotosOperation::ListAlbums => Ok(serde_json::to_value(self.port.list_albums(&a.email).await?)?),
            PhotosOperation::ListMedia => Ok(serde_json::to_value(self.port.list_media(&a.email, a.album_id.as_deref(), a.page_size).await?)?),
            PhotosOperation::GetMedia => { let m = a.media_item_id.context("media_item_id required")?; Ok(serde_json::to_value(self.port.get_media(&a.email, &m).await?)?) }
        }
    }
}

// =============================================================================
// MANAGE NOTEBOOKLM
// =============================================================================

#[derive(Serialize, Deserialize, JsonSchema, Debug)] #[serde(rename_all = "snake_case")]
pub enum NotebookLmOperation {
    /// List all notebooks.
    List,
    /// Create a new notebook.
    Create,
    /// Delete a notebook by ID.
    Delete,
    /// Get an AI-generated summary of a notebook's sources.
    GetSummary,
    /// Add a URL as a source to a notebook.
    AddSourceUrl,
    /// Ask a question about the notebook's content (AI chat).
    Chat,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ManageNotebookLmArgs {
    /// The operation to perform.
    pub operation: NotebookLmOperation,
    /// Authenticated user email (or 'me').
    pub email: String,
    /// Notebook ID — required for: delete, get_summary, add_source_url, chat.
    pub notebook_id: Option<String>,
    /// Title — required for: create.
    pub title: Option<String>,
    /// URL to add as source — required for: add_source_url.
    pub url: Option<String>,
    /// Question to ask — required for: chat.
    pub question: Option<String>,
}

pub struct ManageNotebookLmTool { port: Arc<dyn NotebookLmPort> }
impl ManageNotebookLmTool { pub fn new(port: Arc<dyn NotebookLmPort>) -> Self { Self { port } } }

#[async_trait]
impl McpTool for ManageNotebookLmTool {
    fn name(&self) -> &'static str { "manage_notebooklm" }
    fn description(&self) -> &'static str {
        "Manage Google NotebookLM notebooks: list, create, delete, summarize sources, add URL sources, and chat with notebook content."
    }
    fn input_schema(&self) -> serde_json::Value { serde_json::to_value(schemars::schema_for!(ManageNotebookLmArgs)).unwrap() }
    async fn call(&self, arguments: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let a: ManageNotebookLmArgs = serde_json::from_value(arguments)?;
        match a.operation {
            NotebookLmOperation::List => Ok(serde_json::to_value(self.port.list_notebooks(&a.email).await?)?),
            NotebookLmOperation::Create => {
                let title = a.title.context("title required")?;
                Ok(serde_json::to_value(self.port.create_notebook(&a.email, &title).await?)?)
            }
            NotebookLmOperation::Delete => {
                let nid = a.notebook_id.context("notebook_id required")?;
                self.port.delete_notebook(&a.email, &nid).await?;
                Ok(serde_json::json!({"status": "deleted"}))
            }
            NotebookLmOperation::GetSummary => {
                let nid = a.notebook_id.context("notebook_id required")?;
                let summary = self.port.get_summary(&a.email, &nid).await?;
                Ok(serde_json::json!({"summary": summary}))
            }
            NotebookLmOperation::AddSourceUrl => {
                let nid = a.notebook_id.context("notebook_id required")?;
                let url = a.url.context("url required")?;
                let result = self.port.add_source_url(&a.email, &nid, &url).await?;
                Ok(serde_json::json!({"status": "added", "result": result}))
            }
            NotebookLmOperation::Chat => {
                let nid = a.notebook_id.context("notebook_id required")?;
                let question = a.question.context("question required")?;
                let answer = self.port.chat(&a.email, &nid, &question).await?;
                Ok(serde_json::json!({"question": question, "answer": answer}))
            }
        }
    }
}

// =============================================================================
// MANAGE ACCOUNTS
// =============================================================================

#[derive(Serialize, Deserialize, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum AccountsOperation {
    /// List all authenticated accounts.
    List,
    /// Authenticate a new account (opens browser).
    Authenticate,
    /// Check token validity and scopes.
    Status,
    /// Remove an account and its stored tokens.
    Remove,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ManageAccountsArgs {
    /// The operation to perform.
    pub operation: AccountsOperation,
    /// Email — required for: status, remove.
    pub email: Option<String>,
}

pub struct ManageAccountsTool {
    store: Arc<TokenStore>,
}
impl ManageAccountsTool {
    pub fn new(store: Arc<TokenStore>) -> Self { Self { store } }
}

#[async_trait]
impl McpTool for ManageAccountsTool {
    fn name(&self) -> &'static str { "manage_accounts" }
    fn description(&self) -> &'static str {
        "Manage Google Workspace account lifecycle: list, authenticate (opens browser), check status, or remove accounts."
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::to_value(schemars::schema_for!(ManageAccountsArgs)).unwrap()
    }
    async fn call(&self, arguments: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let a: ManageAccountsArgs = serde_json::from_value(arguments).context("Invalid args")?;
        match a.operation {
            AccountsOperation::List => {
                let accounts = self.store.list_accounts().await;
                Ok(serde_json::to_value(accounts)?)
            }
            AccountsOperation::Authenticate => {
                let auth_url = self.store.clone().start_authenticate(None).await?;
                Ok(serde_json::json!({
                    "status": "awaiting_callback",
                    "auth_url": auth_url,
                    "message": "Open this URL in your browser to authenticate. The server is listening on http://localhost:8000 for the OAuth callback."
                }))
            }
            AccountsOperation::Status => {
                let email = a.email.context("email required")?;
                let info = self.store.account_status(&email).await?;
                Ok(serde_json::to_value(info)?)
            }
            AccountsOperation::Remove => {
                let email = a.email.context("email required")?;
                self.store.remove_account(&email).await?;
                Ok(serde_json::json!({ "status": "removed", "email": email }))
            }
        }
    }
}
