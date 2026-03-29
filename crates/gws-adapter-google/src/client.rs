use anyhow::{Context, Result};
use async_trait::async_trait;
use gws_domain::*;
use gws_ports::*;
use reqwest::Client;
use serde_json::json;
use std::sync::Arc;

use crate::auth::TokenStore;

/// Multi-account Google API client.
///
/// Each method receives an `email` parameter and dynamically resolves
/// the access token for that account from the shared TokenStore.
/// Tokens are auto-refreshed transparently when expired.
pub struct GoogleClient {
    client: Client,
    pub token_store: Arc<TokenStore>,
}

impl GoogleClient {
    pub fn new(token_store: Arc<TokenStore>) -> Self {
        Self { client: Client::new(), token_store }
    }

    // ========================================================================
    // HTTP PRIMITIVES — resolve token per-request from the store
    // ========================================================================

    async fn token(&self, email: &str) -> Result<String> {
        self.token_store.get_access_token(email).await
    }

    pub(crate) async fn get(&self, email: &str, url: &str) -> Result<reqwest::Response> {
        let tok = self.token(email).await?;
        self.client.get(url).bearer_auth(&tok).send().await
            .context("GET request failed")?
            .error_for_status().context("GET returned error status")
    }

    pub(crate) async fn post<T: serde::Serialize>(&self, email: &str, url: &str, body: &T) -> Result<reqwest::Response> {
        let tok = self.token(email).await?;
        self.client.post(url).bearer_auth(&tok).json(body).send().await
            .context("POST request failed")?
            .error_for_status().context("POST returned error status")
    }

    pub(crate) async fn put<T: serde::Serialize>(&self, email: &str, url: &str, body: &T) -> Result<reqwest::Response> {
        let tok = self.token(email).await?;
        self.client.put(url).bearer_auth(&tok).json(body).send().await
            .context("PUT request failed")?
            .error_for_status().context("PUT returned error status")
    }

    pub(crate) async fn patch<T: serde::Serialize>(&self, email: &str, url: &str, body: &T) -> Result<reqwest::Response> {
        let tok = self.token(email).await?;
        self.client.patch(url).bearer_auth(&tok).json(body).send().await
            .context("PATCH request failed")?
            .error_for_status().context("PATCH returned error status")
    }

    pub(crate) async fn delete(&self, email: &str, url: &str) -> Result<reqwest::Response> {
        let tok = self.token(email).await?;
        self.client.delete(url).bearer_auth(&tok).send().await
            .context("DELETE request failed")?
            .error_for_status().context("DELETE returned error status")
    }

    pub async fn execute_batch(&self, email: &str, batch_url: &str, sub_requests: Vec<(String, String, Option<serde_json::Value>)>) -> Result<Vec<serde_json::Value>> {
        let tok = self.token(email).await?;
        let boundary = format!("batch_{}", uuid::Uuid::new_v4().simple());
        let mut body = String::new();
        for (i, (method, url, payload)) in sub_requests.iter().enumerate() {
            body.push_str(&format!("--{}\r\nContent-Type: application/http\r\nContent-ID: <item{}>\r\n\r\n{} {} HTTP/1.1\r\n", boundary, i, method, url));
            if let Some(p) = payload { body.push_str(&format!("Content-Type: application/json\r\n\r\n{}", p)); }
            body.push_str("\r\n");
        }
        body.push_str(&format!("--{}--", boundary));
        let resp = self.client.post(batch_url).bearer_auth(&tok)
            .header("Content-Type", format!("multipart/mixed; boundary={}", boundary))
            .body(body).send().await.context("Batch request failed")?;
        let text = resp.text().await?;
        Ok(vec![json!({ "batch_response": text })])
    }
}

// ============================================================================
// GMAIL
// ============================================================================

