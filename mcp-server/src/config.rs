// src/config.rs

use std::collections::HashMap;
use std::env;
use anyhow::{Context, Result};

// A struct to hold all configuration, loaded once at startup from the .env file.
#[derive(Clone, Debug)]
pub struct Config {
    // Server settings
    pub port: u16,
    
    // Blockchain settings
    pub chain_rpc_urls: HashMap<String, String>,
    pub websocket_url: String,
    pub default_chain_id: u64,
    
    // Wallet settings
    pub master_password: String,
    pub wallet_storage_path: Option<String>,
    
    // Transaction settings
    pub default_gas_limit: u64,
    pub default_gas_price: u64,
    pub tx_private_key: Option<String>,
    
    // External services
    pub faucet_api_url: Option<String>,
    pub discord_api_url: Option<String>,
}

impl Config {
    /// Loads configuration from environment variables.
    pub fn from_env() -> Result<Self> {
        // Load variables from the .env file into the environment
        dotenvy::dotenv().ok();

        // Parse CHAIN_RPC_URLS from environment
        let rpc_urls_str = env::var("CHAIN_RPC_URLS")
            .context("CHAIN_RPC_URLS must be set to a JSON map of chain_id -> RPC URL")?;
        let chain_rpc_urls: HashMap<String, String> = serde_json::from_str(&rpc_urls_str)
            .context("Invalid CHAIN_RPC_URLS JSON format")?;

        // Default to Ethereum mainnet chain ID (1) if not specified
        let default_chain_id = env::var("DEFAULT_CHAIN_ID")
            .unwrap_or_else(|_| "1".to_string())
            .parse::<u64>()
            .context("DEFAULT_CHAIN_ID must be a valid number")?;

        // Get the master password from environment or use a default (not recommended for production)
        let master_password = env::var("MASTER_PASSWORD")
            .unwrap_or_else(|_| "default-insecure-password".to_string());
            
        // Get wallet storage path from environment or use a default
        let wallet_storage_path = env::var("WALLET_STORAGE_PATH")
            .ok()
            .or_else(|| {
                // Default to a path in the user's home directory
                dirs::home_dir()
                    .map(|mut path| {
                        path.push(".evm-mcp");
                        path.push("wallets.json");
                        path.to_string_lossy().to_string()
                    })
            });

        Ok(Config {
            // Server settings
            port: env::var("PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()
                .context("PORT must be a valid number")?,
                
            // Blockchain settings
            chain_rpc_urls,
            websocket_url: env::var("WEBSOCKET_URL").unwrap_or_default(),
            default_chain_id,
            
            // Wallet settings
            master_password,
            wallet_storage_path,
            
            // Transaction settings
            default_gas_limit: env::var("DEFAULT_GAS_LIMIT")
                .unwrap_or_else(|_| "300000".to_string())
                .parse()
                .context("DEFAULT_GAS_LIMIT must be a valid number")?,
            default_gas_price: env::var("DEFAULT_GAS_PRICE")
                .unwrap_or_else(|_| "20000000000".to_string())
                .parse()
                .context("DEFAULT_GAS_PRICE must be a valid number")?,
            tx_private_key: env::var("TX_PRIVATE_KEY").ok(),
            
            // External services
            faucet_api_url: env::var("FAUCET_API_URL").ok(),
            discord_api_url: env::var("DISCORD_API_URL").ok(),
        })
    }
}