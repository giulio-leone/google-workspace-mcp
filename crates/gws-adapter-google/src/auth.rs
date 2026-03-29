use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

// ============================================================================
// GOOGLE OAUTH2 SCOPES — covers all Workspace apps
// ============================================================================

pub const DEFAULT_SCOPES: &[&str] = &[
    "https://www.googleapis.com/auth/gmail.modify",
    "https://www.googleapis.com/auth/gmail.send",
    "https://www.googleapis.com/auth/calendar",
    "https://www.googleapis.com/auth/drive",
    "https://www.googleapis.com/auth/documents",
    "https://www.googleapis.com/auth/spreadsheets",
    "https://www.googleapis.com/auth/presentations",
    "https://www.googleapis.com/auth/forms.body.readonly",
    "https://www.googleapis.com/auth/forms.responses.readonly",
    "https://www.googleapis.com/auth/tasks",
    "https://www.googleapis.com/auth/meetings.space.readonly",
    "https://www.googleapis.com/auth/photoslibrary.readonly",
    "openid",
    "email",
    "profile",
];

// ============================================================================
// TOKEN TYPES
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub email: String,
    pub scopes: Vec<String>,
}

impl StoredToken {
    pub fn is_expired(&self) -> bool {
        match self.expires_at {
            Some(exp) => Utc::now() >= exp,
            None => true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    pub email: String,
    pub display_name: Option<String>,
    pub scopes: Vec<String>,
    pub token_valid: bool,
}

// ============================================================================
// TOKEN STORE — persists tokens to ~/.config/google-workspace-mcp/accounts/
// ============================================================================

pub struct TokenStore {
    client_id: String,
    client_secret: String,
    storage_dir: PathBuf,
    tokens: RwLock<HashMap<String, StoredToken>>,
    http: reqwest::Client,
}

impl TokenStore {
    pub fn new(client_id: String, client_secret: String) -> Result<Self> {
        let storage_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("google-workspace-mcp")
            .join("accounts");
        std::fs::create_dir_all(&storage_dir)
            .context("Failed to create token storage directory")?;

        // Load existing tokens from disk (called once at startup, before tokio)
        let mut tokens = HashMap::new();
        if let Ok(entries) = std::fs::read_dir(&storage_dir) {
            for entry in entries.flatten() {
                if entry.path().extension().map(|e| e == "json").unwrap_or(false) {
                    if let Ok(data) = std::fs::read_to_string(entry.path()) {
                        if let Ok(token) = serde_json::from_str::<StoredToken>(&data) {
                            tokens.insert(token.email.clone(), token);
                        }
                    }
                }
            }
        }

        Ok(Self {
            client_id,
            client_secret,
            storage_dir,
            tokens: RwLock::new(tokens),
            http: reqwest::Client::new(),
        })
    }

    /// Get a valid access token for the given email, refreshing if needed.
    pub async fn get_access_token(&self, email: &str) -> Result<String> {
        // Check if we have a token
        let maybe_refresh = {
            let tokens = self.tokens.read().await;
            if let Some(token) = tokens.get(email) {
                if !token.is_expired() {
                    return Ok(token.access_token.clone());
                }
                // Token expired — clone refresh token for use after lock is released
                token.refresh_token.clone()
            } else {
                None
            }
        };
        // Lock is now released — safe to call refresh_token which acquires a write lock
        if let Some(refresh) = maybe_refresh {
            return self.refresh_token(email, &refresh).await;
        }
        bail!("No valid token for '{}'. Use manage_accounts with operation 'authenticate' to sign in.", email)
    }

    /// Refresh an expired access token using the refresh_token.
    async fn refresh_token(&self, email: &str, refresh_token: &str) -> Result<String> {
        let resp = self.http
            .post("https://oauth2.googleapis.com/token")
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                ("refresh_token", refresh_token),
                ("grant_type", "refresh_token"),
            ])
            .send().await
            .context("Token refresh request failed")?;

        if !resp.status().is_success() {
            let err = resp.text().await.unwrap_or_default();
            bail!("Token refresh failed: {}", err);
        }

        #[derive(Deserialize)]
        struct RefreshResp {
            access_token: String,
            expires_in: Option<i64>,
        }
        let r: RefreshResp = resp.json().await?;
        let expires_at = r.expires_in.map(|secs| Utc::now() + chrono::Duration::seconds(secs));

        // Update in-memory and persist
        let mut tokens = self.tokens.write().await;
        if let Some(t) = tokens.get_mut(email) {
            t.access_token = r.access_token.clone();
            t.expires_at = expires_at;
            self.persist_token(t)?;
        }

        Ok(r.access_token)
    }