fn build_rfc2822(to: &str, subject: &str, body: &str, cc: Option<&str>, bcc: Option<&str>, in_reply_to: Option<&str>, references: Option<&str>) -> String {
    use base64::Engine;
    let mut headers = format!("To: {}\r\nSubject: {}\r\nContent-Type: text/plain; charset=utf-8\r\n", to, subject);
    if let Some(c) = cc { headers.push_str(&format!("Cc: {}\r\n", c)); }
    if let Some(b) = bcc { headers.push_str(&format!("Bcc: {}\r\n", b)); }
    if let Some(r) = in_reply_to { headers.push_str(&format!("In-Reply-To: {}\r\nReferences: {}\r\n", r, references.unwrap_or(r))); }
    headers.push_str(&format!("\r\n{}", body));
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(headers.as_bytes())
}

#[async_trait]
impl GmailPort for GoogleClient {
    async fn get_message(&self, email: &str, message_id: &str) -> Result<EmailMessage> {
        let url = format!("https://gmail.googleapis.com/gmail/v1/users/{}/messages/{}?format=full", email, message_id);
        Ok(self.get(email, &url).await?.json().await?)
    }

    async fn list_messages(&self, email: &str, query: Option<&str>, max_results: Option<u32>) -> Result<Vec<EmailMessage>> {
        let mut url = format!("https://gmail.googleapis.com/gmail/v1/users/{}/messages?maxResults={}", email, max_results.unwrap_or(10));
        if let Some(q) = query { url.push_str(&format!("&q={}", urlencoding::encode(q))); }
        #[derive(serde::Deserialize)] struct R { messages: Option<Vec<EmailMessage>> }
        Ok(self.get(email, &url).await?.json::<R>().await?.messages.unwrap_or_default())
    }

    async fn get_thread(&self, email: &str, thread_id: &str) -> Result<GmailThread> {
        let url = format!("https://gmail.googleapis.com/gmail/v1/users/{}/threads/{}", email, thread_id);
        Ok(self.get(email, &url).await?.json().await?)
    }

    async fn send_message(&self, email: &str, to: &str, subject: &str, body: &str, cc: Option<&str>, bcc: Option<&str>) -> Result<EmailMessage> {
        let raw = build_rfc2822(to, subject, body, cc, bcc, None, None);
        let url = format!("https://gmail.googleapis.com/gmail/v1/users/{}/messages/send", email);
        Ok(self.post(email, &url, &json!({ "raw": raw })).await?.json().await?)
    }

    async fn reply(&self, email: &str, message_id: &str, body: &str) -> Result<EmailMessage> {
        let orig = self.get_message(email, message_id).await?;
        let raw = build_rfc2822("", "Re: ", body, None, None, Some(message_id), None);
        let url = format!("https://gmail.googleapis.com/gmail/v1/users/{}/messages/send", email);
        Ok(self.post(email, &url, &json!({ "raw": raw, "threadId": orig.thread_id })).await?.json().await?)
    }

    async fn forward(&self, email: &str, message_id: &str, to: &str) -> Result<EmailMessage> {
        let orig = self.get_message(email, message_id).await?;
        let raw = build_rfc2822(to, "Fwd: ", &orig.snippet.unwrap_or_default(), None, None, None, None);
        let url = format!("https://gmail.googleapis.com/gmail/v1/users/{}/messages/send", email);
        Ok(self.post(email, &url, &json!({ "raw": raw, "threadId": orig.thread_id })).await?.json().await?)
    }

    async fn trash_message(&self, email: &str, message_id: &str) -> Result<EmailMessage> {
        let url = format!("https://gmail.googleapis.com/gmail/v1/users/{}/messages/{}/trash", email, message_id);
        Ok(self.post(email, &url, &json!({})).await?.json().await?)
    }

    async fn untrash_message(&self, email: &str, message_id: &str) -> Result<EmailMessage> {
        let url = format!("https://gmail.googleapis.com/gmail/v1/users/{}/messages/{}/untrash", email, message_id);
        Ok(self.post(email, &url, &json!({})).await?.json().await?)
    }

