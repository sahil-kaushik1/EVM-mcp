use anyhow::Result;
use bip39::{Language, Mnemonic};
use ethers_core::{
    k256::ecdsa::SigningKey,
    utils::{
        secret_key_to_address,
        keccak256,
    },
};
use rand::RngCore;
use tracing::info;
use hex;

use crate::blockchain::models::{
    EvmWallet,
    WalletResponse,
    WalletGenerationError,
    ImportWalletError,
};

/// Utility function to normalize Ethereum addresses
fn normalize_address(address: &str) -> String {
    address.trim_start_matches("0x").to_lowercase()
}

/// Wallet manager for EVM-compatible wallets
#[derive(Debug, Clone)]
pub struct EvmWalletManager;

impl EvmWalletManager {
    /// Create a new EVM wallet manager
    pub fn new() -> Self {
        Self {}
    }

    /// Generate a new EVM wallet with a mnemonic phrase
    pub fn generate_wallet(&self) -> Result<WalletResponse, WalletGenerationError> {
        info!("Generating new EVM wallet");
        
        // Generate a new mnemonic phrase (12 words)
        let mut entropy = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut entropy);
        let mnemonic = Mnemonic::from_entropy_in(Language::English, &entropy)
            .map_err(|e| WalletGenerationError::KeyGenerationFailed(e.to_string()))?;
        let phrase = mnemonic.to_string();
        
        // Derive the private key from the mnemonic (using the first account)
        let seed = mnemonic.to_seed("");
        let private_key = self.derive_private_key(&seed[..32])?;
        
        // Derive the address from the private key
        let address = self.private_key_to_address(&private_key)?;
        let wallet_name = format!("wallet_{}", &normalize_address(&address)[..8]);
        
        // Create the wallet response
        Ok(WalletResponse {
            name: wallet_name,
            address,
            private_key: format!("0x{}", hex::encode(private_key)),
            mnemonic: Some(phrase),
            created_at: Some(chrono::Utc::now()),
        })
    }

    /// Import a wallet from a mnemonic phrase or private key
    pub fn import_wallet(&self, input: &str) -> Result<WalletResponse, ImportWalletError> {
        info!("Attempting to import wallet");
        
        // Try to parse as private key first (0x-prefixed hex string)
        if input.starts_with("0x") && input.len() == 66 {
            if let Ok(wallet) = self.import_private_key(input) {
                return Ok(wallet);
            }
        }
        
        // Then try as mnemonic (space-separated words)
        if input.split_whitespace().count() >= 12 {
            if let Ok(wallet) = self.import_mnemonic(input) {
                return Ok(wallet);
            }
        }
        
        Err(ImportWalletError::InvalidInput(
            "Input must be a valid private key (0x-prefixed 64-char hex) or BIP39 mnemonic phrase".to_string(),
        ))
    }
    
    /// Import a wallet from a private key
    fn import_private_key(&self, private_key_hex: &str) -> Result<WalletResponse, ImportWalletError> {
        // Remove 0x prefix if present
        let private_key_hex = private_key_hex.trim_start_matches("0x");
        
        // Parse the private key
        let private_key_bytes = hex::decode(private_key_hex)
            .map_err(|_| ImportWalletError::InvalidPrivateKey("Invalid hex format".to_string()))?;
            
        if private_key_bytes.len() != 32 {
            return Err(ImportWalletError::InvalidPrivateKey(
                "Private key must be 32 bytes".to_string(),
            ));
        }
        
        // Convert to fixed-size array
        let mut private_key = [0u8; 32];
        private_key.copy_from_slice(&private_key_bytes);
        
        // Derive the address from the private key
        let address = self.private_key_to_address(&private_key)
            .map_err(|e| ImportWalletError::InvalidPrivateKey(e.to_string()))?;
            
        // Generate a default wallet name based on the address
        let wallet_name = format!("wallet_{}", &normalize_address(&address)[..8]);
        
        // Create the wallet response
        Ok(WalletResponse {
            name: wallet_name,
            address,
            private_key: format!("0x{}", hex::encode(private_key)),
            mnemonic: None,
            created_at: Some(chrono::Utc::now()),
        })
    }
    
    /// Import a wallet from a mnemonic phrase
    fn import_mnemonic(&self, mnemonic_phrase: &str) -> Result<WalletResponse, ImportWalletError> {
        // Parse the mnemonic using the new bip39 2.0 API
        let mnemonic = Mnemonic::parse_in_normalized(Language::English, mnemonic_phrase)
            .map_err(|e| ImportWalletError::InvalidMnemonic(format!("Invalid mnemonic phrase: {}", e)))?;
        
        // Derive the private key from the mnemonic (using the first account)
        let seed = mnemonic.to_seed("");
        let private_key = self.derive_private_key(&seed[..32])
            .map_err(|e| ImportWalletError::InvalidMnemonic(e.to_string()))?;
        
        // Derive the address from the private key
        let address = self.private_key_to_address(&private_key)
            .map_err(|e| ImportWalletError::InvalidMnemonic(e.to_string()))?;
        
        // Generate a default wallet name based on the address
        let wallet_name = format!("wallet_{}", &normalize_address(&address)[..8]);
        
        // Create the wallet response
        Ok(WalletResponse {
            name: wallet_name,
            address,
            private_key: format!("0x{}", hex::encode(private_key)),
            mnemonic: Some(mnemonic_phrase.to_string()),
            created_at: Some(chrono::Utc::now()),
        })
    }
    
    /// Derive a private key from seed bytes (first 32 bytes of BIP39 seed)
    fn derive_private_key(&self, seed: &[u8]) -> Result<[u8; 32], WalletGenerationError> {
        if seed.len() < 32 {
            return Err(WalletGenerationError::KeyGenerationFailed(
                "Seed must be at least 32 bytes".to_string(),
            ));
        }
        
        let mut private_key = [0u8; 32];
        private_key.copy_from_slice(&seed[..32]);
        
        // Ensure the private key is valid by trying to create a wallet with it
        ethers::signers::Wallet::from_bytes(&private_key)
            .map_err(|e| WalletGenerationError::KeyGenerationFailed(e.to_string()))?;
            
        Ok(private_key)
    }
    
    /// Convert a private key to an Ethereum address
    fn private_key_to_address(&self, private_key: &[u8; 32]) -> Result<String, WalletGenerationError> {
        // Convert the private key to a SigningKey
        let signing_key = SigningKey::from_bytes(private_key.into())
            .map_err(|e| WalletGenerationError::KeyGenerationFailed(e.to_string()))?;
        
        // Derive the public key and then the address
        let public_key = signing_key.verifying_key();
        let public_key = public_key.to_encoded_point(false);
        let public_key = public_key.as_bytes();
        
        // Take the last 20 bytes of the keccak256 hash of the public key
        let hash = keccak256(&public_key[1..]);
        let address = format!("0x{}", hex::encode(&hash[12..]));
        
        Ok(address)
    }
    
    /// Validate an EVM address format
    fn validate_address(&self, address: &str) -> bool {
        if !address.starts_with("0x") || address.len() != 42 {
            return false;
        }
        // Check if it's a valid hex string
        hex::decode(&address[2..]).is_ok()
    }
    
    /// Get wallet information without requiring external APIs
    pub fn get_wallet_info(&self, address: &str) -> Result<WalletResponse, WalletGenerationError> {
        if !self.validate_address(address) {
            return Err(WalletGenerationError::InvalidAddress(
                "Invalid Ethereum address format".to_string(),
            ));
        }
        
        // Create a basic wallet response with just the address
        Ok(WalletResponse {
            name: format!("wallet_{}", &normalize_address(address)[..8]),
            address: address.to_string(),
            private_key: String::new(), // Empty since we don't have the private key
            mnemonic: None,
            created_at: Some(chrono::Utc::now()),
        })
    }
}

