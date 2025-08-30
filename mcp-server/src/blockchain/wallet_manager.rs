//! Wallet manager for handling EVM wallet operations

use anyhow::{anyhow, Result};
use bip39::{Mnemonic, Language};
use ethers::{
    core::k256::ecdsa::SigningKey,
    prelude::*,
    signers::{LocalWallet, Signer},
};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, error};

use crate::{
    blockchain::models::WalletResponse,
    mcp::wallet_storage::{WalletStorage, StoredWallet},
};

/// Manages EVM wallet operations
#[derive(Clone)]
pub struct WalletManager {
    /// Storage for wallet data
    storage: Arc<Mutex<WalletStorage>>,
}

impl WalletManager {
    /// Create a new wallet manager
    pub fn new(storage: WalletStorage) -> Self {
        Self {
            storage: Arc::new(Mutex::new(storage)),
        }
    }

    /// Generate a new EVM wallet with a mnemonic and save it to storage
    pub async fn generate_wallet(&self, name: &str, master_password: &str) -> Result<WalletResponse> {
        // Generate a new mnemonic phrase
        let mut rng = rand::rngs::OsRng;
        let entropy = rand::Rng::gen::<[u8; 16]>(&mut rng);
        let mnemonic = Mnemonic::from_entropy_in(Language::English, &entropy)?;
        let mnemonic_phrase = mnemonic.to_string();

        // Derive wallet from mnemonic
        let wallet = self.derive_wallet_from_mnemonic(&mnemonic_phrase).await?;
        
        // Save to storage
        let mut storage = self.storage.lock().await;
        storage.add_wallet(
            name.to_string(),
            &wallet.private_key,
            wallet.address.clone(),
            master_password,
        )?;
        
        info!("Generated new wallet: {} ({})", name, wallet.address);
        
        Ok(WalletResponse {
            name: name.to_string(),
            address: wallet.address,
            private_key: wallet.private_key,
            mnemonic: Some(mnemonic_phrase),
            created_at: Some(chrono::Utc::now()),
        })
    }

    /// Import a wallet from a mnemonic or private key and save it to storage
    pub async fn import_wallet(
        &self, 
        name: &str, 
        input: &str, 
        master_password: &str
    ) -> Result<WalletResponse> {
        // Try to parse as private key first
        if let Ok(wallet) = self.import_private_key(name, input, master_password).await {
            return Ok(wallet);
        }
        
        // If not a private key, try as mnemonic
        self.import_mnemonic(name, input, master_password).await
    }
    
    /// Import a wallet from a private key and save it to storage
    pub async fn import_private_key(
        &self, 
        name: &str, 
        private_key: &str,
        master_password: &str
    ) -> Result<WalletResponse> {
        // Remove 0x prefix if present
        let private_key = private_key.trim_start_matches("0x");
        
        // Parse the private key
        let wallet = private_key.parse::<LocalWallet>()
            .map_err(|e| anyhow!("Invalid private key: {}", e))?;
        
        // Get the address
        let address = wallet.address();
        let address_str = format!("0x{:x}", address);
        
        // Save to storage
        let mut storage = self.storage.lock().await;
        storage.add_wallet(
            name.to_string(),
            private_key,
            address_str.clone(),
            master_password,
        )?;
        
        info!("Imported wallet from private key: {}", name);
        
        Ok(WalletResponse {
            name: name.to_string(),
            address: address_str,
            private_key: private_key.to_string(),
            mnemonic: None,
            created_at: Some(chrono::Utc::now()),
        })
    }
    
    /// Import a wallet from a mnemonic phrase and save it to storage
    pub async fn import_mnemonic(
        &self, 
        name: &str, 
        mnemonic_phrase: &str,
        master_password: &str
    ) -> Result<WalletResponse> {
        // Derive wallet from mnemonic
        let wallet = self.derive_wallet_from_mnemonic(mnemonic_phrase).await?;
        
        // Save to storage
        let mut storage = self.storage.lock().await;
        storage.add_wallet(
            name.to_string(),
            &wallet.private_key,
            wallet.address.clone(),
            master_password,
        )?;
        
        info!("Imported wallet from mnemonic: {}", name);
        
        Ok(WalletResponse {
            name: name.to_string(),
            address: wallet.address,
            private_key: wallet.private_key,
            created_at: Some(chrono::Utc::now()),
            mnemonic: Some(mnemonic_phrase.to_string()),
        })
    }
    
    /// Derive a wallet from a mnemonic phrase
    async fn derive_wallet_from_mnemonic(&self, mnemonic_phrase: &str) -> Result<WalletResponse> {
        // Parse the mnemonic (stored in _mnemonic to avoid unused variable warning)
        let _mnemonic = Mnemonic::parse_in(Language::English, mnemonic_phrase)
            .map_err(|e| anyhow!("Invalid mnemonic phrase: {}", e))?;
        
        // Derive the wallet using the default Ethereum derivation path
        let path = "m/44'/60'/0'/0/0";
        let wallet = ethers_signers::MnemonicBuilder::<ethers_signers::coins_bip39::English>::default()
            .phrase(mnemonic_phrase)
            .derivation_path(path)?
            .build()
            .map_err(|e| anyhow!("Failed to derive wallet: {}", e))?;
        
        // Get the private key and address
        let private_key = format!("0x{:x}", wallet.signer().to_bytes());
        let address = format!("0x{:x}", wallet.address());
        
        // Note: The name will be set by the caller when saving to storage
        Ok(WalletResponse {
            name: "".to_string(),
            address,
            private_key,
            mnemonic: Some(mnemonic_phrase.to_string()),
            created_at: Some(chrono::Utc::now()),
        })
    }
    
    /// Get a wallet by name
    pub async fn get_wallet(&self, name: &str, master_password: &str) -> Result<WalletResponse> {
        let storage = self.storage.lock().await;
        let wallet = storage.get_wallet(name)
            .ok_or_else(|| anyhow!("Wallet '{}' not found", name))?;
            
        let private_key = storage.get_private_key(name, master_password)?;
        
        Ok(WalletResponse {
            name: name.to_string(),
            address: wallet.public_address.clone(),
            private_key,
            mnemonic: None, // For security, we don't store the mnemonic after initial import
            created_at: Some(wallet.created_at),
        })
    }
    
    /// List all wallets with their names, addresses, and creation timestamps
    pub async fn list_wallets(&self) -> Result<Vec<(String, String, chrono::DateTime<chrono::Utc>)>, anyhow::Error> {
        let storage = self.storage.lock().await;
        let wallets = storage.list_wallets_with_timestamps()?;
        let result = wallets.into_iter()
            .map(|(name, address, created_at)| (name, address, created_at))
            .collect();
        Ok(result)
    }
    
    /// Remove a wallet by name
    pub async fn remove_wallet(&self, name: &str, master_password: &str) -> Result<WalletResponse> {
        // First get the wallet details before removing
        let wallet = self.get_wallet(name, master_password).await?;
        
        // Now remove the wallet
        let mut storage = self.storage.lock().await;
        storage.remove_wallet(name, master_password)?;
        
        // Return the wallet details that were removed
        Ok(wallet)
    }
}