    async fn modify_message(&self, email: &str, message_id: &str, add_labels: Option<Vec<&str>>, remove_labels: Option<Vec<&str>>) -> Result<EmailMessage> {
        let url = format!("https://gmail.googleapis.com/gmail/v1/users/{}/messages/{}/modify", email, message_id);
        let payload = json!({ "addLabelIds": add_labels.unwrap_or_default(), "removeLabelIds": remove_labels.unwrap_or_default() });
        Ok(self.post(email, &url, &payload).await?.json().await?)
    }

    async fn list_labels(&self, email: &str) -> Result<Vec<GmailLabel>> {
        let url = format!("https://gmail.googleapis.com/gmail/v1/users/{}/labels", email);
        #[derive(serde::Deserialize)] struct R { labels: Option<Vec<GmailLabel>> }
        Ok(self.get(email, &url).await?.json::<R>().await?.labels.unwrap_or_default())
    }

    async fn triage(&self, email: &str) -> Result<Vec<EmailMessage>> {
        self.list_messages(email, Some("is:unread in:inbox"), Some(20)).await
    }

    async fn list_threads(&self, email: &str, query: Option<&str>, max_results: Option<u32>) -> Result<Vec<GmailThread>> {
        let mut url = format!("https://gmail.googleapis.com/gmail/v1/users/{}/threads?maxResults={}", email, max_results.unwrap_or(10));
        if let Some(q) = query { url.push_str(&format!("&q={}", urlencoding::encode(q))); }
        #[derive(serde::Deserialize)] struct R { threads: Option<Vec<GmailThread>> }
        Ok(self.get(email, &url).await?.json::<R>().await?.threads.unwrap_or_default())
    }

    async fn get_attachment(&self, email: &str, message_id: &str, attachment_id: &str) -> Result<String> {
        let url = format!("https://gmail.googleapis.com/gmail/v1/users/{}/messages/{}/attachments/{}", email, message_id, attachment_id);
        #[derive(serde::Deserialize)] struct R { data: String }
        Ok(self.get(email, &url).await?.json::<R>().await?.data)
    }

    async fn watch(&self, email: &str, topic_name: &str, label_ids: Option<Vec<String>>) -> Result<GmailWatchResponse> {
        let url = format!("https://gmail.googleapis.com/gmail/v1/users/{}/watch", email);
        let req = GmailWatchRequest { topic_name: topic_name.to_string(), label_ids, label_filter_action: None };
        Ok(self.post(email, &url, &req).await?.json().await?)
    }
}

// ============================================================================
// CALENDAR
// ============================================================================

#[async_trait]
impl CalendarPort for GoogleClient {
    async fn get_event(&self, email: &str, calendar_id: &str, event_id: &str) -> Result<CalendarEvent> {
        let url = format!("https://www.googleapis.com/calendar/v3/calendars/{}/events/{}", urlencoding::encode(calendar_id), event_id);
        Ok(self.get(email, &url).await?.json().await?)
    }

    async fn list_events(&self, email: &str, calendar_id: &str, time_min: Option<&str>, time_max: Option<&str>, max_results: Option<u32>, query: Option<&str>) -> Result<Vec<CalendarEvent>> {
        let mut url = format!("https://www.googleapis.com/calendar/v3/calendars/{}/events?maxResults={}&singleEvents=true&orderBy=startTime",
            urlencoding::encode(calendar_id), max_results.unwrap_or(10));
        if let Some(t) = time_min { url.push_str(&format!("&timeMin={}", urlencoding::encode(t))); }
        if let Some(t) = time_max { url.push_str(&format!("&timeMax={}", urlencoding::encode(t))); }
        if let Some(q) = query { url.push_str(&format!("&q={}", urlencoding::encode(q))); }
        #[derive(serde::Deserialize)] struct R { items: Option<Vec<CalendarEvent>> }
        Ok(self.get(email, &url).await?.json::<R>().await?.items.unwrap_or_default())
    }

