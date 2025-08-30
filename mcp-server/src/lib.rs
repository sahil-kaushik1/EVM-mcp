#![recursion_limit = "256"]
// src/lib.rs

use std::sync::Arc;
use tokio::sync::Mutex;
use std::path::PathBuf;

// Re-export commonly used types
pub use ethers::types::{Address, H160, H256, U256, U64};
pub use k256::ecdsa::SigningKey;

// Re-export modules
pub mod api;
pub mod blockchain;
pub mod config;
pub mod mcp;
pub mod utils;

/// Application state shared across all request handlers
#[derive(Clone)]
pub struct AppState {
    /// Application configuration
    pub config: config::Config,
    /// EVM client for interacting with blockchain nodes
    pub evm_client: blockchain::client::EvmClient,
    /// Manages transaction nonces
    pub nonce_manager: blockchain::nonce_manager::NonceManager,
    /// Manages wallet operations
    pub wallet_manager: blockchain::wallet_manager::WalletManager,
    /// Secure storage for wallet data
    pub wallet_storage: Arc<Mutex<mcp::wallet_storage::WalletStorage>>,
    /// Path to the wallet storage file
    pub wallet_storage_path: Arc<PathBuf>,
}

pub mod api;
pub mod blockchain;
pub mod config;
pub mod mcp;