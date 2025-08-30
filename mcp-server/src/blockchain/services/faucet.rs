// src/blockchain/services/faucet.rs

use crate::blockchain::models::ChainType;
use crate::config::Config;
use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::json;
use tracing::info;

/// Sends faucet tokens via a standard EVM transaction.
pub async fn send_faucet_tokens(
    config: &Config,
    recipient_address: &str,
    _nonce_manager: &crate::blockchain::nonce_manager::NonceManager,
    _rpc_url: &str,
    chain_id: &str,
) -> Result<String> {
    let chain_type = ChainType::from_chain_id(chain_id);

    // Map ChainType to faucet API chain labels
    let faucet_chain = match chain_type {
        ChainType::Evm => "sei-evm-testnet",
        ChainType::Native => "sei-native-testnet",
    };

    info!("Requesting faucet via API for {} on {}", recipient_address, faucet_chain);

    let client = reqwest::Client::new();
    let url = format!("{}/faucet/request", config.faucet_api_url.trim_end_matches('/'));

    #[derive(Deserialize)]
    struct FaucetResponse {
        #[serde(rename = "txHash")] 
        tx_hash: String,
    }

    let resp = client
        .post(&url)
        .json(&json!({
            "address": recipient_address,
            "chain": faucet_chain,
        }))
        .send()
        .await
        .context("Failed to call faucet API")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Faucet API error: status={} body={}", status, text);
    }

    let parsed: FaucetResponse = resp.json().await.context("Invalid faucet API response")?;
    Ok(parsed.tx_hash)
}