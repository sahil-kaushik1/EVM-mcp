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
    // FIX: Use the client from the shared state
    match state.sei_client.get_balance(&path.chain_id, &path.address).await {
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