/// Create a new EVM wallet
pub fn create_wallet() -> Result<WalletResponse, WalletGenerationError> {
    let manager = EvmWalletManager::new();
    manager.generate_wallet()
}

/// Import a wallet from mnemonic or private key
pub fn import_wallet(input: &str) -> Result<WalletResponse, ImportWalletError> {
    let manager = EvmWalletManager::new();
    manager.import_wallet(input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bip39::Mnemonic;
    
    #[test]
    fn test_wallet_creation() {
        let manager = EvmWalletManager::new();
        let wallet = manager.generate_wallet().unwrap();
        
        // Check address format
        assert!(wallet.address.starts_with("0x"));
        assert_eq!(wallet.address.len(), 42);
        
        // Check private key format
        assert!(wallet.private_key.starts_with("0x"));
        assert_eq!(wallet.private_key.len(), 66); // 0x + 64 hex chars
        
        // Mnemonic should be present
        assert!(wallet.mnemonic.is_some());
        
        // Validate the address
        assert!(manager.validate_address(&wallet.address));
    }
    
    #[test]
    fn test_wallet_import_private_key() {
        let manager = EvmWalletManager::new();
        let private_key = "0x4f3edf983ac636a65a842ce7c78d9aa706d3b113bce9c46f30d7d21715b23b1d"; // Test key
        
        let wallet = manager.import_private_key(private_key).unwrap();
        
        // Should derive the expected address for this private key
        assert_eq!(wallet.address, "0x90f8bf6a479f320ead074411a4b0e7944ea8c9c1");
        assert_eq!(wallet.private_key, private_key);
    }
    
    #[test]
    fn test_wallet_import_mnemonic() {
        let manager = EvmWalletManager::new();
        let mnemonic = "test test test test test test test test test test test junk";
        
        let wallet = manager.import_mnemonic(mnemonic).unwrap();
        
        // Should derive the expected address for this mnemonic (first account)
        assert_eq!(wallet.address, "0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266");
        
        // The mnemonic should be included in the response
        assert_eq!(wallet.mnemonic, Some(mnemonic.to_string()));
    }
    
    #[test]
    fn test_address_validation() {
        let manager = EvmWalletManager::new();
        
        // Valid addresses
        assert!(manager.validate_address("0x742d35Cc6634C0532925a3b844Bc454e4438f44e"));
        assert!(manager.validate_address("0x0000000000000000000000000000000000000000"));
        
        // Invalid addresses
        assert!(!manager.validate_address(""));
        assert!(!manager.validate_address("0x"));
        assert!(!manager.validate_address("0x123"));
        assert!(!manager.validate_address("0x742d35Cc6634C0532925a3b844Bc454e4438f44z")); // Invalid character 'z'
    }
}
