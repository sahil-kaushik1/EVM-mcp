// src/api/faucet.rs

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, error, info};
use validator::Validate;

use crate::{AppState, blockchain::services::faucet::FaucetError};

/// Request payload for faucet endpoint
#[derive(Debug, Deserialize)]
pub struct FaucetRequest {
    /// The address to send tokens to (0x-prefixed hex string)
    pub address: String,
    
    /// The chain ID to request tokens for
    pub chain_id: String,
}

/// Custom error type for faucet API errors
#[derive(Debug, Error)]
pub enum FaucetApiError {
    #[error("Invalid address format")]
    InvalidAddress,
    #[error("Unsupported chain ID: {0}")]
    UnsupportedChain(String),
    #[error("Faucet service error: {0}")]
    ServiceError(#[from] FaucetError),
    #[error("Internal server error: {0}")]
    InternalError(String),
}

impl From<anyhow::Error> for FaucetApiError {
    fn from(err: anyhow::Error) -> Self {
        FaucetApiError::InternalError(err.to_string())
    }
}

impl FaucetApiError {
    fn status_code(&self) -> StatusCode {
        match self {
            FaucetApiError::InvalidAddress => StatusCode::BAD_REQUEST,
            FaucetApiError::UnsupportedChain(_) => StatusCode::BAD_REQUEST,
            FaucetApiError::ServiceError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            FaucetApiError::InternalError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for FaucetApiError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let error_message = match &self {
            FaucetApiError::InvalidAddress => "Invalid address format".to_string(),
            FaucetApiError::UnsupportedChain(chain_id) => format!("Unsupported chain ID: {}", chain_id),
            FaucetApiError::ServiceError(err) => format!("Faucet service error: {}", err),
            FaucetApiError::InternalError(err) => format!("Internal server error: {}", err),
        };

        let body = serde_json::json!({ "error": error_message });
        (status, Json(body)).into_response()
    }
}

/// Validates the faucet request
fn validate_faucet_request(req: &FaucetRequest) -> Result<(), FaucetApiError> {
    // Validate address format
    if req.address.trim().is_empty() {
        return Err(FaucetApiError::InvalidAddress);
    }

    // Basic address validation (0x prefix + hex chars)
    let addr = req.address.trim_start_matches("0x");
    if addr.is_empty() || addr.len() != 40 || !addr.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(FaucetApiError::InvalidAddress);
    }
    
    // Validate chain ID is not empty
    if req.chain_id.trim().is_empty() {
        return Err(FaucetApiError::UnsupportedChain("Empty chain ID".to_string()));
    }
    
    Ok(())
}

/// Axum handler for the faucet request endpoint.
#[tracing::instrument(skip(state, req), fields(chain_id = %req.chain_id, address = %req.address))]
pub async fn request_faucet(
    State(state): State<AppState>,
    Json(req): Json<FaucetRequest>,
) -> Result<Json<serde_json::Value>, FaucetApiError> {
    // Validate request
    validate_faucet_request(&req)?;

    let chain_id = req.chain_id.trim().to_string();
    let address = req.address.trim().to_string();

    debug!("Processing faucet request for chain: {}", chain_id);

    // Get RPC URL for the chain (for future use, currently passed to service)
    let rpc_url = state
        .config
        .chain_rpc_urls
        .get(&chain_id)
        .map(String::as_str)
        .ok_or_else(|| {
            let available = state.config.chain_rpc_urls.keys().cloned().collect::<Vec<_>>().join(", ");
            FaucetApiError::UnsupportedChain(
                format!("Chain ID '{}' is not supported. Available: {}", chain_id, available)
            )
        })?;

    // Call the faucet service
    let tx_hash = crate::blockchain::services::faucet::send_faucet_tokens(
        &state.config,
        &address,
        &state.nonce_manager,
        rpc_url,
        &chain_id,
    )
    .await
    .map_err(FaucetApiError::from)?;

    info!("Successfully processed faucet request. Tx hash: {}", tx_hash);

    Ok(Json(serde_json::json!({
        "success": true,
        "tx_hash": tx_hash,
        "message": "Faucet request processed successfully"
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        routing::post,
        Router,
    };
    use serde_json::json;
    use std::collections::HashMap;
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_request_faucet_success() {
        let app = Router::new()
            .route("/faucet", post(request_faucet))
            .with_state(create_test_state());

        let req = Request::builder()
            .uri("/faucet")
            .method("POST")
            .header("Content-Type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&json!({ "address": "0x742d35Cc6634C0532925a3b844Bc454e4438f44e", "chain_id": "11155111" })).unwrap(),
            ))
            .unwrap();

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_request_faucet_invalid_address() {
        let app = Router::new()
            .route("/faucet", post(request_faucet))
            .with_state(create_test_state());

        let req = Request::builder()
            .uri("/faucet")
            .method("POST")
            .header("Content-Type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&json!({ "address": "invalid-address", "chain_id": "11155111" })).unwrap(),
            ))
            .unwrap();

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_request_faucet_unsupported_chain() {
        let app = Router::new()
            .route("/faucet", post(request_faucet))
            .with_state(create_test_state());

        let req = Request::builder()
            .uri("/faucet")
            .method("POST")
            .header("Content-Type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&json!({ "address": "0x742d35Cc6634C0532925a3b844Bc454e4438f44e", "chain_id": "9999" })).unwrap(),
            ))
            .unwrap();

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    fn create_test_state() -> AppState {
        use std::path::PathBuf;
        use std::sync::Arc;
        use tokio::sync::Mutex;
        use crate::blockchain::{client::EvmClient, wallet_manager::WalletManager};
        use crate::mcp::wallet_storage::WalletStorage;

        // Create a test config
        let mut config = crate::config::Config::default();
        config.chain_rpc_urls = [
            ("1".to_string(), "https://mainnet.example.com".to_string()),
            ("11155111".to_string(), "https://sepolia.example.com".to_string()),
        ]
        .iter()
        .cloned()
        .collect();
        config.faucet_api_url = Some("http://test-faucet.example.com".to_string());

        // Create a temp dir for wallet storage
        let temp_dir = std::env::temp_dir().join("mcp-test-wallets");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let wallet_storage_path = temp_dir.join("wallets.json");
        
        // Initialize wallet storage
        let wallet_storage = WalletStorage::new(wallet_storage_path.clone());
        
        // Create a new EvmClient with empty config for testing
        let mut rpc_urls = std::collections::HashMap::new();
        rpc_urls.insert("1".to_string(), "https://mainnet.example.com".to_string());
        
        AppState {
            config,
            evm_client: EvmClient::new(&rpc_urls),
            nonce_manager: crate::blockchain::nonce_manager::NonceManager::new(),
            wallet_manager: WalletManager::new(wallet_storage.clone()),
            wallet_storage: Arc::new(Mutex::new(wallet_storage)),
            wallet_storage_path: Arc::new(wallet_storage_path),
        }
    }
}