    async fn create_event(&self, email: &str, calendar_id: &str, summary: &str, start_time: &str, end_time: &str, description: Option<&str>, attendees: Option<Vec<&str>>, location: Option<&str>) -> Result<CalendarEvent> {
        let url = format!("https://www.googleapis.com/calendar/v3/calendars/{}/events", urlencoding::encode(calendar_id));
        let mut payload = json!({ "summary": summary, "start": { "dateTime": start_time }, "end": { "dateTime": end_time } });
        if let Some(d) = description { payload["description"] = json!(d); }
        if let Some(l) = location { payload["location"] = json!(l); }
        if let Some(a) = attendees { payload["attendees"] = json!(a.iter().map(|e| json!({"email": e})).collect::<Vec<_>>()); }
        Ok(self.post(email, &url, &payload).await?.json().await?)
    }

    async fn quick_add(&self, email: &str, calendar_id: &str, text: &str) -> Result<CalendarEvent> {
        let url = format!("https://www.googleapis.com/calendar/v3/calendars/{}/events/quickAdd?text={}", urlencoding::encode(calendar_id), urlencoding::encode(text));
        Ok(self.post(email, &url, &json!({})).await?.json().await?)
    }

    async fn update_event(&self, email: &str, calendar_id: &str, event_id: &str, summary: Option<&str>, start_time: Option<&str>, end_time: Option<&str>, description: Option<&str>) -> Result<CalendarEvent> {
        let url = format!("https://www.googleapis.com/calendar/v3/calendars/{}/events/{}", urlencoding::encode(calendar_id), event_id);
        let mut payload = json!({});
        if let Some(s) = summary { payload["summary"] = json!(s); }
        if let Some(s) = start_time { payload["start"] = json!({ "dateTime": s }); }
        if let Some(e) = end_time { payload["end"] = json!({ "dateTime": e }); }
        if let Some(d) = description { payload["description"] = json!(d); }
        Ok(self.patch(email, &url, &payload).await?.json().await?)
    }

    async fn delete_event(&self, email: &str, calendar_id: &str, event_id: &str) -> Result<()> {
        let url = format!("https://www.googleapis.com/calendar/v3/calendars/{}/events/{}", urlencoding::encode(calendar_id), event_id);
        self.delete(email, &url).await?; Ok(())
    }

    async fn list_calendars(&self, email: &str) -> Result<Vec<CalendarListEntry>> {
        let url = "https://www.googleapis.com/calendar/v3/users/me/calendarList";
        #[derive(serde::Deserialize)] struct R { items: Option<Vec<CalendarListEntry>> }
        Ok(self.get(email, url).await?.json::<R>().await?.items.unwrap_or_default())
    }

    async fn freebusy(&self, email: &str, time_min: &str, time_max: &str, items: Vec<&str>) -> Result<FreeBusyResponse> {
        let url = "https://www.googleapis.com/calendar/v3/freeBusy";
        let payload = json!({ "timeMin": time_min, "timeMax": time_max, "items": items.iter().map(|i| json!({"id": i})).collect::<Vec<_>>() });
        Ok(self.post(email, url, &payload).await?.json().await?)
    }

    async fn watch(&self, email: &str, calendar_id: &str, request: PushWatchRequest) -> Result<PushWatchResponse> {
        let url = format!("https://www.googleapis.com/calendar/v3/calendars/{}/events/watch", urlencoding::encode(calendar_id));
        Ok(self.post(email, &url, &request).await?.json().await?)
    }
}

// ============================================================================
// DRIVE
// ============================================================================

#[async_trait]
impl DrivePort for GoogleClient {
    async fn get_file(&self, email: &str, file_id: &str) -> Result<DriveFile> {
        let url = format!("https://www.googleapis.com/drive/v3/files/{}?fields=id,name,mimeType,parents,webViewLink", file_id);
        Ok(self.get(email, &url).await?.json().await?)
    }

