// src/blockchain/mod.rs

// Re-export the client module with EVM client
pub mod client;
pub use client::EvmClient;

// Re-export other modules
pub mod models;
pub mod nonce_manager;
pub mod services;
pub mod wallet_manager;

// Re-export commonly used types
pub use ethers::{
    types::{Address, H256, U256, U64},
    utils::to_checksum,
};
