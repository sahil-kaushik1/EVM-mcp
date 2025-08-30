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
                tracing::warn!("Failed to create provider for chain {} at {}", chain_id, url);
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
    pub async fn get_balance(&self, chain_id: &str, address: &str) -> Result<BalanceResponse> {
        let provider = self.get_provider(chain_id)?;
        let address: Address = address.parse().map_err(|e| anyhow!("Invalid address: {}", e))?;
        
        let balance = provider.get_balance(address, None).await?;
        
        Ok(BalanceResponse {
            amount: balance.to_string(),
            denom: "wei".to_string(),
        })
    }

    /// Create a new wallet
    pub async fn create_wallet(&self) -> Result<WalletResponse> {
        wallet::create_wallet()
    }

    /// Import a wallet from private key or mnemonic
    pub async fn import_wallet(&self, input: &str) -> Result<WalletResponse> {
        wallet::import_wallet(input)
    }

    /// Get transaction history for an address
    pub async fn get_transaction_history(
        &self,
        chain_id: &str,
        address: &str,
        limit: u64,
    ) -> Result<TransactionHistoryResponse> {
        history::get_transaction_history(&self.get_provider(chain_id)?, address, limit).await
    }

    /// Send a raw transaction
    pub async fn send_transaction(
        &self,
        chain_id: &str,
        private_key: &str,
        tx_request: TransactionRequest,
        nonce_manager: &NonceManager,
    ) -> Result<TransactionResponse> {
        transactions::send_evm_transaction(
            &self.get_provider(chain_id)?,
            private_key,
            tx_request,
            nonce_manager,
        )
        .await
    }

    /// Get contract information
    pub async fn get_contract(&self, chain_id: &str, address: &str) -> Result<Value> {
        contract::get_contract(&self.get_provider(chain_id)?, address).await
    }

    /// Get contract bytecode
    pub async fn get_contract_code(&self, chain_id: &str, address: &str) -> Result<Value> {
        contract::get_contract_code(&self.get_provider(chain_id)?, address).await
    }

    /// Get contract transactions
    pub async fn get_contract_transactions(
        &self,
        chain_id: &str,
        address: &str,
    ) -> Result<Value> {
        contract::get_contract_transactions(&self.get_provider(chain_id)?, address).await
    }

    /// Check if an address is a contract
    pub async fn is_contract(&self, chain_id: &str, address: &str) -> Result<bool> {
        contract::is_contract(&self.get_provider(chain_id)?, address).await
    }
}
