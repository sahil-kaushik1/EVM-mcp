// src/blockchain/services/faucet.rs

use crate::config::Config;
use anyhow::{Context, Result};
use ethers::core::types::Address;
use serde::Deserialize;
use serde_json::json;
use std::str::FromStr;
use thiserror::Error;
use tracing::{debug, error, info, warn, instrument};

/// Custom error type for faucet operations
#[derive(Debug, Error)]
pub enum FaucetError {
    #[error("Faucet API not configured")]
    NotConfigured,
    #[error("Invalid recipient address: {0}")]
    InvalidAddress(String),
    #[error("Faucet API error: {0}")]
    ApiError(String),
    #[error("Invalid chain ID: {0}")]
    InvalidChainId(String),
    #[error("Rate limited: {0}")]
    RateLimited(String),
}

/// Response from the faucet API
#[derive(Debug, Deserialize)]
struct FaucetResponse {
    #[serde(rename = "txHash")]
    tx_hash: String,
    #[serde(default)]
    message: Option<String>,
}

/// Sends faucet tokens to the specified address.
/// 
/// # Arguments
/// * `config` - Application configuration
/// * `recipient_address` - The address to send tokens to (0x-prefixed hex string)
/// * `_nonce_manager` - Nonce manager for transaction sequencing (currently unused)
/// * `_rpc_url` - RPC URL for the target chain (currently unused)
/// * `chain_id` - The chain ID to request tokens for
/// 
/// # Returns
/// The transaction hash of the faucet transaction if successful
#[instrument(skip(config, _nonce_manager), fields(chain_id = %chain_id, recipient = %recipient_address))]
pub async fn send_faucet_tokens(
    config: &Config,
    recipient_address: &str,
    _nonce_manager: &crate::blockchain::nonce_manager::NonceManager,
    _rpc_url: &str,
    chain_id: &str,
) -> Result<String> {
    // Validate recipient address
    if let Err(e) = Address::from_str(recipient_address.trim_start_matches("0x")) {
        return Err(FaucetError::InvalidAddress(e.to_string()).into());
    }

    // Get faucet API URL
    let faucet_url = config.faucet_api_url.as_deref()
        .ok_or_else(|| {
            error!("Faucet API URL not configured");
            FaucetError::NotConfigured
        })?;

    // Determine chain type (testnet/mainnet)
    let faucet_chain = match chain_id {
        // Sepolia testnet
        "11155111" | "5" | "80001" | "97" | "43113" | "421613" | "420" | "1442" | "59140" | "5001" => "testnet",
        // Mainnet
        "1" | "137" | "56" | "43114" | "10" | "42161" | "250" | "1284" | "100" => "mainnet",
        // Unsupported chain
        _ => {
            let msg = format!("Unsupported chain ID: {}", chain_id);
            error!(msg);
            return Err(FaucetError::InvalidChainId(msg).into());
        }
    };

    info!("Requesting faucet tokens for {} on {} (chain_id: {})", recipient_address, faucet_chain, chain_id);

    let client = reqwest::Client::new();
    let url = format!("{}/faucet/request", faucet_url.trim_end_matches('/'));
    
    debug!("Sending faucet request to: {}", url);

    let response = client
        .post(&url)
        .json(&json!({
            "address": recipient_address,
            "chain": faucet_chain,
            "chain_id": chain_id,
        }))
        .send()
        .await
        .context("Failed to send request to faucet API")?;

    let status = response.status();
    let response_text = response.text().await.unwrap_or_default();

    // Handle rate limiting
    if status.as_u16() == 429 {
        let msg = format!("Rate limited by faucet API: {}", response_text);
        warn!(msg);
        return Err(FaucetError::RateLimited(msg).into());
    }

    // Handle other error statuses
    if !status.is_success() {
        let msg = format!("Faucet API error ({}): {}", status, response_text);
        error!(msg);
        return Err(FaucetError::ApiError(msg).into());
    }

    // Parse the successful response
    match serde_json::from_str::<FaucetResponse>(&response_text) {
        Ok(parsed) => {
            info!("Successfully requested faucet tokens. Tx hash: {}", parsed.tx_hash);
            if let Some(msg) = parsed.message {
                info!("Faucet message: {}", msg);
            }
            Ok(parsed.tx_hash)
        }
        Err(e) => {
            let msg = format!("Failed to parse faucet response: {}", e);
            error!("{} (response: {})", msg, response_text);
            Err(anyhow::anyhow!(msg))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Server;
    use serde_json::json;

    #[tokio::test]
    async fn test_send_faucet_tokens_success() {
        let mut server = Server::new_async().await;
        
        // Mock the faucet API response
        let mock_response = json!({
            "txHash": "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
            "message": "Tokens sent successfully"
        });
        
        let _m = server
            .mock("POST", "/faucet/request")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(mock_response.to_string())
            .create_async()
            .await;
        
        let config = Config {
            faucet_api_url: Some(server.url() + "/faucet/request"),
            ..Default::default()
        };
        
        let nonce_manager = crate::blockchain::nonce_manager::NonceManager::new();
        
        let result = send_faucet_tokens(
            &config,
            "0x742d35Cc6634C0532925a3b844Bc454e4438f44e",
            &nonce_manager,
            "http://localhost:8545",
            "11155111"
        ).await;
        
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef");
    }
    
    #[tokio::test]
    async fn test_send_faucet_tokens_invalid_address() {
        let config = Config::default();
        let nonce_manager = crate::blockchain::nonce_manager::NonceManager::new();
        
        let result = send_faucet_tokens(
            &config,
            "invalid-address",
            &nonce_manager,
            "http://localhost:8545",
            "11155111"
        ).await;
        
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Invalid recipient EVM address format"
        );
    }
    
    #[tokio::test]
    async fn test_send_faucet_tokens_rate_limited() {
        let mut server = Server::new_async().await;
        
        // Mock the faucet API rate limit response
        let mock_response = json!({
            "error": "Rate limit exceeded",
            "message": "Please try again later"
        });
        
        let _m = server
            .mock("POST", "/faucet/request")
            .with_status(429)
            .with_header("content-type", "application/json")
            .with_body(mock_response.to_string())
            .create_async()
            .await;
        
        let config = Config {
            faucet_api_url: Some(server.url() + "/faucet/request"),
            ..Default::default()
        };
        
        let nonce_manager = crate::blockchain::nonce_manager::NonceManager::new();
        
        let result = send_faucet_tokens(
            &config,
            "0x742d35Cc6634C0532925a3b844Bc454e4438f44e",
            &nonce_manager,
            "http://localhost:8545",
            "11155111"
        ).await;
        
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err().downcast_ref::<FaucetError>(),
            Some(FaucetError::RateLimited(_))
        ));
    }
}
