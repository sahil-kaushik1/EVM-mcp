use crate::{
    AppState,
    blockchain::{client::SeiClient, models::SeiTransferRequest},
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

pub async fn transfer_sei_handler(
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
    let client = SeiClient::new(&state.config.chain_rpc_urls, &state.config.websocket_url);

    let transfer_request = SeiTransferRequest {
        to_address: request.to_address,
        amount: request.amount,
        private_key: request.private_key,
        gas_limit: request.gas_limit,
        gas_price: request.gas_price,
    };

    match client.transfer_sei(&chain_id, &transfer_request).await {
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