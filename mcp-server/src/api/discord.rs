// src/api/discord.rs

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::Value;
use std::fmt;
use thiserror::Error;
use tracing::{debug, error, info, warn};

use crate::AppState;

/// Maximum message length allowed by Discord
const MAX_MESSAGE_LENGTH: usize = 2000;
const MAX_USERNAME_LENGTH: usize = 80;

/// Custom error type for Discord API errors
#[derive(Debug, Error)]
pub enum DiscordApiError {
    #[error("Message validation error: {0}")]
    Validation(String),
    #[error("Discord service error: {0}")]
    Service(#[from] anyhow::Error),
    #[error("Discord API error: {0}")]
    Api(String),
}

impl IntoResponse for DiscordApiError {
    fn into_response(self) -> Response {
        let (status, error_message) = match &self {
            DiscordApiError::Validation(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            DiscordApiError::Service(err) => {
                error!("Discord service error: {}", err);
                (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
            }
            DiscordApiError::Api(msg) => {
                error!("Discord API error: {}", msg);
                (StatusCode::BAD_GATEWAY, msg.clone())
            }
        };

        let body = Json(serde_json::json!({ "error": error_message }));
        (status, body).into_response()
    }
}

/// Request payload for Discord message
#[derive(Debug, Deserialize)]
pub struct DiscordPostRequest {
    /// The message content to send (1-2000 characters)
    pub message: String,
    
    /// Optional username to override the default webhook username (1-80 characters)
    #[serde(default)]
    pub username: Option<String>,
}

impl DiscordPostRequest {
    /// Validates the request payload
    fn validate(&self) -> Result<(), DiscordApiError> {
        if self.message.is_empty() {
            return Err(DiscordApiError::Validation(
                "Message cannot be empty".to_string(),
            ));
        }

        if self.message.len() > MAX_MESSAGE_LENGTH {
            return Err(DiscordApiError::Validation(format!(
                "Message too long (max {} characters)",
                MAX_MESSAGE_LENGTH
            )));
        }

        if let Some(username) = &self.username {
            if username.is_empty() {
                return Err(DiscordApiError::Validation(
                    "Username cannot be empty if provided".to_string(),
                ));
            }
            if username.len() > MAX_USERNAME_LENGTH {
                return Err(DiscordApiError::Validation(format!(
                    "Username too long (max {} characters)",
                    MAX_USERNAME_LENGTH
                )));
            }
        }

        Ok(())
    }
}

/// Sends a message to Discord using either the configured webhook or bot token
#[tracing::instrument(skip(state))]
pub async fn post_discord_message(
    state: &AppState,
    content: &str,
    username: Option<&str>,
) -> Result<Value, DiscordApiError> {
    debug!("Sending Discord message (length: {})", content.len());

    // If external discord-api is configured, proxy to it
    if let Some(base) = &state.config.discord_api_url {
        return proxy_to_discord_api(base, content, username).await;
    }

    // Otherwise use the direct implementation
    crate::blockchain::services::discord::send_message(state, content, username)
        .await
        .map_err(DiscordApiError::Service)
}

/// Proxies the request to an external Discord API service
async fn proxy_to_discord_api(
    base_url: &str,
    content: &str,
    username: Option<&str>,
) -> Result<Value, DiscordApiError> {
    use reqwest::Client;

    let url = format!("{}/discord/post", base_url.trim_end_matches('/'));
    debug!("Proxying Discord message to: {}", url);

    let client = Client::new();
    let response = client
        .post(&url)
        .json(&serde_json::json!({ "message": content, "username": username }))
        .send()
        .await
        .map_err(|e| {
            error!("Failed to send request to Discord API: {}", e);
            DiscordApiError::Api(format!("Failed to connect to Discord API: {}", e))
        })?;

    let status = response.status();
    let response_text = response.text().await.unwrap_or_default();

    if !status.is_success() {
        error!(
            "Discord API proxy error: {} - {}",
            status, response_text
        );
        return Err(DiscordApiError::Api(format!(
            "Discord API error: {} - {}",
            status, response_text
        )));
    }

    // Try to parse the response as JSON, fall back to a simple success response
    match serde_json::from_str(&response_text) {
        Ok(json) => Ok(json),
        Err(_) => Ok(serde_json::json!({ "ok": true, "proxied": true })),
    }
}

/// HTTP handler for the POST /discord/message endpoint
#[tracing::instrument(skip(state, req))]
pub async fn post_discord_handler(
    State(state): State<AppState>,
    Json(req): Json<DiscordPostRequest>,
) -> Result<Json<Value>, DiscordApiError> {
    // Validate the request
    req.validate()?;

    // Process the message
    let response = post_discord_message(&state, &req.message, req.username.as_deref()).await?;
    
    info!("Successfully sent Discord message");
    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_validate_discord_post_request() {
        // Valid request
        let req = DiscordPostRequest {
            message: "Hello".to_string(),
            username: Some("test".to_string()),
        };
        assert!(req.validate().is_ok());

        // Empty message
        let req = DiscordPostRequest {
            message: "".to_string(),
            username: None,
        };
        assert!(req.validate().is_err());

        // Message too long
        let req = DiscordPostRequest {
            message: "x".repeat(MAX_MESSAGE_LENGTH + 1),
            username: None,
        };
        assert!(req.validate().is_err());

        // Username too long
        let req = DiscordPostRequest {
            message: "test".to_string(),
            username: Some("x".repeat(MAX_USERNAME_LENGTH + 1)),
        };
        assert!(req.validate().is_err());
    }
}
