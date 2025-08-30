// src/blockchain/nonce_manager.rs

use dashmap::DashMap;
use ethers_core::types::{Address, U256};
use tokio::sync::Mutex;

// Manages nonces for multiple sender addresses to prevent race conditions.
#[derive(Debug, Clone)]
pub struct NonceManager {
    // Each address gets its own state, protected by a Mutex.
    // The DashMap allows for concurrent access to different address states.
    nonces: DashMap<Address, Arc<Mutex<NonceState>>>,
}

#[derive(Debug)]
struct NonceState {
    next_nonce: Option<U256>,
}

use std::sync::Arc;
impl NonceManager {
    pub fn new() -> Self {
        Self {
            nonces: DashMap::new(),
        }
    }

    /// Gets the next valid nonce for a given address.
    /// It locks the specific nonce for the address, fetches it from the RPC if not cached,
    /// increments it, and returns it. This ensures sequential nonces for concurrent requests.
    pub async fn get_next_nonce(
        &self,
        address: Address,
        rpc_url: &str, // Pass client/rpc_url to make network calls
    ) -> anyhow::Result<U256> {
        // Find or insert the nonce state for the given address.
        let address_nonce_lock = self
            .nonces
            .entry(address)
            .or_insert_with(|| Arc::new(Mutex::new(NonceState { next_nonce: None })))
            .clone();

        // Lock the mutex specifically for this address.
        let mut state = address_nonce_lock.lock().await;

        let nonce_to_use = match state.next_nonce {
            Some(nonce) => nonce,
            // If we don't have a nonce, fetch the current one from the blockchain.
            None => {
                let client = reqwest::Client::new();
                let payload = serde_json::json!({
                    "jsonrpc": "2.0",
                    "method": "eth_getTransactionCount",
                    "params": [format!("{:?}", address), "latest"],
                    "id": 1
                });

                let resp: serde_json::Value = client.post(rpc_url)
                    .json(&payload)
                    .send()
                    .await?
                    .json()
                    .await?;

                let nonce_hex = resp["result"].as_str().ok_or_else(|| anyhow::anyhow!("Failed to get nonce from RPC response"))?;
                let nonce = U256::from_str_radix(nonce_hex.trim_start_matches("0x"), 16)?;
                nonce
            }
        };

        // Increment the nonce for the *next* transaction and save it.
        state.next_nonce = Some(nonce_to_use + U256::one());

        Ok(nonce_to_use)
    }
}