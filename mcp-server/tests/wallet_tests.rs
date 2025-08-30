//! Tests for wallet management functionality

use axum::{
    body::to_bytes,
    http::{Request, StatusCode, Method},
    Router,
    routing::{get, post},
    extract::State,
};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tempfile::tempdir;
use tokio::sync::Mutex;

use evm_mcp_server::{
    api::wallet::{
        create_wallet_handler,
        import_wallet_handler,
        list_wallets_handler,
        get_wallet_handler,
        delete_wallet_handler,
    },
    blockchain::{
        wallet_manager::WalletManager,
        client::EvmClient,
        nonce_manager::NonceManager,
    },
    config::Config,
    mcp::wallet_storage::WalletStorage,
    AppState,
};

#[derive(serde::Deserialize, Debug)]
struct CreateWalletResponse {
    address: String,
    mnemonic: String,
}

async fn create_test_app() -> Router {
    // Create a temporary directory for test storage
    let temp_dir = tempdir().unwrap();
    let wallet_path = temp_dir.path().join("test_wallets.json");
    
    // Initialize wallet storage and manager
    let wallet_storage = WalletStorage::new(wallet_path.clone());
    let wallet_manager = WalletManager::new(wallet_storage.clone());
    
    // Create a mock config with test RPC URLs
    let mut config = Config::default();
    config.chain_rpc_urls = [
        ("1".to_string(), "https://mainnet.example.com".to_string()),
        ("11155111".to_string(), "https://sepolia.example.com".to_string()),
    ]
    .iter()
    .cloned()
    .collect();
    
    // Create mock EVM client
    let evm_client = EvmClient::new(&config.chain_rpc_urls);
    
    // Create app state
    let state = AppState {
        config,
        evm_client,
        nonce_manager: NonceManager::new(),
        wallet_manager: wallet_manager.clone(),
        wallet_storage: Arc::new(tokio::sync::Mutex::new(wallet_storage)),
        wallet_storage_path: Arc::new(wallet_path),
    };
    
    // Create a router with the wallet routes
    Router::new()
        .route("/wallet/create", post(create_wallet_handler))
        .route("/wallet/import", post(import_wallet_handler))
        .route("/wallet/list", get(list_wallets_handler))
        .route(
            "/wallet/:wallet_name", 
            get(get_wallet_handler)
                .delete(delete_wallet_handler)
        )
        .with_state(Arc::new(state))
}

#[tokio::test]
async fn test_wallet_creation() {
    // Initialize test app
    let app = create_test_app().await;

    // Test wallet creation
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/wallet/create")
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_vec(&json!({
                        "name": "test_wallet",
                        "master_password": "test_password"
                    })).unwrap()
                ))
                .unwrap()
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let wallet_response: CreateWalletResponse = serde_json::from_slice(&body).unwrap();
    
    // Verify the response contains a valid address and mnemonic
    assert!(!wallet_response.address.is_empty());
    assert!(!wallet_response.mnemonic.is_empty());
    assert!(wallet_response.address.starts_with("0x"));
}

#[tokio::test]
async fn test_wallet_import() {
    // Initialize test app
    let app = create_test_app().await;

    // First create a wallet to get a mnemonic
    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/wallet/create")
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_vec(&json!({
                        "name": "test_wallet",
                        "master_password": "test_password"
                    })).unwrap()
                ))
                .unwrap()
        )
        .await
        .unwrap();

    assert_eq!(create_response.status(), StatusCode::OK);
    let body = to_bytes(create_response.into_body(), usize::MAX).await.unwrap();
    let wallet_response: CreateWalletResponse = serde_json::from_slice(&body).unwrap();
    
    // Create a new app instance for import test
    let app = create_test_app().await;
    
    // Test importing the same wallet
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/wallet/import")
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_vec(&json!({
                        "name": "imported_wallet",
                        "input": wallet_response.mnemonic,
                        "master_password": "test_password"
                    })).unwrap()
                ))
                .unwrap()
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let import_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    // Verify the imported wallet has the same address as the created one
    assert_eq!(import_response["address"], wallet_response.address);
}

#[tokio::test]
async fn test_list_wallets() {
    // Initialize test app
    let app = create_test_app().await;

    // Create a wallet first
    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/wallet/create")
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_vec(&json!({
                        "name": "test_wallet",
                        "master_password": "test_password"
                    })).unwrap()
                ))
                .unwrap()
        )
        .await
        .unwrap();

    assert_eq!(create_response.status(), StatusCode::OK);
    
    // Now list wallets
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/wallet/list")
                .body(axum::body::Body::empty())
                .unwrap()
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let wallets: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    
    // Verify we have at least one wallet
    assert!(!wallets.is_empty());
}
