// src/blockchain/services/transactions.rs

use crate::blockchain::{models::TransactionResponse, nonce_manager::NonceManager};
use anyhow::{anyhow, Result, Context};
use ethers_core::types::{TransactionRequest, U64, U256};
use ethers_signers::{LocalWallet, Signer};
use reqwest::Client;
use serde_json::json;
use crate::config::Config;

/// A centralized, secure function for sending any EVM transaction.
/// It uses the NonceManager to prevent race conditions.
pub async fn send_evm_transaction(
    rpc_url: &str,
    wallet: LocalWallet,
    tx_request: TransactionRequest,
    nonce_manager: &NonceManager,
) -> Result<TransactionResponse> {
    let client = Client::new();
    let from_address = wallet.address();

    // FIX: Get the next sequential nonce from the manager.
    let nonce = nonce_manager.get_next_nonce(from_address, rpc_url).await?;

    // Get chain ID from the node.
    let chain_id_payload = json!({
        "jsonrpc": "2.0",
        "method": "eth_chainId",
        "params": [],
        "id": 1
    });

    let chain_id_response: serde_json::Value = client.post(rpc_url)
        .json(&chain_id_payload)
        .send().await?.json().await?;
        
    let chain_id_hex = chain_id_response["result"].as_str().context("Failed to get chain_id from RPC")?;
    let chain_id = U64::from_str_radix(chain_id_hex.trim_start_matches("0x"), 16)?;

    // Populate the final transaction request
    let mut tx = tx_request
        .from(from_address)
        .nonce(nonce)
        .chain_id(chain_id.as_u64());

    // If gas is not provided, estimate it via eth_estimateGas
    if tx.gas.is_none() {
        let call_obj = serde_json::to_value(&tx)?;
        let estimate_payload = json!({
            "jsonrpc": "2.0",
            "method": "eth_estimateGas",
            "params": [call_obj],
            "id": 1
        });
        let estimate_resp: serde_json::Value = client.post(rpc_url)
            .json(&estimate_payload)
            .send().await?
            .json().await?;
        if let Some(err) = estimate_resp.get("error") {
            return Err(anyhow!("RPC Error estimating gas: {}", err));
        }
        let gas_hex = estimate_resp["result"].as_str().context("Failed to get gas estimate")?;
        let gas = U256::from_str_radix(gas_hex.trim_start_matches("0x"), 16)?;
        tx = tx.gas(gas);
    }

    // If gas price not provided, fetch eth_gasPrice and use legacy gas_price
    if tx.gas_price.is_none() {
        let gp_payload = json!({
            "jsonrpc": "2.0",
            "method": "eth_gasPrice",
            "params": [],
            "id": 1
        });
        let gp_resp: serde_json::Value = client.post(rpc_url)
            .json(&gp_payload)
            .send().await?
            .json().await?;
        if let Some(err) = gp_resp.get("error") {
            return Err(anyhow!("RPC Error getting gasPrice: {}", err));
        }
        let gp_hex = gp_resp["result"].as_str().context("Failed to get gasPrice")?;
        let gp = U256::from_str_radix(gp_hex.trim_start_matches("0x"), 16)?;
        tx = tx.gas_price(gp);
    }

    // Sign the transaction
    let signature = wallet.sign_transaction(&tx.clone().into()).await?;
    let raw_tx = tx.rlp_signed(&signature);

    // Send the raw transaction
    let params = json!([format!("0x{}", hex::encode(raw_tx))]);
    let payload = json!({
        "jsonrpc": "2.0",
        "method": "eth_sendRawTransaction",
        "params": params,
        "id": 1,
    });

    let response: serde_json::Value = client.post(rpc_url)
        .json(&payload)
        .send().await?.json().await?;

    if let Some(error) = response.get("error") {
        return Err(anyhow!("RPC Error sending transaction: {}", error));
    }

    let tx_hash = response["result"]
        .as_str()
        .ok_or_else(|| anyhow!("Failed to extract transaction hash from response"))?;

    Ok(TransactionResponse {
        tx_hash: tx_hash.to_string(),
    })
}



pub async fn send_transaction(
    config: &Config,  // Configuration containing default values
    _chain_id: &str,  // Currently unused, kept for future use
    recipient_address: &str,
    amount: u64,
    nonce_manager: &crate::blockchain::nonce_manager::NonceManager,
    rpc_url: &str,
) -> Result<String> {
    use ethers_core::types::{Address, TransactionRequest, U256};
    use ethers_signers::LocalWallet;
    use std::str::FromStr;

    let wallet = LocalWallet::from_str(&config.tx_private_key.as_ref().ok_or_else(|| anyhow!("No private key configured"))?)
        .context("Failed to load sender wallet from private key")?;
    let recipient = Address::from_str(recipient_address)
        .context("Invalid recipient EVM address format")?;
    let value = U256::from(amount);
    let gas_limit = U256::from(config.default_gas_limit);
    let gas_price = U256::from(config.default_gas_price);

    let tx_request = TransactionRequest::new()
        .to(recipient)
        .value(value)
        .gas(gas_limit)
        .gas_price(gas_price);

    let tx_response = send_evm_transaction(
        rpc_url,
        wallet,
        tx_request,
        nonce_manager
    ).await?;
    Ok(tx_response.tx_hash)
}