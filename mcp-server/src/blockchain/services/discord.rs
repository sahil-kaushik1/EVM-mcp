use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::Value;
use tracing::{debug, error, info, warn, instrument};
use url::Url;

use crate::AppState;

const MAX_MESSAGE_LENGTH: usize = 2000;
const MAX_USERNAME_LENGTH: usize = 80;

#[instrument(skip(state), fields(
    message_length = content.len(),
    has_username = username.is_some()
))]
pub async fn send_message(state: &AppState, content: &str, username: Option<&str>) -> Result<Value> {
    // Input validation
    if content.is_empty() {
        return Err(anyhow::anyhow!("Message content cannot be empty"));
    }
    if content.len() > MAX_MESSAGE_LENGTH {
        return Err(anyhow::anyhow!(
            "Message too long (max {} characters)",
            MAX_MESSAGE_LENGTH
        ));
    }
    if let Some(name) = username {
        if name.len() > MAX_USERNAME_LENGTH {
            return Err(anyhow::anyhow!(
                "Username too long (max {} characters)",
                MAX_USERNAME_LENGTH
            ));
        }
    }

    let client = Client::new();
    
    // Webhook mode (preferred)
    if let Some(webhook_url) = &state.config.discord_webhook_url {
        return send_via_webhook(&client, webhook_url, content, username).await;
    }
    
    // Bot mode (fallback)
    if let (Some(token), Some(channel_id)) = (
        &state.config.discord_bot_token,
        &state.config.discord_channel_id,
    ) {
        return send_via_bot(&client, token, channel_id, content).await;
    }
    
    // No configuration available
    Err(anyhow::anyhow!(
        "Discord not configured. Set DISCORD_WEBHOOK_URL or DISCORD_BOT_TOKEN and DISCORD_CHANNEL_ID"
    ))
}

/// Send message using Discord webhook
#[instrument(skip(client), fields(url = %webhook_url))]
async fn send_via_webhook(
    client: &Client,
    webhook_url: &str,
    content: &str,
    username: Option<&str>,
) -> Result<Value> {
    // Validate webhook URL format
    if let Err(e) = Url::parse(webhook_url) {
        return Err(anyhow::anyhow!("Invalid webhook URL: {}", e));
    }

    let body = serde_json::json!({ "content": content, "username": username });
    
    let response = client
        .post(webhook_url)
        .json(&body)
        .send()
        .await
        .context("Failed to send Discord webhook request")?;
    
    let status = response.status();
    debug!(status = %status, "Discord webhook response status");

    // Handle rate limiting
    if status.as_u16() == 429 {
        let retry_after = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(5);
        
        warn!(
            "Discord rate limit hit, retrying after {} seconds",
            retry_after
        );
        tokio::time::sleep(std::time::Duration::from_secs(retry_after)).await;
        // Use Box::pin to avoid recursive async fn
        return Box::pin(send_via_webhook(client, webhook_url, content, username)).await;
    }

    // Webhooks commonly return 204 No Content on success
    if status.as_u16() == 204 {
        info!("Discord message sent successfully (204 No Content)");
        return Ok(serde_json::json!({ "ok": true, "mode": "webhook", "status": 204 }));
    }

    // Handle error responses
    if !status.is_success() {
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "No error details".to_string());
        
        error!(
            "Discord webhook error: {} - {}",
            status, error_text
        );
        return Err(anyhow::anyhow!(
            "Discord webhook error: {} - {}",
            status,
            error_text
        ));
    }

    // Parse JSON response if available, otherwise return success
    match response.json::<Value>().await {
        Ok(json) => {
            debug!("Discord message sent successfully (JSON response)");
            Ok(json)
        }
        Err(_e) => {
            debug!("Discord message sent successfully (no response body)");
            Ok(serde_json::json!({
                "ok": true,
                "mode": "webhook",
                "status": status.as_u16()
            }))
        }
    }
}

/// Send message using Discord bot
#[instrument(skip(client, token), fields(channel_id = %channel_id))]
async fn send_via_bot(
    client: &Client,
    token: &str,
    channel_id: &str,
    content: &str,
) -> Result<Value> {
    let api_url = format!("https://discord.com/api/v10/channels/{}/messages", channel_id);
    
    let response = client
        .post(&api_url)
        .bearer_auth(token)
        .json(&serde_json::json!({ "content": content }))
        .send()
        .await
        .context("Failed to send Discord bot message")?;
    
    let status = response.status();
    debug!(status = %status, "Discord bot API response status");
    
    // Handle rate limiting
    if status.as_u16() == 429 {
        let retry_after = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(5);
        
        warn!(
            "Discord rate limit hit, retrying after {} seconds",
            retry_after
        );
        tokio::time::sleep(std::time::Duration::from_secs(retry_after)).await;
        // Use Box::pin to avoid recursive async fn
        return Box::pin(send_via_bot(client, token, channel_id, content)).await;
    }
    
    // Handle error responses
    if !status.is_success() {
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "No error details".to_string());
        
        error!(
            "Discord bot API error: {} - {}",
            status, error_text
        );
        return Err(anyhow::anyhow!(
            "Discord bot API error: {} - {}",
            status,
            error_text
        ));
    }
    
    // Parse the response
    match response.json::<Value>().await {
        Ok(json) => {
            debug!("Discord message sent successfully via bot");
            Ok(serde_json::json!({
                "ok": true,
                "mode": "bot",
                "data": json
            }))
        }
        Err(e) => {
            error!("Failed to parse Discord bot response: {}", e);
            Err(anyhow::anyhow!("Failed to parse Discord bot response"))
        }
    }
}