    async fn list_files(&self, email: &str, query: Option<&str>, max_results: Option<u32>) -> Result<Vec<DriveFile>> {
        let mut url = format!("https://www.googleapis.com/drive/v3/files?pageSize={}&fields=files(id,name,mimeType,parents,webViewLink)", max_results.unwrap_or(10));
        if let Some(q) = query { url.push_str(&format!("&q={}", urlencoding::encode(q))); }
        #[derive(serde::Deserialize)] struct R { files: Option<Vec<DriveFile>> }
        Ok(self.get(email, &url).await?.json::<R>().await?.files.unwrap_or_default())
    }

    async fn upload_file(&self, email: &str, name: &str, mime_type: &str, content_base64: &str, parent_folder_id: Option<&str>) -> Result<DriveFile> {
        let tok = self.token(email).await?;
        let url = "https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart&fields=id,name,mimeType,parents,webViewLink";
        let mut metadata = json!({ "name": name });
        if let Some(p) = parent_folder_id { metadata["parents"] = json!([p]); }
        let boundary = format!("batch_{}", uuid::Uuid::new_v4().simple());
        let body = format!(
            "--{b}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{meta}\r\n--{b}\r\nContent-Type: {mime}\r\nContent-Transfer-Encoding: base64\r\n\r\n{data}\r\n--{b}--",
            b = boundary, meta = metadata, mime = mime_type, data = content_base64
        );
        let resp = self.client.post(url).bearer_auth(&tok)
            .header("Content-Type", format!("multipart/related; boundary={}", boundary))
            .body(body).send().await?.error_for_status()?;
        Ok(resp.json().await?)
    }

    async fn download_file(&self, email: &str, file_id: &str) -> Result<String> {
        let url = format!("https://www.googleapis.com/drive/v3/files/{}?alt=media", file_id);
        Ok(self.get(email, &url).await?.text().await?)
    }

    async fn copy_file(&self, email: &str, file_id: &str, new_name: Option<&str>) -> Result<DriveFile> {
        let url = format!("https://www.googleapis.com/drive/v3/files/{}/copy?fields=id,name,mimeType", file_id);
        let payload = if let Some(n) = new_name { json!({"name": n}) } else { json!({}) };
        Ok(self.post(email, &url, &payload).await?.json().await?)
    }

    async fn delete_file(&self, email: &str, file_id: &str) -> Result<()> {
        let url = format!("https://www.googleapis.com/drive/v3/files/{}", file_id);
        self.delete(email, &url).await?; Ok(())
    }

    async fn export_file(&self, email: &str, file_id: &str, mime_type: &str) -> Result<String> {
        let url = format!("https://www.googleapis.com/drive/v3/files/{}/export?mimeType={}", file_id, urlencoding::encode(mime_type));
        Ok(self.get(email, &url).await?.text().await?)
    }

    async fn list_permissions(&self, email: &str, file_id: &str) -> Result<Vec<DrivePermission>> {
        let url = format!("https://www.googleapis.com/drive/v3/files/{}/permissions?fields=permissions(id,type,role,emailAddress)", file_id);
        #[derive(serde::Deserialize)] struct R { permissions: Option<Vec<DrivePermission>> }
        Ok(self.get(email, &url).await?.json::<R>().await?.permissions.unwrap_or_default())
    }

    async fn share(&self, email: &str, file_id: &str, share_email: &str, role: &str) -> Result<DrivePermission> {
        let url = format!("https://www.googleapis.com/drive/v3/files/{}/permissions", file_id);
        Ok(self.post(email, &url, &json!({ "type": "user", "role": role, "emailAddress": share_email })).await?.json().await?)
    }

    async fn unshare(&self, email: &str, file_id: &str, permission_id: &str) -> Result<()> {
        let url = format!("https://www.googleapis.com/drive/v3/files/{}/permissions/{}", file_id, permission_id);
        self.delete(email, &url).await?; Ok(())
    }

    async fn watch(&self, email: &str, file_id: &str, request: PushWatchRequest) -> Result<PushWatchResponse> {
        let url = format!("https://www.googleapis.com/drive/v3/files/{}/watch", file_id);
        Ok(self.post(email, &url, &request).await?.json().await?)
    }
}

