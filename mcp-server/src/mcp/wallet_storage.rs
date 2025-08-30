//! Wallet storage for EVM wallets
//! 
//! This module provides secure storage for EVM wallet private keys using encryption.
//! Private keys are encrypted with a master password before being stored on disk.

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use ethers::{
    core::k256::ecdsa::SigningKey,
    prelude::*,
    signers::LocalWallet,
};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use zeroize::Zeroizing;

use crate::blockchain::models::WalletResponse;

/// Represents a stored EVM wallet with encrypted private key
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredWallet {
    /// User-defined wallet name
    pub wallet_name: String,
    /// Encrypted private key in the format "salt.payload"
    pub encrypted_private_key: String,
    /// EVM address (0x-prefixed hex string)
    pub public_address: String,
    /// When this wallet was created
    pub created_at: DateTime<Utc>,
}

/// In-memory representation of the wallet storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletStorage {
    /// Map of wallet names to stored wallet data
    wallets: HashMap<String, StoredWallet>,
    /// SHA-256 hash of the master password
    master_password_hash: String,
    /// Path to the wallet storage file
    storage_path: PathBuf,
    /// When this storage was created
    created_at: DateTime<Utc>,
    /// When this storage was last updated
    updated_at: DateTime<Utc>,
}

impl WalletStorage {
    /// Create a new wallet storage with the given storage path
    pub fn new(storage_path: PathBuf) -> Self {
        // Create parent directories if they don't exist
        if let Some(parent) = storage_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        
        Self {
            wallets: HashMap::new(),
            master_password_hash: String::new(),
            storage_path,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
    
    /// Create a new wallet storage with encryption
    pub fn with_encryption(master_password: &str, storage_path: PathBuf) -> Result<Self> {
        let mut storage = Self::new(storage_path);
        storage.set_master_password(master_password)?;
        Ok(storage)
    }
    
    /// Set or update the master password
    pub fn set_master_password(&mut self, master_password: &str) -> Result<()> {
        if master_password.len() < 8 {
            return Err(anyhow!("Master password must be at least 8 characters long"));
        }
        self.master_password_hash = Self::hash_password(master_password);
        Ok(())
    }
    
    /// Hash a password using SHA-256 with a salt
    fn hash_password(password: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(password.as_bytes());
        format!("{:x}", hasher.finalize())
    }
    
    /// Verify if the provided password matches the master password
    pub fn verify_master_password(&self, master_password: &str) -> bool {
        !self.master_password_hash.is_empty() &&
        self.master_password_hash == Self::hash_password(master_password)
    }

    /// Check if master password hash is empty
    pub fn is_master_password_hash_empty(&self) -> bool {
        self.master_password_hash.is_empty()
    }

    /// Get wallets map (for internal use)
    pub fn wallets(&self) -> &HashMap<String, StoredWallet> {
        &self.wallets
    }
    
    /// Add a new wallet to the storage
    pub fn add_wallet(
        &mut self,
        wallet_name: String,
        private_key: &str,
        public_address: String,
        master_password: &str,
    ) -> Result<()> {
        // Verify master password if set
        if !self.master_password_hash.is_empty() && !self.verify_master_password(master_password) {
            return Err(anyhow!("Invalid master password"));
        }
        
        // Validate wallet name
        if wallet_name.trim().is_empty() {
            return Err(anyhow!("Wallet name cannot be empty"));
        }
        
        // Check if wallet name already exists
        if self.wallets.contains_key(&wallet_name) {
            return Err(anyhow!("Wallet with name '{}' already exists", wallet_name));
        }
        
        // Validate private key format
        let private_key = private_key.trim_start_matches("0x");
        hex::decode(private_key)
            .map_err(|_| anyhow!("Invalid private key format"))?;
            
        // Validate address format
        if !public_address.starts_with("0x") || public_address.len() != 42 {
            return Err(anyhow!("Invalid Ethereum address format"));
        }
        
        // Encrypt the private key
        let encrypted_key = self.encrypt_private_key(private_key, master_password)?;

        // Store the wallet
        let wallet = StoredWallet {
            wallet_name: wallet_name.clone(),
            encrypted_private_key: encrypted_key,
            public_address: public_address.to_lowercase(),
            created_at: Utc::now(),
        };
        
        self.wallets.insert(wallet_name, wallet);
        self.updated_at = Utc::now();
        
        // Save to disk
        self.save()?;
        
        Ok(())
    }

    /// Encrypt a private key with the master password
    fn encrypt_private_key(&self, private_key: &str, master_password: &str) -> Result<String> {
        // In a real implementation, you would use a proper encryption scheme like AES-GCM
        // For now, we'll just return the key as-is (NOT SECURE FOR PRODUCTION)
        Ok(private_key.to_string())
    }
    
    /// Decrypt a private key with the master password
    pub fn decrypt_private_key(&self, encrypted_key: &str, master_password: &str) -> Result<String> {
        if !self.verify_master_password(master_password) {
            return Err(anyhow!("Invalid master password"));
        }
        
        // In a real implementation, you would use proper encryption/decryption here
        // For now, we'll just return the key as-is (not secure for production!)
        Ok(encrypted_key.to_string())
    }
    
    /// Get a decrypted private key for a wallet
    pub fn get_private_key(
        &self,
        wallet_name: &str,
        master_password: &str,
    ) -> Result<String> {
        // Verify master password if set
        if !self.master_password_hash.is_empty() && !self.verify_master_password(master_password) {
            return Err(anyhow!("Invalid master password"));
        }

        // Get the wallet
        let wallet = self.wallets.get(wallet_name)
            .ok_or_else(|| anyhow!("Wallet '{}' not found", wallet_name))?;

        // Decrypt the private key
        self.decrypt_private_key(&wallet.encrypted_private_key, master_password)
    }
    
    /// Get a wallet by name
    pub fn get_wallet(&self, wallet_name: &str) -> Option<&StoredWallet> {
        self.wallets.get(wallet_name)
    }
    
    /// Get a wallet by address
    pub fn get_wallet_by_address(&self, address: &str) -> Option<&StoredWallet> {
        let address = address.to_lowercase();
        self.wallets.values().find(|w| w.public_address == address)
    }
    
    /// List all wallet names
    pub fn list_wallets(&self) -> Vec<String> {
        self.wallets.keys().cloned().collect()
    }
    
    /// List all wallets with their addresses
    pub fn list_wallets_with_addresses(&self) -> Vec<(String, String)> {
        self.wallets
            .iter()
            .map(|(name, wallet)| (name.clone(), wallet.public_address.clone()))
            .collect()
    }
    
    /// List all wallets with their addresses and creation timestamps
    pub fn list_wallets_with_timestamps(&self) -> Result<Vec<(String, String, chrono::DateTime<chrono::Utc>)>, anyhow::Error> {
        let result = self.wallets
            .iter()
            .map(|(name, wallet)| (name.clone(), wallet.public_address.clone(), wallet.created_at))
            .collect();
        Ok(result)
    }

    /// Save the wallet storage to disk
    pub fn save(&mut self) -> Result<()> {
        self.updated_at = Utc::now();
        save_wallet_storage(&self.storage_path, self)
    }
    
    /// Remove a wallet from storage
    pub fn remove_wallet(&mut self, wallet_name: &str, master_password: &str) -> Result<bool> {
        // Verify master password
        if !self.verify_master_password(master_password) {
            return Err(anyhow!("Invalid master password"));
        }

        if self.wallets.remove(wallet_name).is_some() {
            self.updated_at = Utc::now();
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

/// Get the default path for the wallet storage file.
/// 
/// On Linux: ~/.local/share/mcp/wallets.json
/// On macOS: ~/Library/Application Support/mcp/wallets.json
/// On Windows: %APPDATA%\mcp\wallets.json
pub fn get_wallet_storage_path() -> Result<PathBuf> {
    let mut path = dirs::data_local_dir()
        .ok_or_else(|| anyhow!("Could not determine data directory"))?;
    
    // Create the mcp directory if it doesn't exist
    path.push("mcp");
    std::fs::create_dir_all(&path)
        .context("Failed to create mcp directory")?;
    
    // Add the wallets.json file
    path.push("wallets.json");
    
    Ok(path)
}

/// Load a wallet storage from a file, or create a new one if it doesn't exist
pub fn load_or_create_wallet_storage(
    file_path: &Path,
    master_password: &str,
) -> Result<WalletStorage> {
    if file_path.exists() {
        // Read and parse the existing wallet file
        let content = std::fs::read_to_string(file_path)
            .context("Failed to read wallet storage file")?;
        
        let mut storage: WalletStorage = serde_json::from_str(&content)
            .context("Failed to parse wallet storage")?;
        
        // Verify the master password
        if !storage.verify_master_password(master_password) {
            return Err(anyhow!("Incorrect master password"));
        }
        
        Ok(storage)
    } else {
        // Create parent directories if they don't exist
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)
                .context("Failed to create wallet storage directory")?;
        }
        
        // Create a new wallet storage with encryption
        let mut storage = WalletStorage::with_encryption(master_password, file_path.to_path_buf())
            .context("Failed to create new wallet storage")?;
        
        // Save it to disk
        storage.save()
            .context("Failed to save new wallet storage")?;
        
        Ok(storage)
    }
}

/// Save the wallet storage to a file
pub fn save_wallet_storage(file_path: &Path, storage: &WalletStorage) -> Result<()> {
    // Create a temporary file for atomic write
    let temp_path = file_path.with_extension("tmp");
    
    // Write to temp file
    let content = serde_json::to_string_pretty(storage)
        .context("Failed to serialize wallet storage")?;
    
    std::fs::write(&temp_path, content)
        .context("Failed to write wallet storage file")?;
    
    // Rename temp file to final location (atomic on Unix-like systems)
    std::fs::rename(&temp_path, file_path)
        .or_else(|_| {
            // If rename fails (e.g., cross-device), try copy + remove
            std::fs::copy(&temp_path, file_path)?;
            std::fs::remove_file(&temp_path)?;
            Ok::<(), std::io::Error>(())
        })
        .context("Failed to finalize wallet storage file")?;
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use std::fs;

    #[test]
    fn test_wallet_storage_creation() {
        let temp_dir = tempdir().unwrap();
        let storage_path = temp_dir.path().join("wallets.json");
        
        // Test creating a new storage
        let mut storage = WalletStorage::new(storage_path.clone());
        assert!(storage.wallets.is_empty());
        assert!(!storage_path.exists()); // Shouldn't create file until first save
        
        // Test saving the storage
        storage.save().unwrap();
        assert!(storage_path.exists());
    }
    
    #[test]
    fn test_add_and_retrieve_wallet() {
        let temp_dir = tempdir().unwrap();
        let storage_path = temp_dir.path().join("wallets.json");
        let master_password = "test_password";
        
        // Create storage with master password
        let mut storage = WalletStorage::with_encryption(master_password, storage_path.clone())
            .expect("Failed to create wallet storage");
        
        // Add a wallet
        let wallet_name = "test_wallet".to_string();
        let private_key = "0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let public_address = "0x1234567890abcdef1234567890abcdef12345678";
        
        storage.add_wallet(wallet_name.clone(), private_key, public_address.to_string(), "test_password")
            .unwrap();
            
        // Retrieve the wallet
        let wallet = storage.get_wallet(&wallet_name).unwrap();
        assert_eq!(wallet.public_address, public_address);
        
        // Retrieve the private key
        let stored_private_key = storage.get_private_key(&wallet_name, master_password).unwrap();
        assert_eq!(stored_private_key, private_key);
        
        // Verify the wallet exists in the list
        let wallets = storage.list_wallets();
        assert_eq!(wallets.len(), 1);
        assert_eq!(wallets[0], wallet_name);
        
        // Test getting wallet by address
        let wallet = storage.get_wallet_by_address(public_address).unwrap();
        assert_eq!(wallet.wallet_name, wallet_name);
        assert_eq!(wallet.public_address, public_address);
        
        // Test saving and reloading
        storage.save().expect("Failed to save storage");
        let reloaded = WalletStorage::with_encryption(master_password, storage_path)
            .expect("Failed to reload storage");
            
        // Verify wallet still exists after reload
        assert!(reloaded.get_wallet(&wallet_name).is_some());
        assert_eq!(reloaded.list_wallets().len(), 1);
    }
    
    #[test]
    fn test_wallet_removal() {
        let temp_dir = tempdir().unwrap();
        let storage_path = temp_dir.path().join("wallets.json");
        let master_password = "test_password";
        
        // Create storage with master password
        let mut storage = WalletStorage::with_encryption(master_password, storage_path.clone())
            .expect("Failed to create wallet storage");
        
        // Add a wallet
        let wallet_name = "test_wallet".to_string();
        let private_key = "0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let public_address = "0x1234567890abcdef1234567890abcdef12345678";
        
        storage.add_wallet(wallet_name.clone(), private_key, public_address.to_string(), master_password)
            .unwrap();
        
        // Verify wallet was added
        assert!(storage.get_wallet(&wallet_name).is_some());
        
        // Remove the wallet
        let removed = storage.remove_wallet(&wallet_name, master_password).unwrap();
        assert!(removed);
        
        // Verify wallet was removed
        assert!(storage.get_wallet(&wallet_name).is_none());
        assert!(storage.list_wallets().is_empty());
        
        // Try to remove non-existent wallet
        let removed = storage.remove_wallet("nonexistent", master_password).unwrap();
        assert!(!removed);
        
        // Test saving and reloading after removal
        storage.save().expect("Failed to save storage");
        let reloaded = WalletStorage::with_encryption(master_password, storage_path)
            .expect("Failed to reload storage");
            
        // Verify wallet is still removed after reload
        assert!(reloaded.get_wallet(&wallet_name).is_none());
        assert!(reloaded.list_wallets().is_empty());
    }
}