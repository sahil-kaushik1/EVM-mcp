// src/api/faucet.rs

use crate::AppState; // FIX: Import AppState
use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct FaucetRequest {
    pub address: String,
    pub chain_id: String, // required
}

/// Axum handler for the faucet request endpoint.
// FIX: It now receives the full AppState.
pub async fn request_faucet(
    State(state): State<AppState>,
    Json(req): Json<FaucetRequest>,
) -> Result<Json<String>, (StatusCode, String)> {
    // Normalize common aliases users might pass
    let mut chain_id = req.chain_id.trim().to_string();
    if chain_id == "sei-testnet" { chain_id = "sei-evm-testnet".to_string(); }
    if chain_id == "sei-mainnet" { chain_id = "sei-evm-mainnet".to_string(); }

    let rpc_url = match state.config.chain_rpc_urls.get(&chain_id) {
        Some(u) => u,
        None => {
            let keys: Vec<String> = state.config.chain_rpc_urls.keys().cloned().collect();
            return Err((
                StatusCode::BAD_REQUEST,
                format!(
                    "RPC URL not configured for chain_id '{}'. Available: {}",
                    chain_id,
                    keys.join(", ")
                ),
            ));
        }
    };

    // Cooldowns and rate limits are enforced by the external faucet API now.

    let tx_hash = crate::blockchain::services::faucet::send_faucet_tokens(
        &state.config,
        &req.address,
        &state.nonce_manager,
        rpc_url,
        &chain_id,
    ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(tx_hash))
}