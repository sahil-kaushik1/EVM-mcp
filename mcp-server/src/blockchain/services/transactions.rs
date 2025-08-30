// src/blockchain/services/transactions.rs

use crate::blockchain::{models::TransactionResponse, nonce_manager::NonceManager};
use anyhow::{anyhow, Result, Context};
use ethers_core::types::{TransactionRequest, U64, U256};
use ethers_signers::{LocalWallet, Signer};
use reqwest::Client;
use serde_json::json;
use crate::config::Config;
use crate::blockchain::models::ChainType;
// Cosmos (native) signing
use cosmrs::crypto::secp256k1::SigningKey as CosmosSigningKey;
use cosmrs::tx::{SignDoc, SignerInfo, AuthInfo, Body, Fee};
use cosmrs::Any;
use cosmrs::proto::cosmos::{
    bank::v1beta1::MsgSend,
    base::v1beta1::Coin,
};
use prost::Message as _;
use base64::engine::general_purpose::STANDARD as BASE64STD;
use base64::Engine;

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

pub async fn send_native_transaction(
    config: &Config,
    recipient_address: &str,
    amount: u64,
    rpc_url: &str,
    _nonce_manager: &crate::blockchain::nonce_manager::NonceManager,
) -> Result<String> {
    // Compose the Cosmos SDK tx message
    let sender_address = &config.default_sender_address; // optional default sender
    let denom = &config.native_denom;
    let msg = serde_json::json!({
        "type": "cosmos-sdk/MsgSend",
        "value": {
            "from_address": sender_address,
            "to_address": recipient_address,
            "amount": [{
                "denom": denom,
                "amount": amount.to_string()
            }]
        }
    });

    // Compose the full tx body (simplified, you may need to add fee, memo, etc.)
    let tx_body = serde_json::json!({
        "msg": [msg],
        "fee": {
            "amount": [{
                "denom": denom,
                "amount": config.native_fee_amount.to_string()
            }],
            "gas": config.native_gas_limit.to_string()
        },
        "signatures": null,
        "memo": ""
    });

    // Broadcast the tx (assumes /txs endpoint, adjust for your node)
    let client = reqwest::Client::new();
    let res = client.post(format!("{}/txs", rpc_url))
        .json(&tx_body)
        .send()
        .await
        .context("Failed to send native SEI tx")?;

    let res_json: serde_json::Value = res.json().await.context("Failed to parse tx response")?;
    if let Some(error) = res_json.get("error") {
        return Err(anyhow!("Native SEI tx error: {}", error));
    }
    let tx_hash = res_json.get("txhash")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("No txhash in native SEI response"))?;
    Ok(tx_hash.to_string())
}

