//! # API Module
//!
//! This module contains HTTP API handlers for the EVM MCP server.
//! It provides RESTful endpoints for blockchain operations, wallet management,
//! and external service integrations.
//!
//! ## Available Endpoints
//!
//! ### Wallet Management
//! - `POST /wallet/create` - Create a new EVM wallet
//! - `POST /wallet/import` - Import wallet from private key or mnemonic
//! - `GET /wallet/list` - List all stored wallets
//! - `GET /wallet/:name` - Get wallet details
//! - `DELETE /wallet/:name` - Delete a wallet
//!
//! ### Blockchain Operations
//! - `GET /balance/:chain_id/:address` - Get account balance
//! - `GET /history/:chain_id/:address` - Get transaction history
//! - `POST /tx/send` - Send a transaction
//!
//! ### Contract Interaction
//! - `GET /contract/:chain_id/:address` - Get contract info
//! - `GET /contract/:chain_id/:address/code` - Get contract bytecode
//! - `GET /contract/:chain_id/:address/transactions` - Get contract transactions
//! - `GET /contract/:chain_id/:address/is_contract` - Check if address is contract
//!
//! ### External Services
//! - Discord integration endpoints
//! - Faucet services for testnet tokens

pub mod balance;
pub mod contract;
pub mod faucet;
pub mod health;
pub mod history;
pub mod wallet;
pub mod tx;
pub mod discord;
