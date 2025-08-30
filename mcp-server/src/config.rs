// src/config.rs

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use tracing::{info, warn};

#[derive(Deserialize)]
struct McpConfig {
    mcpServers: HashMap<String, McpServerConfig>,
}

#[derive(Deserialize)]
struct McpServerConfig {
    env: HashMap<String, String>,
}

// A struct to hold all configuration, loaded once at startup from the .env file.
#[derive(Clone, Debug, Default)]
pub struct Config {
    // Server settings
    pub port: u16,

    /// Blockchain settings for supported EVM-compatible networks
    /// Supported chains:
    /// - Ethereum Mainnet (1)
    /// - Ethereum Sepolia Testnet (11155111)
    /// - zkSync Mainnet (324)
    /// - zkSync Sepolia Testnet (300)
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
    pub discord_webhook_url: Option<String>,
    pub discord_bot_token: Option<String>,
    pub discord_channel_id: Option<String>,
    pub etherscan_api_key: Option<String>,
}

/// Load Etherscan API key from mcp_config.json file
fn load_etherscan_api_key() -> Option<String> {
    // Try to find mcp_config.json or mcp.json in common locations
    let possible_paths = vec![
        // Check in the current directory first
        PathBuf::from("mcp_config.json"),
        PathBuf::from("mcp.json"),
        // Check in the parent directory
        PathBuf::from("..").join("mcp_config.json"),
        PathBuf::from("..").join("mcp.json"),
        // Check in the grandparent directory
        PathBuf::from("..").join("..").join("mcp_config.json"),
        PathBuf::from("..").join("..").join("mcp.json"),
        // Check in the .codeium/windsurf directory
        PathBuf::from("..")
            .join("..")
            .join("..")
            .join(".codeium")
            .join("windsurf")
            .join("mcp_config.json"),
        PathBuf::from("..")
            .join("..")
            .join("..")
            .join(".codeium")
            .join("windsurf")
            .join("mcp.json"),
    ];

    for path in possible_paths {
        if path.exists() {
            match fs::read_to_string(&path) {
                Ok(content) => {
                    match serde_json::from_str::<McpConfig>(&content) {
                        Ok(config) => {
                            // Look for the evm-mcp-release server configuration
                            if let Some(server_config) = config.mcpServers.get("evm-mcp-release") {
                                if let Some(api_key) = server_config.env.get("ETHERSCAN_API_KEY") {
                                    // Skip if it's just a placeholder
                                    if api_key != "ETHERSCAN_API_KEY" && !api_key.is_empty() {
                                        info!("Loaded Etherscan API key from {}", path.display());
                                        return Some(api_key.clone());
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Failed to parse {}: {}", path.display(), e);
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to read {}: {}", path.display(), e);
                }
            }
        }
    }

    // Fallback to environment variable
    env::var("ETHERSCAN_API_KEY").ok()
}

impl Config {
    /// Returns a list of configured chain IDs
    pub fn supported_chains(&self) -> Vec<String> {
        self.chain_rpc_urls.keys().cloned().collect()
    }

    /// Checks if a chain ID is supported
    pub fn is_chain_supported(&self, chain_id: &str) -> bool {
        self.chain_rpc_urls.contains_key(chain_id)
    }

    /// Loads configuration from environment variables.
    pub fn from_env() -> Result<Self> {
        // Load variables from the .env file into the environment
        dotenvy::dotenv().ok();

        // Logger is initialized in main.rs

        // Load chain RPC URLs with fallbacks
        let mut chain_rpc_urls = HashMap::new();

        // Try to load from environment variable first
        if let Ok(rpc_urls_str) = env::var("CHAIN_RPC_URLS") {
            if let Ok(urls) = serde_json::from_str::<HashMap<String, String>>(&rpc_urls_str) {
                chain_rpc_urls = urls;
                info!("Loaded RPC URLs from CHAIN_RPC_URLS environment variable");
            } else {
                warn!("Failed to parse CHAIN_RPC_URLS, using default RPC URLs");
            }
        }

        // Add default RPC URLs if not already set
        let default_urls = vec![
            ("1", "https://eth.llamarpc.com"),        // Ethereum Mainnet
            ("11155111", "https://rpc.sepolia.org"),  // Sepolia Testnet
            ("324", "https://mainnet.era.zksync.io"), // zkSync Mainnet
            ("300", "https://sepolia.era.zksync.io"), // zkSync Sepolia Testnet
        ];

        for (chain_id, url) in default_urls {
            chain_rpc_urls
                .entry(chain_id.to_string())
                .or_insert_with(|| {
                    info!("Using default RPC URL for chain {}: {}", chain_id, url);
                    url.to_string()
                });
        }

        // Default to Ethereum mainnet chain ID (1) if not specified
        let default_chain_id = env::var("DEFAULT_CHAIN_ID")
            .unwrap_or_else(|_| "1".to_string())
            .parse::<u64>()
            .context("DEFAULT_CHAIN_ID must be a valid number")?;

        // Get the master password from environment or use a default (not recommended for production)
        let master_password =
            env::var("MASTER_PASSWORD").unwrap_or_else(|_| "default-insecure-password".to_string());

        // Get wallet storage path from environment or use a default
        let wallet_storage_path = env::var("WALLET_STORAGE_PATH").ok().or_else(|| {
            // Default to a path in the user's home directory
            dirs::home_dir().map(|mut path| {
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

            // External services - load with debug logging
            faucet_api_url: env::var("FAUCET_API_URL").ok().map(|url| {
                info!("Faucet API URL configured");
                url
            }),
            discord_api_url: env::var("DISCORD_API_URL").ok().map(|url| {
                info!("Discord API URL configured");
                url
            }),
            discord_webhook_url: env::var("DISCORD_WEBHOOK_URL").ok().map(|url| {
                info!("Discord webhook URL configured");
                url
            }),
            discord_bot_token: env::var("DISCORD_BOT_TOKEN").ok().map(|token| {
                info!("Discord bot token configured");
                token
            }),
            discord_channel_id: env::var("DISCORD_CHANNEL_ID").ok().map(|id| {
                info!("Discord channel ID configured");
                id
            }),
            etherscan_api_key: load_etherscan_api_key().map(|key| {
                info!("Etherscan API key configured");
                key
            }),
        })
    }
}
