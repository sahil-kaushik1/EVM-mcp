use anyhow::Result;
use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use crate::{
    AppState,
    blockchain::{
        wallet_manager::WalletManager,
        models::WalletResponse,
    },
    mcp::wallet_storage::{self, WalletStorage},
};

// --- Request and Response Models ---

/// Response for wallet creation
#[derive(Debug, Serialize)]
pub struct CreateWalletResponse {
    /// The EVM address of the created wallet (0x-prefixed hex string)
    pub address: String,
    /// The mnemonic phrase (only returned once on creation)
    pub mnemonic: String,
}

/// Response for wallet import
#[derive(Debug, Serialize)]
pub struct ImportWalletResponse {
    /// The wallet name
    pub name: String,
    /// The EVM address of the imported wallet (0x-prefixed hex string)
    pub address: String,
    /// When the wallet was created
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Response for listing wallets
#[derive(Debug, Serialize)]
pub struct ListWalletsResponse {
    /// List of wallet names and their addresses
    pub wallets: Vec<WalletInfo>,
}

/// Wallet information
#[derive(Debug, Serialize)]
pub struct WalletInfo {
    /// User-defined wallet name
    pub name: String,
    /// EVM address (0x-prefixed hex string)
    pub address: String,
    /// When the wallet was created
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Request to import a wallet
#[derive(Debug, Deserialize)]
pub struct ImportWalletRequest {
    /// The wallet name
    pub name: String,
    /// The mnemonic phrase or private key to import
    pub input: String,
    /// Master password for wallet encryption
    pub master_password: String,
}

/// Request to create a wallet
#[derive(Debug, Deserialize)]
pub struct CreateWalletRequest {
    /// Optional wallet name (defaults to the address if not provided)
    pub name: Option<String>,
}

// --- Handlers ---

/// Create a new EVM wallet
#[axum::debug_handler]
pub async fn create_wallet_handler(
    State(state): State<AppState>,
    Json(input): Json<CreateWalletRequest>,
) -> Result<Json<CreateWalletResponse>, (axum::http::StatusCode, String)> {
    info!("Handling wallet creation request");
    
    // Generate a default name if not provided
    let name = input.name.unwrap_or_else(|| "default".to_string());
    
    // Use the master password from config
    let master_password = &state.config.master_password;

    match state.wallet_manager.generate_wallet(&name, master_password).await {
        Ok(wallet) => {
            info!("Successfully created wallet: {}", wallet.address);
            
            // Return the mnemonic only on creation
            let mnemonic = wallet.mnemonic
                .ok_or_else(|| (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to get mnemonic".to_string()
                ))?;
                
            Ok(Json(CreateWalletResponse {
                address: wallet.address,
                mnemonic,
            }))
        }
        Err(e) => {
            error!("Failed to create wallet: {}", e);
            Err((
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to create wallet: {}", e),
            ))
        }
    }
}

/// Import an existing wallet from a mnemonic or private key
pub async fn import_wallet_handler(
    State(state): State<AppState>,
    Json(input): Json<ImportWalletRequest>,
) -> Result<Json<ImportWalletResponse>, (axum::http::StatusCode, String)> {
    info!("Handling wallet import request");

    match state.wallet_manager.import_wallet(
        &input.name,
        &input.input,
        &input.master_password
    ).await {
        Ok(wallet) => {
            info!("Successfully imported wallet: {}", wallet.address);
            Ok(Json(ImportWalletResponse {
                name: wallet.name,
                address: wallet.address,
                created_at: wallet.created_at.unwrap_or_else(chrono::Utc::now),
            }))
        }
        Err(e) => {
            error!("Failed to import wallet: {}", e);
            let status = if e.to_string().contains("already exists") {
                axum::http::StatusCode::CONFLICT
            } else if e.to_string().contains("Invalid") || e.to_string().contains("Failed to parse") {
                axum::http::StatusCode::BAD_REQUEST
            } else {
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            };
            
            Err((status, format!("Failed to import wallet: {}", e)))
        }
    }
}

/// List all wallets in storage
pub async fn list_wallets_handler(
    State(state): State<AppState>,
) -> Result<Json<ListWalletsResponse>, (axum::http::StatusCode, String)> {
    info!("Listing all wallets");

    match state.wallet_manager.list_wallets().await {
        Ok(wallets) => {
            let wallets = wallets
                .into_iter()
                .map(|(name, address, created_at)| WalletInfo { 
                    name, 
                    address, 
                    created_at 
                })
                .collect();
                
            Ok(Json(ListWalletsResponse { wallets }))
        }
        Err(e) => {
            error!("Failed to list wallets: {}", e);
            Err((
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to list wallets".to_string(),
            ))
        }
    }
}

/// Get wallet details by name
#[derive(Debug, Deserialize)]
pub struct GetWalletRequest {
    /// Master password for decryption
    pub master_password: String,
}

/// Get wallet details by name
pub async fn get_wallet_handler(
    State(state): State<AppState>,
    axum::extract::Path(wallet_name): axum::extract::Path<String>,
    Json(input): Json<GetWalletRequest>,
) -> Result<Json<WalletResponse>, (axum::http::StatusCode, String)> {
    info!("Getting wallet details for: {}", wallet_name);

    match state.wallet_manager.get_wallet(&wallet_name, &input.master_password).await {
        Ok(wallet) => {
            info!("Successfully retrieved wallet: {}", wallet_name);
            Ok(Json(wallet))
        }
        Err(e) => {
            error!("Failed to get wallet: {}", e);
            let status = if e.to_string().contains("not found") {
                axum::http::StatusCode::NOT_FOUND
            } else if e.to_string().contains("Invalid password") {
                axum::http::StatusCode::UNAUTHORIZED
            } else {
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            };
            
            Err((status, format!("Failed to get wallet: {}", e)))
        }
    }
}

/// Response for wallet deletion
#[derive(Debug, Serialize)]
pub struct DeleteWalletResponse {
    /// The name of the deleted wallet
    pub name: String,
    /// The address of the deleted wallet
    pub address: String,
    /// When the wallet was created
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Delete a wallet by name
pub async fn delete_wallet_handler(
    State(state): State<AppState>,
    axum::extract::Path(wallet_name): axum::extract::Path<String>,
    Json(input): Json<GetWalletRequest>,
) -> Result<Json<DeleteWalletResponse>, (axum::http::StatusCode, String)> {
    info!("Deleting wallet: {}", wallet_name);

    match state.wallet_manager.remove_wallet(&wallet_name, &input.master_password).await {
        Ok(wallet) => {
            info!("Successfully deleted wallet: {}", wallet_name);
            Ok(Json(DeleteWalletResponse {
                name: wallet.name,
                address: wallet.address,
                created_at: wallet.created_at.unwrap_or_else(chrono::Utc::now),
            }))
        }
        Err(e) => {
            error!("Failed to delete wallet: {}", e);
            let status = if e.to_string().contains("not found") {
                axum::http::StatusCode::NOT_FOUND
            } else if e.to_string().contains("Invalid password") {
                axum::http::StatusCode::UNAUTHORIZED
            } else {
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            };
            
            Err((status, format!("Failed to delete wallet: {}", e)))
        }
    }
}

/// Create the wallet router
pub fn create_wallet_router() -> Router<AppState> {
    Router::new()
        .route("/wallet/create", post(create_wallet_handler))
        .route("/wallet/import", post(import_wallet_handler))
        .route("/wallet/list", get(list_wallets_handler))
        .route("/wallet/:wallet_name", get(get_wallet_handler).delete(delete_wallet_handler))
}