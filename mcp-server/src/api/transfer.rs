use crate::{
    AppState,
    blockchain::client::EvmClient,
};
use anyhow::Result;
use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use tracing::error;

#[derive(Debug, Deserialize)]
pub struct TransferRequest {
    pub to_address: String,
    pub amount: String,
    pub private_key: String,
    #[serde(default = "default_gas_limit")]
    pub gas_limit: Option<String>,
    #[serde(default = "default_gas_price")]
    pub gas_price: Option<String>,
}

fn default_gas_limit() -> Option<String> {
    Some("100000".to_string()) // Higher gas limit for SEI transfers
}

fn default_gas_price() -> Option<String> {
    Some("20000000000".to_string()) // 20 Gwei
}

#[derive(Debug, Serialize)]
pub struct TransferResponse {
    pub chain_id: String,
    pub tx_hash: String,
}

pub async fn transfer_evm_handler(
    Path(chain_id): Path<String>,
    State(state): State<AppState>,
    Json(request): Json<TransferRequest>,
) -> Result<Json<TransferResponse>, (axum::http::StatusCode, String)> {
    tracing::info!(
        "Received transfer request to {} on chain {}, amount: {}, gas_limit: {:?}, gas_price: {:?}",
        request.to_address,
        chain_id,
        request.amount,
        request.gas_limit,
        request.gas_price
    );
    let client = EvmClient::new(&state.config.chain_rpc_urls);

    // Create EVM transaction request
    let mut tx_request = ethers_core::types::TransactionRequest::new()
        .to(request.to_address.parse::<ethers_core::types::Address>().map_err(|_| {
            (
                axum::http::StatusCode::BAD_REQUEST,
                "Invalid recipient address".to_string(),
            )
        })?)
        .value(ethers_core::types::U256::from_dec_str(&request.amount).map_err(|_| {
            (
                axum::http::StatusCode::BAD_REQUEST,
                "Invalid amount".to_string(),
            )
        })?);

    // Set gas limit if provided
    if let Some(gas_limit) = &request.gas_limit {
        tx_request = tx_request.gas(ethers_core::types::U256::from_dec_str(gas_limit).map_err(|_| {
            (
                axum::http::StatusCode::BAD_REQUEST,
                "Invalid gas limit".to_string(),
            )
        })?);
    }

    // Set gas price if provided
    if let Some(gas_price) = &request.gas_price {
        tx_request = tx_request.gas_price(ethers_core::types::U256::from_dec_str(gas_price).map_err(|_| {
            (
                axum::http::StatusCode::BAD_REQUEST,
                "Invalid gas price".to_string(),
            )
        })?);
    }

    match client.send_transaction(&chain_id, &request.private_key, tx_request, &state.nonce_manager).await {
        Ok(response) => Ok(Json(TransferResponse {
            chain_id,
            tx_hash: response.tx_hash,
        })),
        Err(e) => {
            error!("Failed to transfer SEI tokens: {:?}", e);
            Err((
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to transfer SEI tokens: {}", e),
            ))
        }
    }
}