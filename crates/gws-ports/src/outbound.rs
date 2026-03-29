use async_trait::async_trait;
use anyhow::Result;
use gws_domain::*;
use serde::{Serialize, Deserialize};

// ============================================================================
// SHARED VALUE OBJECTS (not domain-core, used by ports only)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SheetFile { pub id: String, pub title: String }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocFile { pub id: String, pub contents: Option<String> }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhotoAlbum { pub id: String, pub title: String }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoMediaItem { pub id: String, pub mime_type: Option<String>, pub base_url: Option<String> }


// ============================================================================
// GMAIL PORT
// ============================================================================

#[async_trait]
pub trait GmailPort: Send + Sync {
    // Read
    async fn get_message(&self, email: &str, message_id: &str) -> Result<EmailMessage>;
    async fn list_messages(&self, email: &str, query: Option<&str>, max_results: Option<u32>) -> Result<Vec<EmailMessage>>;
    async fn get_thread(&self, email: &str, thread_id: &str) -> Result<GmailThread>;

    // Write
    async fn send_message(&self, email: &str, to: &str, subject: &str, body: &str, cc: Option<&str>, bcc: Option<&str>) -> Result<EmailMessage>;
    async fn reply(&self, email: &str, message_id: &str, body: &str) -> Result<EmailMessage>;
    async fn forward(&self, email: &str, message_id: &str, to: &str) -> Result<EmailMessage>;

    // Lifecycle
    async fn trash_message(&self, email: &str, message_id: &str) -> Result<EmailMessage>;
    async fn untrash_message(&self, email: &str, message_id: &str) -> Result<EmailMessage>;
    async fn modify_message(&self, email: &str, message_id: &str, add_labels: Option<Vec<&str>>, remove_labels: Option<Vec<&str>>) -> Result<EmailMessage>;

    // Labels
    async fn list_labels(&self, email: &str) -> Result<Vec<GmailLabel>>;

    // Triage (convenience: unread inbox summary)
    async fn triage(&self, email: &str) -> Result<Vec<EmailMessage>>;

    // Threads
    async fn list_threads(&self, email: &str, query: Option<&str>, max_results: Option<u32>) -> Result<Vec<GmailThread>>;

    // Attachments
    async fn get_attachment(&self, email: &str, message_id: &str, attachment_id: &str) -> Result<String>;

    // Push watcher
    async fn watch(&self, email: &str, topic_name: &str, label_ids: Option<Vec<String>>) -> Result<GmailWatchResponse>;
}

// ============================================================================
// CALENDAR PORT
// ============================================================================

#[async_trait]
pub trait CalendarPort: Send + Sync {
    async fn get_event(&self, email: &str, calendar_id: &str, event_id: &str) -> Result<CalendarEvent>;
    async fn list_events(&self, email: &str, calendar_id: &str, time_min: Option<&str>, time_max: Option<&str>, max_results: Option<u32>, query: Option<&str>) -> Result<Vec<CalendarEvent>>;
    async fn create_event(&self, email: &str, calendar_id: &str, summary: &str, start_time: &str, end_time: &str, description: Option<&str>, attendees: Option<Vec<&str>>, location: Option<&str>) -> Result<CalendarEvent>;
    async fn quick_add(&self, email: &str, calendar_id: &str, text: &str) -> Result<CalendarEvent>;
    async fn update_event(&self, email: &str, calendar_id: &str, event_id: &str, summary: Option<&str>, start_time: Option<&str>, end_time: Option<&str>, description: Option<&str>) -> Result<CalendarEvent>;
    async fn delete_event(&self, email: &str, calendar_id: &str, event_id: &str) -> Result<()>;
    async fn list_calendars(&self, email: &str) -> Result<Vec<CalendarListEntry>>;
    async fn freebusy(&self, email: &str, time_min: &str, time_max: &str, items: Vec<&str>) -> Result<FreeBusyResponse>;
    async fn watch(&self, email: &str, calendar_id: &str, request: PushWatchRequest) -> Result<PushWatchResponse>;
}

// ============================================================================
// DRIVE PORT
// ============================================================================

#[async_trait]
pub trait DrivePort: Send + Sync {
    async fn get_file(&self, email: &str, file_id: &str) -> Result<DriveFile>;
    async fn list_files(&self, email: &str, query: Option<&str>, max_results: Option<u32>) -> Result<Vec<DriveFile>>;
    async fn upload_file(&self, email: &str, name: &str, mime_type: &str, content_base64: &str, parent_folder_id: Option<&str>) -> Result<DriveFile>;
    async fn download_file(&self, email: &str, file_id: &str) -> Result<String>;
    async fn copy_file(&self, email: &str, file_id: &str, new_name: Option<&str>) -> Result<DriveFile>;
    async fn delete_file(&self, email: &str, file_id: &str) -> Result<()>;
    async fn export_file(&self, email: &str, file_id: &str, mime_type: &str) -> Result<String>;
    async fn list_permissions(&self, email: &str, file_id: &str) -> Result<Vec<DrivePermission>>;
    async fn share(&self, email: &str, file_id: &str, share_email: &str, role: &str) -> Result<DrivePermission>;
    async fn unshare(&self, email: &str, file_id: &str, permission_id: &str) -> Result<()>;
    async fn watch(&self, email: &str, file_id: &str, request: PushWatchRequest) -> Result<PushWatchResponse>;
}

