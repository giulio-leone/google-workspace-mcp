use serde::{Deserialize, Serialize};

// ============================================================================
// GMAIL
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailMessage {
    pub id: String,
    pub thread_id: String,
    pub snippet: Option<String>,
    #[serde(default)]
    pub label_ids: Vec<String>,
    pub payload: Option<MessagePart>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessagePart {
    pub part_id: Option<String>,
    pub mime_type: String,
    #[serde(default)]
    pub headers: Vec<Header>,
    pub body: Option<MessagePartBody>,
    pub parts: Option<Vec<MessagePart>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Header {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagePartBody {
    pub size: i64,
    pub data: Option<String>,
    #[serde(rename = "attachmentId")]
    pub attachment_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailThread {
    pub id: String,
    pub snippet: Option<String>,
    pub history_id: Option<String>,
    #[serde(default)]
    pub messages: Vec<EmailMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GmailLabel {
    pub id: String,
    pub name: String,
    pub r#type: Option<String>,
}

// ============================================================================
// CALENDAR
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEvent {
    pub id: Option<String>,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub start: Option<EventDateTime>,
    pub end: Option<EventDateTime>,
    pub status: Option<String>,
    pub html_link: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventDateTime {
    pub date: Option<String>,
    pub date_time: Option<String>,
    pub time_zone: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarListEntry {
    pub id: String,
    pub summary: Option<String>,
    pub primary: Option<bool>,
    pub access_role: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FreeBusyResponse {
    pub kind: Option<String>,
    pub calendars: Option<serde_json::Value>,
}

// ============================================================================
// DRIVE
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveFile {
    pub id: Option<String>,
    pub name: Option<String>,
    pub mime_type: Option<String>,
    pub parents: Option<Vec<String>>,
    pub web_view_link: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrivePermission {
    pub id: Option<String>,
    pub r#type: Option<String>,
    pub role: Option<String>,
    pub email_address: Option<String>,
}

// ============================================================================
// SLIDES
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Presentation {
    pub presentation_id: Option<String>,
    pub title: Option<String>,
    pub slides: Option<Vec<Slide>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Slide {
    pub object_id: Option<String>,
}

// ============================================================================
// FORMS
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Form {
    pub form_id: Option<String>,
    pub info: Option<FormInfo>,
    pub responder_uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormInfo {
    pub title: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormResponse {
    pub response_id: Option<String>,
    pub create_time: Option<String>,
    pub answers: Option<serde_json::Value>,
}

// ============================================================================
// TASKS
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskList {
    pub id: Option<String>,
    pub title: Option<String>,
    pub updated: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Option<String>,
    pub title: Option<String>,
    pub notes: Option<String>,
    pub status: Option<String>,
    pub due: Option<String>,
    pub completed: Option<String>,
}

// ============================================================================
// MEET
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConferenceRecord {
    pub name: Option<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub space: Option<MeetSpace>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetSpace {
    pub meeting_code: Option<String>,
    pub meeting_uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Participant {
    pub name: Option<String>,
    pub earliest_start_time: Option<String>,
    pub latest_end_time: Option<String>,
}

// ============================================================================
// WEB API PUSH WATCHERS (shared across Gmail, Calendar, Drive)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushWatchRequest {
    pub id: String,
    pub r#type: String,
    pub address: String,
    pub token: Option<String>,
    pub expiration: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PushWatchResponse {
    pub id: String,
    pub resource_id: String,
    pub resource_uri: String,
    pub token: Option<String>,
    pub expiration: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailWatchRequest {
    pub label_ids: Option<Vec<String>>,
    pub label_filter_action: Option<String>,
    pub topic_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailWatchResponse {
    pub history_id: String,
    pub expiration: i64,
}
