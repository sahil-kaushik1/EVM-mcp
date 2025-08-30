use crate::blockchain::models::BalanceResponse;
use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::error;

#[derive(Deserialize)]
struct EtherscanResponse {
    status: String,
    message: String,
    result: String,
}

pub async fn get_balance(
    client: &Client,
    chain_id: &str,
    address: &str,
    etherscan_api_key: &str,
) -> Result<BalanceResponse> {
    // Map chain IDs to Etherscan base URLs
    let base_url = match chain_id {
        "1" => "https://api.etherscan.io/v2/api",
        "11155111" => "https://api-sepolia.etherscan.io/v2/api",
        "324" | "300" => {
            // zkSync chains don't have Etherscan support, return error
            return Err(anyhow!("Etherscan API not supported for zkSync chains"));
        }
        _ => return Err(anyhow!("Unsupported chain ID for Etherscan: {}", chain_id)),
    };

    // Build the Etherscan API URL
    let url = format!(
        "{}?chainid={}&module=account&action=balance&address={}&tag=latest&apikey={}",
        base_url, chain_id, address, etherscan_api_key
    );

    let res: EtherscanResponse = client
        .get(&url)
        .send()
        .await?
        .json()
        .await
        .map_err(|e| anyhow!("Failed to parse Etherscan response: {}", e))?;

    if res.status != "1" {
        return Err(anyhow!(
            "Etherscan API error: {} - {}",
            res.message,
            res.result
        ));
    }

    // Etherscan returns balance in wei as a string
    let amount = res.result.trim().to_string();

    Ok(BalanceResponse {
        amount,
        denom: "wei".to_string(),
    })
}
