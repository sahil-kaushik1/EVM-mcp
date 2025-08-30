// src/blockchain/models.rs
use chrono::{DateTime, Utc};
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

// --- Error types for wallet operations ---

#[derive(Error, Debug)]
pub enum WalletGenerationError {
    #[error("failed to generate mnemonic: {0}")]
    MnemonicError(#[from] bip39::Error),
    #[error("failed to derive wallet from mnemonic: {0}")]
    DerivationError(#[from] anyhow::Error),
    #[error("key generation failed: {0}")]
    KeyGenerationFailed(String),
}

#[derive(Error, Debug)]
pub enum ImportWalletError {
    #[error("invalid mnemonic: {0}")]
    InvalidMnemonic(String),
    #[error("invalid private key: {0}")]
    InvalidPrivateKey(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("wallet generation error: {0}")]
    WalletGenerationError(String),
}

impl From<WalletGenerationError> for ImportWalletError {
    fn from(err: WalletGenerationError) -> Self {
        ImportWalletError::WalletGenerationError(err.to_string())
    }
}

#[derive(Error, Debug)]
pub enum CreateWalletError {
    #[error("failed to generate wallet: {0}")]
    GenerationFailed(String),
    #[error("key derivation failed: {0}")]
    KeyDerivationFailed(String),
}

// --- Wallet Models ---

/// EVM wallet containing an address and associated private key
#[derive(Debug, Clone)]
pub struct EvmWallet {
    /// The EVM address (0x-prefixed hex string)
    pub address: String,
    /// The private key (securely stored in memory)
    pub private_key: secrecy::Secret<[u8; 32]>,
    /// Optional mnemonic phrase for wallet recovery
    pub mnemonic: Option<secrecy::SecretString>,
}

impl EvmWallet {
    /// Create a new wallet from a private key
    pub fn from_private_key(private_key_bytes: &[u8]) -> Result<Self, WalletGenerationError> {
        use ethers_core::{
            k256::ecdsa::SigningKey,
            utils::secret_key_to_address,
        };
        use secrecy::Secret;
        use hex;

        if private_key_bytes.len() != 32 {
            return Err(WalletGenerationError::KeyGenerationFailed(
                "Private key must be 32 bytes".to_string(),
            ));
        }
        
        // Convert the private key to a SigningKey
        let signing_key = SigningKey::from_bytes(private_key_bytes.into())
            .map_err(|e| WalletGenerationError::KeyGenerationFailed(e.to_string()))?;
        
        // Get the address from the signing key
        let address = format!("0x{}", hex::encode(secret_key_to_address(&signing_key)));
        
        // Copy the private key to ensure we have the correct size
        let mut private_key = [0u8; 32];
        private_key.copy_from_slice(private_key_bytes);
        
        Ok(Self {
            address,
            private_key: Secret::new(private_key),
            mnemonic: None,
        })
    }

    /// Create a new wallet response with the EVM address
    pub fn to_wallet_response(&self, name: &str) -> WalletResponse {
        WalletResponse {
            name: name.to_string(),
            address: self.address.clone(),
            private_key: self.private_key_hex(),
            mnemonic: self.mnemonic_string(),
            created_at: Some(chrono::Utc::now()),
        }
    }

    /// Return hex-encoded private key for API response (avoid logging elsewhere)
    pub fn private_key_hex(&self) -> String {
        format!("0x{}", hex::encode(self.private_key.expose_secret()))
    }

    /// Return mnemonic as String for API response if present
    pub fn mnemonic_string(&self) -> Option<String> {
        self.mnemonic.as_ref().map(|m| m.expose_secret().to_string())
    }
}

/// Response for wallet operations
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WalletResponse {
    /// The wallet name
    pub name: String,
    /// The EVM address of the wallet (0x-prefixed hex string)
    pub address: String,
    /// The private key (0x-prefixed hex string, only included in certain responses)
    pub private_key: String,
    /// The mnemonic phrase (only included on wallet creation/import)
    pub mnemonic: Option<String>,
    /// When the wallet was created
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
}

/// Defines the structure for the request to import a wallet.
#[derive(Debug, Deserialize)]
pub struct ImportWalletRequest {
    pub mnemonic_or_private_key: String,
}

// --- Balance Models ---

/// Defines the structure for a balance response from the blockchain client.
#[derive(Debug, Serialize, Deserialize)]
pub struct BalanceResponse {
    pub amount: String,
    pub denom: String,
}

// --- Transaction History Models ---

/// Enum to distinguish between native and token transfers.
#[derive(Debug, Serialize, Deserialize)]
pub enum TransactionType {
    Native,
    ERC20,
}

/// Defines the structure for a single transaction (our internal representation).
#[derive(Debug, Serialize, Deserialize)]
pub struct Transaction {
    pub tx_hash: String,
    pub from_address: String,
    pub to_address: String,
    pub amount: String,
    pub denom: String, // 'usei' for native, token symbol for ERC20
    pub timestamp: String,
    pub transaction_type: TransactionType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract_address: Option<String>,
}

/// Defines the structure for the transaction history response.
#[derive(Debug, Serialize, Deserialize)]
pub struct TransactionHistoryResponse {
    pub transactions: Vec<Transaction>,
}

// --- Transfer Models ---

/// Defines the structure for a token transfer request.
#[derive(Debug, Serialize, Deserialize)]
pub struct TokenTransferRequest {
    pub to_address: String,
    pub contract_address: String,
    pub amount: String,
    pub private_key: String,
}

/// Defines the structure for an NFT transfer request.
#[derive(Debug, Serialize, Deserialize)]
pub struct NftTransferRequest {
    pub to_address: String,
    pub contract_address: String,
    pub token_id: String,
    pub private_key: String,
}

/// Defines the structure for a token approval request.
#[derive(Debug, Serialize, Deserialize)]
pub struct ApproveRequest {
    pub spender_address: String,
    pub contract_address: String,
    pub amount: String,
    pub private_key: String,
}

/// Defines the structure for a transaction response.
#[derive(Debug, Serialize, Deserialize)]
pub struct TransactionResponse {
    pub tx_hash: String,
}

/// Defines the structure for token information response.
#[derive(Debug, Serialize, Deserialize)]
pub struct TokenInfoResponse {
    pub name: String,
    pub symbol: String,
    pub decimals: u64,
    pub contract_address: String,
}

// --- Fee Estimation Models ---

/// Defines the structure for a fee estimation request.
#[derive(Debug, Serialize, Deserialize)]
pub struct EstimateFeesRequest {
    pub from: String,
    pub to: String,
    pub amount: String,
}

/// Defines the structure for a fee estimation response.
#[derive(Debug, Serialize)]
pub struct EstimateFeesResponse {
    pub estimated_gas: String,
    pub gas_price: String,
    pub total_fee: String,
    pub denom: String,
}

/// Represents the query parameters for searching events.
#[derive(Debug, Clone)]
pub struct EventQuery {
    pub contract_address: Option<String>,
    pub event_type: Option<String>,
    pub attribute_key: Option<String>,
    pub attribute_value: Option<String>,
    pub from_block: Option<u64>,
    pub to_block: Option<u64>,
}

/// The response structure for the search_events endpoint.
#[derive(Serialize, Deserialize, Debug)]
pub struct SearchEventsResponse {
    pub txs: Vec<serde_json::Value>,
    pub total_count: u32,
}


// ... (existing structs)

// --- Contract Models ---

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Contract {
    pub hash: String,
    pub balance: String,
    pub name: String,
    pub creator_address: Option<String>,
    pub tx_hash: Option<String>,
    pub compiler_version: String,
    pub evm_version: String,
    pub optimization: bool,
    pub optimization_runs: String,
    pub code_checked_at: Option<String>,
    pub pointer_type: String,
    pub pointee_address: String,
    pub pointer_address: String,
    pub is_base_asset: bool,
    pub is_pointer: bool,
    pub proxy_type: Option<String>,
    pub implementations: Option<Vec<String>>,
    pub partially_verified: bool,
    pub fully_verified: bool,
    pub verified: bool,
    pub token: Option<TokenInfo>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TokenInfo {
    #[serde(rename = "type")]
    pub token_type: String,
    pub token: TokenDetails,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TokenDetails {
    pub hash: String,
    pub name: String,
    pub symbol: String,
    pub decimals: String,
    pub total_supply: String,
    pub id: String,
    pub address: String,
    pub pointer_type: String,
    pub pointee_address: String,
    pub pointer_address: String,
    pub is_base_asset: bool,
    pub is_pointer: bool,
    pub holders: u64,
    pub transfers: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ContractCode {
    pub abi: Vec<Value>,
    pub compiler_settings: Value,
    pub external_libraries: Vec<Value>,
    pub runtime_code: String,
    pub creation_code: String,
    pub sources: Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ContractTransactionsResponse {
    pub items: Vec<ContractTransaction>,
    pub pagination: Pagination,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ContractTransaction {
    pub hash: String,
    pub timestamp: String,
    pub value: String,
    pub fee: String,
    #[serde(rename = "type")]
    pub tx_type: u64,
    pub action_type: String,
    pub gas_price: String,
    pub gas_limit: String,
    pub max_fee_per_gas: String,
    pub max_priority_fee_per_gas: String,
    pub priority_fee: String,
    pub burnt_fees: String,
    pub gas_used_by_transaction: String,
    pub nonce: u64,
    pub status: bool,
    pub failure_reason: Option<String>,
    pub height: u64,
    pub to: String,
    pub from: String,
    pub data: String,
    pub method: String,
    pub block_confirmation: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Pagination {
    pub pages: u64,
    pub rows: u64,
    pub curr_page: u64,
    pub next_page: Option<u64>,
}
