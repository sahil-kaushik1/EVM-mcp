use axum::{Json, extract::{State}, http::StatusCode};
use serde::{Deserialize, Serialize};
use anyhow::Result;
use std::str::FromStr;

use crate::{
    AppState,
    blockchain::{
        models::{ChainType, TransactionResponse},
        services::transactions::{send_evm_transaction, send_native_transaction_signed},
    },
};
use ethers_core::types::{Address, TransactionRequest, U256};
use ethers_signers::LocalWallet;
use cosmrs::AccountId as CosmosAccountId;

#[derive(Debug, Deserialize)]
pub struct SendTxRequest {
    pub chain_id: String,
    pub to: String,
    pub amount_usei: u64,

    // Preferred: use a registered wallet
    pub wallet_name: Option<String>,
    pub master_password: Option<String>,

    // Dev mode: send directly with a private key
    pub from_private_key: Option<String>,

    // Optional EVM overrides
    pub gas_limit: Option<u64>,
    pub gas_price: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct SendTxResponse {
    pub tx_hash: String,
}

pub async fn send_transaction_handler(
    State(state): State<AppState>,
    Json(req): Json<SendTxRequest>,
) -> Result<Json<SendTxResponse>, (StatusCode, String)> {
    let chain_type = ChainType::from_chain_id(&req.chain_id);

    // Resolve RPC URL
    let rpc_url = state
        .config
        .chain_rpc_urls
        .get(&req.chain_id)
        .cloned()
        .ok_or_else(|| (StatusCode::BAD_REQUEST, format!("Unknown chain_id: {}", req.chain_id)))?;

    match chain_type {
        ChainType::Evm => {
            // Validate EVM address format early
            if Address::from_str(&req.to).is_err() {
                return Err((StatusCode::BAD_REQUEST, "Invalid EVM address format".to_string()));
            }
            // Resolve signing key
            let wallet: LocalWallet = if let (Some(name), Some(pw)) = (&req.wallet_name, &req.master_password) {
                // Use stored wallet
                let storage = state.wallet_storage.lock().await;
                let pk_hex = storage
                    .get_decrypted_private_key(name, pw)
                    .map_err(|e| (StatusCode::UNAUTHORIZED, format!("Wallet unlock failed: {}", e)))?;
                LocalWallet::from_str(&pk_hex)
                    .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid stored private key: {}", e)))?
            } else if let Some(pk) = &req.from_private_key {
                LocalWallet::from_str(pk)
                    .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid private key: {}", e)))?
            } else {
                return Err((StatusCode::BAD_REQUEST, "Provide wallet_name+master_password or from_private_key".to_string()));
            };

            // Build transaction
            let to_addr = Address::from_str(&req.to)
                .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid EVM to address".to_string()))?;
            let value = U256::from(req.amount_usei);
            let mut tx = TransactionRequest::new().to(to_addr).value(value);
            if let Some(gl) = req.gas_limit { tx = tx.gas(U256::from(gl)); }
            if let Some(gp) = req.gas_price { tx = tx.gas_price(U256::from(gp)); }

            // Send via shared nonce manager
            let resp: TransactionResponse = send_evm_transaction(
                &rpc_url,
                wallet,
                tx,
                &state.nonce_manager,
            ).await.map_err(|e| (StatusCode::BAD_GATEWAY, format!("EVM send failed: {}", e)))?;

            Ok(Json(SendTxResponse { tx_hash: resp.tx_hash }))
        }
        ChainType::Native => {
            // Validate bech32 address with configured HRP
            if let Err(e) = req.to.parse::<CosmosAccountId>() {
                return Err((StatusCode::BAD_REQUEST, format!("Invalid native address: {}", e)));
            }
            // Enforce HRP match
            if let Ok(acc) = req.to.parse::<CosmosAccountId>() {
                if acc.prefix() != state.config.native_bech32_hrp {
                    return Err((StatusCode::BAD_REQUEST, format!(
                        "Address HRP '{}' does not match required '{}'",
                        acc.prefix(), state.config.native_bech32_hrp
                    )));
                }
            }
            // Resolve signing key (stored or direct)
            let from_private_key_hex = if let (Some(name), Some(pw)) = (&req.wallet_name, &req.master_password) {
                let storage = state.wallet_storage.lock().await;
                storage
                    .get_decrypted_private_key(name, pw)
                    .map_err(|e| (StatusCode::UNAUTHORIZED, format!("Wallet unlock failed: {}", e)))?
            } else if let Some(pk) = &req.from_private_key { pk.clone() } else {
                return Err((StatusCode::BAD_REQUEST, "Provide wallet_name+master_password or from_private_key".to_string()));
            };

            let txhash = send_native_transaction_signed(
                &state.config,
                &rpc_url,
                &from_private_key_hex,
                &req.to,
                req.amount_usei,
            ).await.map_err(|e| (StatusCode::BAD_GATEWAY, format!("Native send failed: {}", e)))?;
            Ok(Json(SendTxResponse { tx_hash: txhash }))
        }
    }
}
