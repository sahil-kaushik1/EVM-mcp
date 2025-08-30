use anyhow::Result;
use reqwest::Client;
use serde_json::Value;
use tracing::{debug, info, warn};

use crate::AppState;

pub async fn send_message(state: &AppState, content: &str, username: Option<&str>) -> Result<Value> {
    let client = Client::new();

    // Prefer webhook if configured
    if let Some(webhook_url) = &state.config.discord_webhook_url {
        let body = serde_json::json!({
            "content": content,
            // username override is only supported in webhook mode
            "username": username,
        });
        let resp = client.post(webhook_url).json(&body).send().await?;
        let status = resp.status();
        debug!(status = %status, mode = "webhook", "Discord webhook response status");

        // Webhooks commonly return 204 No Content on success
        if status.as_u16() == 204 {
            info!(mode = "webhook", "Discord message sent (204 No Content)");
            return Ok(serde_json::json!({"ok": true, "mode": "webhook", "status": 204}));
        }
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Discord webhook error: {} - {}", status, text);
        }
        // Some webhooks may still return JSON; try to parse otherwise normalize
        match resp.json::<Value>().await {
            Ok(json) => {
                info!(mode = "webhook", "Discord message sent (JSON response)");
                Ok(json)
            }
            Err(_) => {
                info!(mode = "webhook", "Discord message sent (normalized ok)");
                Ok(serde_json::json!({"ok": true, "mode": "webhook", "status": status.as_u16()}))
            }
        }
    } else if let (Some(token), Some(channel_id)) = (
        &state.config.discord_bot_token,
        &state.config.discord_channel_id,
    ) {
        // Bot REST send (bot presence may show offline; presence is not required for REST sends)
        let api = format!("https://discord.com/api/v10/channels/{}/messages", channel_id);
        let resp = client
            .post(api)
            .bearer_auth(token)
            .json(&serde_json::json!({ "content": content }))
            .send()
            .await?;
        let status = resp.status();
        debug!(status = %status, mode = "bot", "Discord bot response status");
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Discord API error: {} - {}", status, text);
        }
        let json: Value = resp.json().await.unwrap_or_else(|_| serde_json::json!({"ok": true, "mode": "bot", "status": status.as_u16()}));
        info!(mode = "bot", "Discord message sent");
        Ok(json)
    } else {
        warn!("Discord config missing. Provide DISCORD_WEBHOOK_URL or DISCORD_BOT_TOKEN + DISCORD_CHANNEL_ID");
        anyhow::bail!(
            "Discord not configured. Set DISCORD_WEBHOOK_URL or both DISCORD_BOT_TOKEN and DISCORD_CHANNEL_ID"
        );
    }
}