    /// Starts the OAuth2 authorization code flow asynchronously.
    /// Returns the authorization URL immediately and spawns a background task 
    /// to wait for the user to complete the browser flow.
    pub async fn start_authenticate(self: Arc<Self>, scopes: Option<Vec<String>>) -> Result<String> {
        let scopes = scopes.unwrap_or_else(|| DEFAULT_SCOPES.iter().map(|s| s.to_string()).collect());
        let scope_str = scopes.join(" ");

        // Bind listener on fixed port 8000 
        let listener = tokio::net::TcpListener::bind("127.0.0.1:8000").await
            .context("Failed to bind local OAuth redirect server on port 8000. Is the port already in use?")?;
        
        let redirect_uri = "http://localhost:8000/oauth2callback".to_string();

        let state = uuid::Uuid::new_v4().to_string();
        let auth_url = format!(
            "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent&state={}",
            urlencoding::encode(&self.client_id),
            urlencoding::encode(&redirect_uri),
            urlencoding::encode(&scope_str),
            urlencoding::encode(&state),
        );

        // Try silently opening the browser for convenience
        let _ = open::that(&auth_url);
        eprintln!("[gws-mcp] Waiting for OAuth callback on http://localhost:8000 ...");

        // Spawn background task to wait for the callback (timeout after 5 minutes)
        let store = self.clone();
        tokio::spawn(async move {
            if let Err(e) = store.wait_for_callback(listener, redirect_uri, scopes).await {
                eprintln!("[gws-mcp] OAuth flow failed: {}", e);
            }
        });

        Ok(auth_url)
    }

    /// Background task that waits for the redirect callback and exchanges the code.
    async fn wait_for_callback(&self, listener: tokio::net::TcpListener, redirect_uri: String, scopes: Vec<String>) -> Result<()> {
        let accept_future = listener.accept();
        let (mut socket, _) = tokio::time::timeout(std::time::Duration::from_secs(300), accept_future)
            .await.context("Timed out waiting for OAuth callback (5 minutes)")?
            .context("Failed to accept connection")?;

        // Read the HTTP request
        let mut buf = vec![0u8; 4096];
        use tokio::io::AsyncReadExt;
        let n = socket.read(&mut buf).await?;
        let request = String::from_utf8_lossy(&buf[..n]);

        // Extract the authorization code from the GET request
        let code = request
            .lines()
            .next()
            .and_then(|line| {
                let path = line.split_whitespace().nth(1)?;
                let url = url::Url::parse(&format!("http://localhost:8000{}", path)).ok()?;
                url.query_pairs().find(|(k, _)| k == "code").map(|(_, v)| v.to_string())
            })
            .context("No authorization code in callback")?;

        // Send a friendly response back to the browser
        use tokio::io::AsyncWriteExt;
        let response = "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Type: text/html\r\n\r\n<html><head><style>body{font-family:sans-serif;display:flex;justify-content:center;align-items:center;height:100vh;background:#f8f9fa} .card{background:#fff;padding:2rem;border-radius:8px;box-shadow:0 4px 6px rgba(0,0,0,.1);text-align:center}</style></head><body><div class=\"card\"><h2 style=\"color:#0f9d58\">✅ Authenticated successfully!</h2><p>You can close this tab and return to your terminal.</p></div></body></html>";
        socket.write_all(response.as_bytes()).await?;
        socket.shutdown().await?;

        // Exchange code for tokens
        let resp = self.http
            .post("https://oauth2.googleapis.com/token")
            .form(&[
                ("code", code.as_str()),
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                ("redirect_uri", redirect_uri.as_str()),
                ("grant_type", "authorization_code"),
            ])
            .send().await
            .context("Token exchange request failed")?;

        if !resp.status().is_success() {
            let err = resp.text().await.unwrap_or_default();
            bail!("Token exchange failed: {}", err);
        }

        #[derive(Deserialize)]
        struct TokenResp {
            access_token: String,
            refresh_token: Option<String>,
            expires_in: Option<i64>,
            id_token: Option<String>,
        }
        let tr: TokenResp = resp.json().await?;
        let expires_at = tr.expires_in.map(|secs| chrono::Utc::now() + chrono::Duration::seconds(secs));

        // Extract email from the ID token (JWT) or userinfo endpoint
        let email = if let Some(ref id_token) = tr.id_token {
            self.extract_email_from_jwt(id_token).unwrap_or_default()
        } else {
            String::new()
        };

        let email = if email.is_empty() {
            self.fetch_userinfo(&tr.access_token).await.unwrap_or_else(|_| "unknown_account@gmail.com".into())
        } else {
            email
        };

        let stored = StoredToken {
            access_token: tr.access_token,
            refresh_token: tr.refresh_token,
            expires_at,
            email: email.clone(),
            scopes: scopes.clone(),
        };

        self.persist_token(&stored)?;
        self.tokens.write().await.insert(email.clone(), stored);
        
        eprintln!("[gws-mcp] Successfully authenticated and saved tokens for {}", email);
        Ok(())
    }