// ============================================================================
// DOCS
// ============================================================================

#[async_trait]
impl DocsPort for GoogleClient {
    async fn get_doc(&self, email: &str, doc_id: &str) -> Result<DocFile> {
        let url = format!("https://docs.googleapis.com/v1/documents/{}", doc_id);
        Ok(self.get(email, &url).await?.json().await?)
    }
    async fn create_doc(&self, email: &str, title: &str) -> Result<DocFile> {
        let url = "https://docs.googleapis.com/v1/documents";
        Ok(self.post(email, url, &json!({ "title": title })).await?.json().await?)
    }
    async fn append_text(&self, email: &str, doc_id: &str, text: &str) -> Result<()> {
        let url = format!("https://docs.googleapis.com/v1/documents/{}:batchUpdate", doc_id);
        self.post(email, &url, &json!({ "requests": [{ "insertText": { "text": text, "endOfSegmentLocation": {} } }] })).await?;
        Ok(())
    }
}

// ============================================================================
// SHEETS
// ============================================================================

#[async_trait]
impl SheetsPort for GoogleClient {
    async fn get_sheet(&self, email: &str, sheet_id: &str) -> Result<SheetFile> {
        let url = format!("https://sheets.googleapis.com/v4/spreadsheets/{}", sheet_id);
        #[derive(serde::Deserialize)] #[serde(rename_all="camelCase")] struct R { spreadsheet_id: String, properties: P }
        #[derive(serde::Deserialize)] struct P { title: String }
        let s = self.get(email, &url).await?.json::<R>().await?;
        Ok(SheetFile { id: s.spreadsheet_id, title: s.properties.title })
    }
    async fn create_sheet(&self, email: &str, title: &str) -> Result<SheetFile> {
        let url = "https://sheets.googleapis.com/v4/spreadsheets";
        #[derive(serde::Deserialize)] #[serde(rename_all="camelCase")] struct R { spreadsheet_id: String, properties: P }
        #[derive(serde::Deserialize)] struct P { title: String }
        let s = self.post(email, url, &json!({ "properties": { "title": title } })).await?.json::<R>().await?;
        Ok(SheetFile { id: s.spreadsheet_id, title: s.properties.title })
    }
    async fn read_range(&self, email: &str, sheet_id: &str, range: &str) -> Result<Vec<Vec<serde_json::Value>>> {
        let url = format!("https://sheets.googleapis.com/v4/spreadsheets/{}/values/{}", sheet_id, urlencoding::encode(range));
        #[derive(serde::Deserialize)] struct R { values: Option<Vec<Vec<serde_json::Value>>> }
        Ok(self.get(email, &url).await?.json::<R>().await?.values.unwrap_or_default())
    }
    async fn update_values(&self, email: &str, sheet_id: &str, range: &str, values_json: &str) -> Result<()> {
        let url = format!("https://sheets.googleapis.com/v4/spreadsheets/{}/values/{}?valueInputOption=USER_ENTERED", sheet_id, urlencoding::encode(range));
        let parsed: Vec<Vec<serde_json::Value>> = serde_json::from_str(values_json)?;
        self.put(email, &url, &json!({ "range": range, "values": parsed })).await?; Ok(())
    }
    async fn append_cells(&self, email: &str, sheet_id: &str, range: &str, values_json: &str) -> Result<()> {
        let url = format!("https://sheets.googleapis.com/v4/spreadsheets/{}/values/{}:append?valueInputOption=USER_ENTERED", sheet_id, urlencoding::encode(range));
        let parsed: Vec<Vec<serde_json::Value>> = serde_json::from_str(values_json)?;
        self.post(email, &url, &json!({ "range": range, "values": parsed })).await?; Ok(())
    }
}

// ============================================================================
// SLIDES
// ============================================================================

