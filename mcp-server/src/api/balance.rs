use crate::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use tracing::error;

// Defines the structure for the address and chain_id extracted from the URL path.
#[derive(Debug, Deserialize)]
pub struct BalancePath {
    pub chain_id: String,
    pub address: String,
}

// Defines the structure for the JSON output returned by our API.
#[derive(Debug, Serialize)]
pub struct BalanceOutput {
    pub chain_id: String,
    pub address: String,
    pub balance: String,
    pub denom: String,
}

// The handler function for the GET /balance/{chain_id}/{address} endpoint.
pub async fn get_balance_handler(
    Path(path): Path<BalancePath>,
    State(state): State<AppState>, // FIX: Use AppState
) -> impl IntoResponse {
    // Check if Etherscan API key is configured
    let etherscan_api_key = match state.config.etherscan_api_key.as_ref() {
        Some(key) => key,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "ETHERSCAN_API_KEY is not configured",
            )
                .into_response();
        }
    };

    // Use Etherscan API directly
    let client = reqwest::Client::new();
    match crate::blockchain::services::balance::get_balance(
        &client,
        &path.chain_id,
        &path.address,
        etherscan_api_key,
    )
    .await
    {
        Ok(balance_response) => {
            let output = BalanceOutput {
                chain_id: path.chain_id.clone(),
                address: path.address.clone(),
                balance: balance_response.amount,
                denom: balance_response.denom,
            };
            (StatusCode::OK, Json(output)).into_response()
        }
        Err(e) => {
            error!("Failed to get balance for {}: {:?}", path.address, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to fetch balance: {}", e),
            )
                .into_response()
        }
    }
}
