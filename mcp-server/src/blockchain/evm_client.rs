// src/blockchain/evm_client.rs

use crate::blockchain::{
    models::*,
    nonce_manager::NonceManager,
    services::{balance, contract, history, transactions, wallet},
};
use anyhow::{anyhow, Result};
use ethers::{
    providers::{Http, Middleware, Provider},
    signers::{LocalWallet, Signer},
    types::{Address, TransactionRequest, H256},
    utils::to_checksum,
};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// Client for interacting with EVM-compatible blockchains
#[derive(Clone)]
pub struct EvmClient {
    providers: HashMap<String, Arc<Provider<Http>>>,
}

impl EvmClient {
    /// Create a new EvmClient with the given RPC URLs
    pub fn new(rpc_urls: &HashMap<String, String>) -> Self {
        let mut providers = HashMap::new();

        for (chain_id, url) in rpc_urls {
            if let Ok(provider) = Provider::<Http>::try_from(url.as_str()) {
                providers.insert(chain_id.clone(), Arc::new(provider));
            } else {
                tracing::warn!(
                    "Failed to create provider for chain {} at {}",
                    chain_id,
                    url
                );
            }
        }

        Self { providers }
    }

    /// Get a provider for the specified chain
    fn get_provider(&self, chain_id: &str) -> Result<Arc<Provider<Http>>> {
        self.providers
            .get(chain_id)
            .cloned()
            .ok_or_else(|| anyhow!("No provider available for chain: {}", chain_id))
    }

    /// Get the balance of an address in wei
    ///
    /// # Note
    /// This method is now deprecated - balance fetching is handled via Etherscan API
    /// in the services layer. This method is kept for backward compatibility.
    pub async fn get_balance(&self, _chain_id: &str, _address: &str) -> Result<BalanceResponse> {
        Err(anyhow!(
            "Balance fetching should use Etherscan API directly"
        ))
    }

    /// Create a new wallet
    pub async fn create_wallet(&self) -> Result<WalletResponse> {
        wallet::create_wallet().map_err(|e| anyhow!("Failed to create wallet: {}", e))
    }

    /// Import a wallet from private key or mnemonic
    pub async fn import_wallet(&self, input: &str) -> Result<WalletResponse> {
        wallet::import_wallet(input).map_err(|e| anyhow!("Failed to import wallet: {}", e))
    }

    /// Get transaction history for an address
    pub async fn get_transaction_history(
        &self,
        _chain_id: &str, // Currently unused, kept for future implementation
        address: &str,
        limit: u64,
    ) -> Result<TransactionHistoryResponse> {
        let client = reqwest::Client::new();
        history::get_transaction_history(&client, address, limit).await
    }

    /// Send a raw transaction
    pub async fn send_transaction(
        &self,
        chain_id: &str,
        private_key: &str,
        tx_request: TransactionRequest,
        nonce_manager: &NonceManager,
    ) -> Result<TransactionResponse> {
        use ethers_signers::LocalWallet;
        use std::str::FromStr;

        let wallet = LocalWallet::from_str(private_key)
            .map_err(|e| anyhow!("Invalid private key: {}", e))?;

        let rpc_url = self
            .providers
            .get(chain_id)
            .ok_or_else(|| anyhow!("No provider available for chain: {}", chain_id))?
            .url()
            .to_string();

        transactions::send_evm_transaction(&rpc_url, wallet, tx_request, nonce_manager).await
    }

    /// Get contract information
    pub async fn get_contract(&self, chain_id: &str, address: &str) -> Result<Value> {
        let rpc_url = self
            .providers
            .get(chain_id)
            .ok_or_else(|| anyhow!("No provider available for chain: {}", chain_id))?
            .url()
            .to_string();
        let client = reqwest::Client::new();
        contract::get_contract(&client, &rpc_url, address).await
    }

    /// Get contract bytecode
    pub async fn get_contract_code(&self, chain_id: &str, address: &str) -> Result<Value> {
        let rpc_url = self
            .providers
            .get(chain_id)
            .ok_or_else(|| anyhow!("No provider available for chain: {}", chain_id))?
            .url()
            .to_string();
        let client = reqwest::Client::new();
        contract::get_contract_code(&client, &rpc_url, address).await
    }

    /// Get contract transactions
    pub async fn get_contract_transactions(&self, chain_id: &str, address: &str) -> Result<Value> {
        let rpc_url = self
            .providers
            .get(chain_id)
            .ok_or_else(|| anyhow!("No provider available for chain: {}", chain_id))?
            .url()
            .to_string();
        let client = reqwest::Client::new();
        contract::get_contract_transactions(&client, &rpc_url, address).await
    }

    /// Check if an address is a contract
    pub async fn is_contract(&self, chain_id: &str, address: &str) -> Result<bool> {
        let rpc_url = self
            .providers
            .get(chain_id)
            .ok_or_else(|| anyhow!("No provider available for chain: {}", chain_id))?
            .url()
            .to_string();
        let client = reqwest::Client::new();
        contract::is_evm_contract(&client, &rpc_url, address).await
    }

    /// Get contract source code from Etherscan
    pub async fn get_contract_source_code(
        &self,
        chain_id: &str,
        address: &str,
        etherscan_api_key: &str,
    ) -> Result<Value> {
        let client = reqwest::Client::new();
        contract::get_contract_source_code(&client, chain_id, address, etherscan_api_key).await
    }
}