#[async_trait]
impl SlidesPort for GoogleClient {
    async fn get_presentation(&self, email: &str, presentation_id: &str) -> Result<Presentation> {
        let url = format!("https://slides.googleapis.com/v1/presentations/{}", presentation_id);
        Ok(self.get(email, &url).await?.json().await?)
    }
    async fn create_presentation(&self, email: &str, title: &str) -> Result<Presentation> {
        let url = "https://slides.googleapis.com/v1/presentations";
        Ok(self.post(email, url, &json!({ "title": title })).await?.json().await?)
    }
}

// ============================================================================
// FORMS
// ============================================================================

#[async_trait]
impl FormsPort for GoogleClient {
    async fn get_form(&self, email: &str, form_id: &str) -> Result<Form> {
        let url = format!("https://forms.googleapis.com/v1/forms/{}", form_id);
        Ok(self.get(email, &url).await?.json().await?)
    }
    async fn list_responses(&self, email: &str, form_id: &str) -> Result<Vec<FormResponse>> {
        let url = format!("https://forms.googleapis.com/v1/forms/{}/responses", form_id);
        #[derive(serde::Deserialize)] struct R { responses: Option<Vec<FormResponse>> }
        Ok(self.get(email, &url).await?.json::<R>().await?.responses.unwrap_or_default())
    }
}

// ============================================================================
// TASKS
// ============================================================================

#[async_trait]
impl TasksPort for GoogleClient {
    async fn list_task_lists(&self, email: &str) -> Result<Vec<TaskList>> {
        let url = "https://tasks.googleapis.com/tasks/v1/users/@me/lists";
        #[derive(serde::Deserialize)] struct R { items: Option<Vec<TaskList>> }
        Ok(self.get(email, url).await?.json::<R>().await?.items.unwrap_or_default())
    }
    async fn list_tasks(&self, email: &str, task_list_id: &str) -> Result<Vec<Task>> {
        let url = format!("https://tasks.googleapis.com/tasks/v1/lists/{}/tasks", task_list_id);
        #[derive(serde::Deserialize)] struct R { items: Option<Vec<Task>> }
        Ok(self.get(email, &url).await?.json::<R>().await?.items.unwrap_or_default())
    }
    async fn get_task(&self, email: &str, task_list_id: &str, task_id: &str) -> Result<Task> {
        let url = format!("https://tasks.googleapis.com/tasks/v1/lists/{}/tasks/{}", task_list_id, task_id);
        Ok(self.get(email, &url).await?.json().await?)
    }
    async fn create_task(&self, email: &str, task_list_id: &str, title: &str, notes: Option<&str>, due: Option<&str>) -> Result<Task> {
        let url = format!("https://tasks.googleapis.com/tasks/v1/lists/{}/tasks", task_list_id);
        let mut p = json!({ "title": title });
        if let Some(n) = notes { p["notes"] = json!(n); }
        if let Some(d) = due { p["due"] = json!(d); }
        Ok(self.post(email, &url, &p).await?.json().await?)
    }
    async fn update_task(&self, email: &str, task_list_id: &str, task_id: &str, title: Option<&str>, notes: Option<&str>, due: Option<&str>) -> Result<Task> {
        let url = format!("https://tasks.googleapis.com/tasks/v1/lists/{}/tasks/{}", task_list_id, task_id);
        let mut p = json!({});
        if let Some(t) = title { p["title"] = json!(t); }
        if let Some(n) = notes { p["notes"] = json!(n); }
        if let Some(d) = due { p["due"] = json!(d); }
        Ok(self.patch(email, &url, &p).await?.json().await?)
    }
    async fn complete_task(&self, email: &str, task_list_id: &str, task_id: &str) -> Result<Task> {
        let url = format!("https://tasks.googleapis.com/tasks/v1/lists/{}/tasks/{}", task_list_id, task_id);
        Ok(self.patch(email, &url, &json!({ "status": "completed" })).await?.json().await?)
    }
    async fn delete_task(&self, email: &str, task_list_id: &str, task_id: &str) -> Result<()> {
        let url = format!("https://tasks.googleapis.com/tasks/v1/lists/{}/tasks/{}", task_list_id, task_id);
        self.delete(email, &url).await?; Ok(())
    }
    async fn create_task_list(&self, email: &str, title: &str) -> Result<TaskList> {
        let url = "https://tasks.googleapis.com/tasks/v1/users/@me/lists";
        Ok(self.post(email, url, &json!({ "title": title })).await?.json().await?)
    }
    async fn delete_task_list(&self, email: &str, task_list_id: &str) -> Result<()> {
        let url = format!("https://tasks.googleapis.com/tasks/v1/users/@me/lists/{}", task_list_id);
        self.delete(email, &url).await?; Ok(())
    }
}