/// Send a native (Cosmos) bank send transaction signed with the provided private key (hex).
pub async fn send_native_transaction_signed(
    config: &Config,
    rpc_url: &str,
    from_private_key_hex: &str,
    to_address: &str,
    amount_usei: u64,
) -> Result<String> {
    // Build signer and derive from address
    let priv_bytes = if let Some(stripped) = from_private_key_hex.strip_prefix("0x") { hex::decode(stripped)? } else { hex::decode(from_private_key_hex)? };
    let signing_key = CosmosSigningKey::from_slice(&priv_bytes)
        .map_err(|e| anyhow!("Invalid Cosmos private key bytes: {}", e))?;
    let public_key = signing_key.public_key();
    let from_account_id = public_key
        .account_id(config.native_bech32_hrp.as_str())
        .map_err(|e| anyhow!("Failed to derive bech32 address from key: {}", e))?;
    let from_address = from_account_id.to_string();

    // Query account number and sequence
    let client = Client::new();
    let acct_res: serde_json::Value = client
        .get(format!("{}/cosmos/auth/v1beta1/accounts/{}", rpc_url, from_address))
        .send().await?
        .json().await?;
    let base_acct = acct_res["account"].clone();
    // handle either base_account nested or direct fields
    let (account_number, sequence) = if base_acct.get("base_account").is_some() {
        let ba = &base_acct["base_account"];
        (
            ba["account_number"].as_str().ok_or_else(|| anyhow!("missing account_number"))?.parse::<u64>()?,
            ba["sequence"].as_str().ok_or_else(|| anyhow!("missing sequence"))?.parse::<u64>()?,
        )
    } else {
        (
            base_acct["account_number"].as_str().ok_or_else(|| anyhow!("missing account_number"))?.parse::<u64>()?,
            base_acct["sequence"].as_str().ok_or_else(|| anyhow!("missing sequence"))?.parse::<u64>()?,
        )
    };

    // Construct MsgSend
    let msg = MsgSend {
        from_address: from_address.clone(),
        to_address: to_address.to_string(),
        amount: vec![Coin { denom: config.native_denom.clone(), amount: amount_usei.to_string() }],
    };
    let any_msg = Any {
        type_url: "/cosmos.bank.v1beta1.MsgSend".to_string(),
        value: msg.encode_to_vec(),
    };

    // Tx body
    let body = Body::new(vec![any_msg], "", 0u32);

    // Fee
    let fee_amount = cosmrs::Coin::new(config.native_fee_amount as u128, &config.native_denom)
        .map_err(|e| anyhow!("invalid fee coin: {}", e))?;
    let fee = Fee::from_amount_and_gas(fee_amount, config.native_gas_limit);

    // Signer info
    let signer_info = SignerInfo::single_direct(Some(public_key), sequence);
    let auth_info = AuthInfo { signer_infos: vec![signer_info], fee };

    // SignDoc
    let sign_doc = SignDoc::new(
        &body,
        &auth_info,
        &config.native_chain_id.parse().context("invalid native chain id")?,
        account_number,
    ).map_err(|e| anyhow!("signdoc error: {}", e))?;
    let tx_raw = sign_doc.sign(&signing_key).map_err(|e| anyhow!("sign error: {}", e))?;

    // Broadcast
    let tx_bytes = tx_raw.to_bytes().map_err(|e| anyhow!("encode tx error: {}", e))?;
    let payload = json!({
        "tx_bytes": BASE64STD.encode(tx_bytes),
        "mode": "BROADCAST_MODE_SYNC"
    });
    let resp: serde_json::Value = client
        .post(format!("{}/cosmos/tx/v1beta1/txs", rpc_url))
        .json(&payload)
        .send().await?
        .json().await?;

    if let Some(err) = resp.get("code").and_then(|c| c.as_i64()).filter(|code| *code != 0) {
        return Err(anyhow!("native tx failed with code {}: {}", err, resp));
    }
    let txhash = resp["tx_response"]["txhash"].as_str()
        .or_else(|| resp["txhash"].as_str())
        .ok_or_else(|| anyhow!("missing txhash in response"))?;
    Ok(txhash.to_string())
}

pub async fn send_transaction(
    config: &Config,
    chain_id: &str,
    recipient_address: &str,
    amount: u64,
    nonce_manager: &crate::blockchain::nonce_manager::NonceManager,
    rpc_url: &str,
) -> Result<String> {
    match ChainType::from_chain_id(chain_id) {
        ChainType::Evm => {
            use ethers_core::types::{Address, TransactionRequest, U256};
            use ethers_signers::LocalWallet;
            use std::str::FromStr;

            let wallet = LocalWallet::from_str(&config.tx_private_key_evm)
                .context("Failed to load sender wallet from private key")?;
            let recipient = Address::from_str(recipient_address)
                .context("Invalid recipient EVM address format")?;
            let value = U256::from(amount);
            let gas_limit = U256::from(config.native_gas_limit);
            let gas_price = U256::from(config.native_fee_amount);

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
        ChainType::Native => {
            send_native_transaction(
                config,
                recipient_address,
                amount,
                rpc_url,
                nonce_manager,
            ).await
        }
    }
}