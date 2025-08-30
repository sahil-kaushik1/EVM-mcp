//! Blockchain client module for EVM-compatible networks.
//! 
//! This module provides the main interface for interacting with EVM-compatible
//! blockchains, including wallet management, transaction sending, and contract
//! interaction.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use ethers::{
    providers::{Http, Provider},
    types::TransactionRequest,
};
use serde_json::Value;

pub use super::evm_client::EvmClient;
use crate::blockchain::{
    models::{BalanceResponse, TransactionHistoryResponse, TransactionResponse},
    nonce_manager::NonceManager,
};

/// Main client for interacting with EVM-compatible blockchains.
/// 
/// This is a thin wrapper around the underlying `EvmClient` that provides
/// a more ergonomic interface for the rest of the application.
#[derive(Clone)]
pub struct BlockchainClient {
    /// The underlying EVM client
    evm_client: EvmClient,
    /// Nonce manager for transaction sequencing
    nonce_manager: Arc<tokio::sync::Mutex<NonceManager>>,
}

impl BlockchainClient {
    /// Create a new blockchain client with the given RPC URLs
    pub fn new(rpc_urls: &HashMap<String, String>) -> Result<Self> {
        Ok(Self {
            evm_client: EvmClient::new(rpc_urls),
            nonce_manager: Arc::new(tokio::sync::Mutex::new(NonceManager::new())),
        })
    }

    /// Get the balance of an address in wei
    pub async fn get_balance(&self, chain_id: &str, address: &str) -> Result<BalanceResponse> {
        self.evm_client.get_balance(chain_id, address).await
    }

    /// Create a new wallet
    pub async fn create_wallet(&self) -> Result<crate::blockchain::models::WalletResponse> {
        self.evm_client.create_wallet().await
    }

    /// Import a wallet from private key or mnemonic
    pub async fn import_wallet(&self, input: &str) -> Result<crate::blockchain::models::WalletResponse> {
        self.evm_client.import_wallet(input).await
    }

    /// Get transaction history for an address
    pub async fn get_transaction_history(
        &self,
        chain_id: &str,
        address: &str,
        limit: u64,
    ) -> Result<TransactionHistoryResponse> {
        // Delegate to the underlying EVM client
        self.evm_client.get_transaction_history(chain_id, address, limit).await
    }

    /// Send a raw transaction
    pub async fn send_transaction(
        &self,
        chain_id: &str,
        private_key: &str,
        tx_request: TransactionRequest,
    ) -> Result<TransactionResponse> {
        let mut nonce_manager = self.nonce_manager.lock().await;
        self.evm_client
            .send_transaction(chain_id, private_key, tx_request, &mut nonce_manager)
            .await
    }

    /// Get contract information
    pub async fn get_contract(&self, chain_id: &str, address: &str) -> Result<Value> {
        self.evm_client.get_contract(chain_id, address).await
    }

    /// Get contract bytecode
    pub async fn get_contract_code(&self, chain_id: &str, address: &str) -> Result<Value> {
        self.evm_client.get_contract_code(chain_id, address).await
    }

    /// Get contract transactions
    pub async fn get_contract_transactions(
        &self,
        chain_id: &str,
        address: &str,
    ) -> Result<Value> {
        self.evm_client.get_contract_transactions(chain_id, address).await
    }

    /// Check if an address is a contract
    pub async fn is_contract(&self, chain_id: &str, address: &str) -> Result<bool> {
        self.evm_client.is_contract(chain_id, address).await
    }
}

/// Create a provider for the given RPC URL
pub fn create_provider(rpc_url: &str) -> Result<Provider<Http>> {
    Provider::<Http>::try_from(rpc_url)
        .map_err(|e| anyhow::anyhow!("Failed to create provider: {}", e))
}