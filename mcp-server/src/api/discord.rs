// src/api/discord.rs

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::Value;
use reqwest::Client;

use crate::AppState;

#[derive(Deserialize)]
pub struct DiscordPostRequest {
    pub message: String,
    pub username: Option<String>,
}

// Note: request payload is passed straight to the unified service layer; no local payload struct needed.

pub async fn post_discord_message(
    state: &AppState,
    content: &str,
    username: Option<&str>,
) -> anyhow::Result<Value> {
    // If external discord-api is configured, proxy to it (mirrors faucet proxying model)
    if let Some(base) = &state.config.discord_api_url {
        let url = format!("{}/discord/post", base.trim_end_matches('/'));
        let client = Client::new();
        let resp = client
            .post(url)
            .json(&serde_json::json!({ "message": content, "username": username }))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("discord-api proxy error: {} - {}", status, text);
        }
        let v: Value = resp.json().await.unwrap_or_else(|_| serde_json::json!({"ok": true}));
        return Ok(v);
    }

    // Otherwise delegate to unified service implementation (webhook/bot)
    let res = crate::blockchain::services::discord::send_message(state, content, username).await?;
    Ok(res)
}

pub async fn post_discord_handler(
    State(state): State<AppState>,
    Json(req): Json<DiscordPostRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let res = post_discord_message(&state, &req.message, req.username.as_deref())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(res))
}
