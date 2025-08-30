//! # Blockchain Module
//!
//! This module provides the core blockchain functionality for EVM-compatible networks.
//! It includes clients for interacting with blockchain nodes, wallet management,
//! transaction handling, and various blockchain services.
//!
//! ## Architecture
//!
//! - `client`: Main blockchain client interface
//! - `evm_client`: EVM-specific client implementation
//! - `models`: Data models for blockchain entities
//! - `nonce_manager`: Manages transaction nonces
//! - `services`: Various blockchain services (balance, transactions, etc.)
//! - `wallet_manager`: Wallet creation and management

// Re-export the client module with EVM client
pub mod client;
pub mod evm_client;
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
