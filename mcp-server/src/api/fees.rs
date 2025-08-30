use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use crate::{
    AppState,
    blockchain::{
        client::SeiClient,
        models::EstimateFeesRequest,
    },
};

// --- Request and Response Models ---

/// Defines the structure for the JSON input when estimating fees.
#[derive(Debug, Deserialize)]
pub struct EstimateFeesInput {
    pub from: String,
    pub to: String,
    pub amount: String,
}

/// Defines the structure for the JSON output when estimating fees.
#[derive(Debug, Serialize)]
pub struct EstimateFeesOutput {
    pub estimated_gas: String,
    pub gas_price: String,
    pub total_fee: String,
    pub denom: String,
}

// --- Handler ---

/// Handler for the POST /fees/estimate endpoint.
/// This function estimates the gas fees for a potential transaction.
pub async fn estimate_fees_handler(
    Path(chain_id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<EstimateFeesInput>,
) -> Result<Json<EstimateFeesOutput>, (axum::http::StatusCode, String)> {
    info!(
        "Received request to estimate fees for a transaction on chain '{}'",
        chain_id
    );

    let client = SeiClient::new(&state.config.chain_rpc_urls, &state.config.websocket_url);

    // Create the request model from the input payload.
    let estimate_fees_request = EstimateFeesRequest {
        from: payload.from,
        to: payload.to,
        amount: payload.amount,
    };

    match client
        .estimate_fees(&chain_id, &estimate_fees_request)
        .await
    {
        Ok(fees_response) => {
            let output = EstimateFeesOutput {
                estimated_gas: fees_response.estimated_gas,
                gas_price: fees_response.gas_price,
                total_fee: fees_response.total_fee,
                denom: fees_response.denom,
            };
            Ok(Json(output))
        }
        Err(e) => {
            error!("Failed to estimate fees: {:?}", e);
            Err((
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to fetch balance: {}", e),
            ))
        }
    }
}