    /// List all stored accounts.
    pub async fn list_accounts(&self) -> Vec<AccountInfo> {
        let tokens = self.tokens.read().await;
        tokens.values().map(|t| AccountInfo {
            email: t.email.clone(),
            display_name: None,
            scopes: t.scopes.clone(),
            token_valid: !t.is_expired(),
        }).collect()
    }

    /// Check the status of a specific account.
    pub async fn account_status(&self, email: &str) -> Result<AccountInfo> {
        let tokens = self.tokens.read().await;
        let t = tokens.get(email).context("Account not found")?;
        Ok(AccountInfo {
            email: t.email.clone(),
            display_name: None,
            scopes: t.scopes.clone(),
            token_valid: !t.is_expired(),
        })
    }

    /// Remove an account and its stored tokens.
    pub async fn remove_account(&self, email: &str) -> Result<()> {
        self.tokens.write().await.remove(email);
        let path = self.token_path(email);
        if path.exists() {
            std::fs::remove_file(&path).context("Failed to remove token file")?;
        }
        Ok(())
    }

    // -- Internal helpers --

    fn token_path(&self, email: &str) -> PathBuf {
        let safe_name = email.replace('@', "_at_").replace('.', "_");
        self.storage_dir.join(format!("{}.json", safe_name))
    }

    fn persist_token(&self, token: &StoredToken) -> Result<()> {
        let path = self.token_path(&token.email);
        let data = serde_json::to_string_pretty(token)?;
        std::fs::write(&path, data).context("Failed to write token file")?;
        Ok(())
    }

    fn extract_email_from_jwt(&self, jwt: &str) -> Option<String> {
        // JWT is header.payload.signature — we only need the payload
        let parts: Vec<&str> = jwt.split('.').collect();
        if parts.len() < 2 { return None; }
        use base64::Engine;
        let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(parts[1]).ok()?;
        let claims: serde_json::Value = serde_json::from_slice(&decoded).ok()?;
        claims.get("email").and_then(|e| e.as_str()).map(|s| s.to_string())
    }

    async fn fetch_userinfo(&self, access_token: &str) -> Result<String> {
        #[derive(Deserialize)]
        struct UserInfo { email: String }
        let resp = self.http.get("https://www.googleapis.com/oauth2/v2/userinfo")
            .bearer_auth(access_token).send().await?;
        let info: UserInfo = resp.json().await?;
        Ok(info.email)
    }
}