// ============================================================================
// DOCS PORT
// ============================================================================

#[async_trait]
pub trait DocsPort: Send + Sync {
    async fn get_doc(&self, email: &str, doc_id: &str) -> Result<DocFile>;
    async fn create_doc(&self, email: &str, title: &str) -> Result<DocFile>;
    async fn append_text(&self, email: &str, doc_id: &str, text: &str) -> Result<()>;
}

// ============================================================================
// SHEETS PORT
// ============================================================================

#[async_trait]
pub trait SheetsPort: Send + Sync {
    async fn get_sheet(&self, email: &str, sheet_id: &str) -> Result<SheetFile>;
    async fn create_sheet(&self, email: &str, title: &str) -> Result<SheetFile>;
    async fn read_range(&self, email: &str, sheet_id: &str, range: &str) -> Result<Vec<Vec<serde_json::Value>>>;
    async fn update_values(&self, email: &str, sheet_id: &str, range: &str, values_json: &str) -> Result<()>;
    async fn append_cells(&self, email: &str, sheet_id: &str, range: &str, values_json: &str) -> Result<()>;
}

// ============================================================================
// SLIDES PORT
// ============================================================================

#[async_trait]
pub trait SlidesPort: Send + Sync {
    async fn get_presentation(&self, email: &str, presentation_id: &str) -> Result<Presentation>;
    async fn create_presentation(&self, email: &str, title: &str) -> Result<Presentation>;
}

// ============================================================================
// FORMS PORT
// ============================================================================

#[async_trait]
pub trait FormsPort: Send + Sync {
    async fn get_form(&self, email: &str, form_id: &str) -> Result<Form>;
    async fn list_responses(&self, email: &str, form_id: &str) -> Result<Vec<FormResponse>>;
}

// ============================================================================
// TASKS PORT
// ============================================================================

#[async_trait]
pub trait TasksPort: Send + Sync {
    async fn list_task_lists(&self, email: &str) -> Result<Vec<TaskList>>;
    async fn list_tasks(&self, email: &str, task_list_id: &str) -> Result<Vec<Task>>;
    async fn get_task(&self, email: &str, task_list_id: &str, task_id: &str) -> Result<Task>;
    async fn create_task(&self, email: &str, task_list_id: &str, title: &str, notes: Option<&str>, due: Option<&str>) -> Result<Task>;
    async fn update_task(&self, email: &str, task_list_id: &str, task_id: &str, title: Option<&str>, notes: Option<&str>, due: Option<&str>) -> Result<Task>;
    async fn complete_task(&self, email: &str, task_list_id: &str, task_id: &str) -> Result<Task>;
    async fn delete_task(&self, email: &str, task_list_id: &str, task_id: &str) -> Result<()>;
    async fn create_task_list(&self, email: &str, title: &str) -> Result<TaskList>;
    async fn delete_task_list(&self, email: &str, task_list_id: &str) -> Result<()>;
}

// ============================================================================
// MEET PORT
// ============================================================================

#[async_trait]
pub trait MeetPort: Send + Sync {
    async fn list_conferences(&self, email: &str, max_results: Option<u32>) -> Result<Vec<ConferenceRecord>>;
    async fn get_conference(&self, email: &str, conference_id: &str) -> Result<ConferenceRecord>;
    async fn list_participants(&self, email: &str, conference_id: &str) -> Result<Vec<Participant>>;
}

// ============================================================================
// PHOTOS PORT
// ============================================================================

#[async_trait]
pub trait PhotosPort: Send + Sync {
    async fn list_albums(&self, email: &str) -> Result<Vec<PhotoAlbum>>;
    async fn list_media(&self, email: &str, album_id: Option<&str>, page_size: Option<u32>) -> Result<Vec<PhotoMediaItem>>;
    async fn get_media(&self, email: &str, media_item_id: &str) -> Result<PhotoMediaItem>;
}

// ============================================================================
// NOTEBOOKLM PORT
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotebookLmEntry {
    pub id: String,
    pub title: String,
    pub created_at: Option<String>,
}

#[async_trait]
pub trait NotebookLmPort: Send + Sync {
    async fn list_notebooks(&self, email: &str) -> Result<Vec<NotebookLmEntry>>;
    async fn create_notebook(&self, email: &str, title: &str) -> Result<NotebookLmEntry>;
    async fn delete_notebook(&self, email: &str, notebook_id: &str) -> Result<()>;
    async fn get_summary(&self, email: &str, notebook_id: &str) -> Result<String>;
    async fn add_source_url(&self, email: &str, notebook_id: &str, url: &str) -> Result<serde_json::Value>;
    async fn chat(&self, email: &str, notebook_id: &str, question: &str) -> Result<String>;
}