// ============================================================================
// MEET
// ============================================================================

#[async_trait]
impl MeetPort for GoogleClient {
    async fn list_conferences(&self, email: &str, max_results: Option<u32>) -> Result<Vec<ConferenceRecord>> {
        let url = format!("https://meet.googleapis.com/v2/conferenceRecords?pageSize={}", max_results.unwrap_or(25));
        #[derive(serde::Deserialize)] #[serde(rename_all="camelCase")] struct R { conference_records: Option<Vec<ConferenceRecord>> }
        Ok(self.get(email, &url).await?.json::<R>().await?.conference_records.unwrap_or_default())
    }
    async fn get_conference(&self, email: &str, conference_id: &str) -> Result<ConferenceRecord> {
        let url = format!("https://meet.googleapis.com/v2/{}", conference_id);
        Ok(self.get(email, &url).await?.json().await?)
    }
    async fn list_participants(&self, email: &str, conference_id: &str) -> Result<Vec<Participant>> {
        let url = format!("https://meet.googleapis.com/v2/{}/participants", conference_id);
        #[derive(serde::Deserialize)] struct R { participants: Option<Vec<Participant>> }
        Ok(self.get(email, &url).await?.json::<R>().await?.participants.unwrap_or_default())
    }
}

// ============================================================================
// PHOTOS
// ============================================================================

#[async_trait]
impl PhotosPort for GoogleClient {
    async fn list_albums(&self, email: &str) -> Result<Vec<PhotoAlbum>> {
        let url = "https://photoslibrary.googleapis.com/v1/albums";
        #[derive(serde::Deserialize)] struct R { albums: Option<Vec<PhotoAlbum>> }
        Ok(self.get(email, url).await?.json::<R>().await?.albums.unwrap_or_default())
    }
    async fn list_media(&self, email: &str, album_id: Option<&str>, page_size: Option<u32>) -> Result<Vec<PhotoMediaItem>> {
        if let Some(aid) = album_id {
            let url = "https://photoslibrary.googleapis.com/v1/mediaItems:search";
            #[derive(serde::Deserialize)] #[serde(rename_all="camelCase")] struct R { media_items: Option<Vec<PhotoMediaItem>> }
            Ok(self.post(email, url, &json!({ "albumId": aid, "pageSize": page_size.unwrap_or(50) })).await?.json::<R>().await?.media_items.unwrap_or_default())
        } else {
            let url = format!("https://photoslibrary.googleapis.com/v1/mediaItems?pageSize={}", page_size.unwrap_or(50));
            #[derive(serde::Deserialize)] #[serde(rename_all="camelCase")] struct R { media_items: Option<Vec<PhotoMediaItem>> }
            Ok(self.get(email, &url).await?.json::<R>().await?.media_items.unwrap_or_default())
        }
    }
    async fn get_media(&self, email: &str, media_item_id: &str) -> Result<PhotoMediaItem> {
        let url = format!("https://photoslibrary.googleapis.com/v1/mediaItems/{}", media_item_id);
        Ok(self.get(email, &url).await?.json().await?)
    }
}

// ============================================================================
// NOTEBOOKLM
// ============================================================================

#[async_trait]
impl NotebookLmPort for GoogleClient {
    async fn list_notebooks(&self) -> Result<Vec<Notebook>> { Ok(vec![]) }
}
