//! # MCP Handler Module
//!
//! This module implements the Model Context Protocol (MCP) for the EVM server.
//! It handles incoming MCP requests and dispatches them to appropriate tools.
//!
//! ## Supported Tools
//!
//! ### Wallet Management
//! - `create_wallet` - Generate a new EVM wallet
//! - `import_wallet` - Import wallet from private key or mnemonic
//! - `register_wallet` - Store wallet securely with encryption
//! - `list_wallets` - List all stored wallets
//! - `transfer_from_wallet` - Send transactions from stored wallets
//!
//! ### Blockchain Operations
//! - `get_balance` - Query account balances
//! - `search_events` - Search for EVM log events
//! - `request_faucet` - Request testnet tokens
//! - `transfer_evm` - Send EVM value transfers
//! - `transfer_nft_evm` - Transfer ERC-721 tokens
//!
//! ### Contract Interaction
//! - `get_contract` - Get contract information
//! - `get_contract_code` - Get contract bytecode
//! - `get_contract_transactions` - Get contract transaction history
//! - `is_contract` - Check if address is a contract
//! - `read_contract` - Read from contract via ABI
//! - `write_contract` - Write to contract via ABI
//!
//! ### Token Operations
//! - `get_token_info` - Get ERC-20 token metadata
//! - `get_token_balance` - Check ERC-20 token balance
//! - `transfer_token` - Transfer ERC-20 tokens
//! - `get_nft_info` - Get ERC-721 token metadata
//! - `check_nft_ownership` - Verify NFT ownership
//! - `transfer_erc1155` - Transfer ERC-1155 tokens
//!
//! ### External Integrations
//! - Discord messaging and health checks

use crate::{
    blockchain::{
        models::WalletResponse,
        services::{transactions, wallet},
    },
    mcp::{
        protocol::{error_codes, Request, Response},
        wallet_storage,
    },
    utils, AppState,
};
use ethers_core::abi::{encode, Token};
use ethers_core::types::{Address, Bytes, TransactionRequest, U256};
use ethers_core::utils::keccak256;
use ethers_signers::{LocalWallet, Signer};
use reqwest::Client;
use serde_json::{json, Value};
use std::str::FromStr;
use tracing::{error, info};

// Normalize common chain_id aliases users might pass via MCP
pub fn normalize_chain_id(input: &str) -> String {
    // Normalize case and separators first
    let mut s = input.trim().to_lowercase();
    // Replace common separators with '-'
    s = s.replace([' ', '_'], "-");
    // Collapse multiple dashes
    while s.contains("--") {
        s = s.replace("--", "-");
    }

    // Common aliases for supported EVM networks
    if s == "mainnet" || s == "main" || s == "m" || s == "eth" || s == "ethereum" {
        return "1".to_string();
    }
    if s == "sepolia" {
        return "11155111".to_string();
    }
    if s == "zksync" || s == "zk" {
        return "324".to_string();
    }
    if s == "zksync-sepolia" || s == "zk-sepolia" || s == "testnet" || s == "test" || s == "t" {
        return "11155111".to_string(); // Default testnet to Sepolia
    }

    s
}

// Use the get_required_arg from utils module

// Heuristic: infer EVM chain from natural language in args if chain_id is absent.
// Looks for words like "mainnet" or "testnet" in common text-bearing fields.
fn infer_evm_chain_from_args(args: &Value) -> Option<String> {
    // common fields where NL may be present
    let candidates = [
        "query",
        "text",
        "prompt",
        "instruction",
        "message",
        "description",
    ];
    let mut blob = String::new();
    for key in candidates.iter() {
        if let Some(s) = args.get(*key).and_then(|v| v.as_str()) {
            blob.push_str(" ");
            blob.push_str(s);
        }
    }
    if blob.is_empty() {
        return None;
    }
    let b = blob.to_lowercase();
    if b.contains("mainnet") {
        return Some("mainnet".to_string());
    }
    if b.contains("testnet") {
        return Some("11155111".to_string());
    }
    None
}

// Helper: produce a result Value that always contains a text content array
// and preserves structured data for JSON-friendly clients.
fn make_texty_result(text: String, payload: Value) -> Value {
    let content = json!([{ "type": "text", "text": text }]);
    match payload {
        Value::Object(mut map) => {
            // Do not overwrite if caller already set content
            if !map.contains_key("content") {
                map.insert("content".into(), content);
            }
            Value::Object(map)
        }
        other => json!({
            "data": other,
            "content": content
        }),
    }
}

/// This is the main dispatcher for all incoming MCP requests.
pub async fn handle_mcp_request(req: Request, state: AppState) -> Option<Response> {
    info!("Handling MCP request for method: {}", req.method);

    if req.is_notification() {
        return None;
    }

    let response = match req.method.as_str() {
        "initialize" => handle_initialize(&req),
        "tools/list" => handle_tools_list(&req),
        "tools/call" => handle_tool_call(req, state).await,
        // Convenience aliases to support direct method calls from CLI
        // They are rewritten into tools/call internally to reuse the same logic
        "get_balance"
        | "request_faucet"
        | "transfer_evm"
        | "transfer_nft_evm"
        | "search_events"
        | "get_contract"
        | "get_contract_code"
        | "get_contract_transactions"
        | "get_transaction_history" => {
            let name = req.method.clone();
            let wrapped = Request {
                jsonrpc: req.jsonrpc.clone(),
                id: req.id.clone(),
                method: "tools/call".to_string(),
                params: Some(json!({
                    "name": name,
                    "arguments": req.params.clone().unwrap_or_else(|| json!({}))
                })),
            };
            handle_tool_call(wrapped, state).await
        }
        _ => Response::error(
            req.id,
            error_codes::METHOD_NOT_FOUND,
            format!("Method not found: {}", req.method),
        ),
    };

    Some(response)
}

/// Handles a 'tools/call' request by dispatching it to the correct tool logic.
async fn handle_tool_call(req: Request, state: AppState) -> Response {
    let params = match req.params.as_ref() {
        Some(p) => p,
        None => {
            return Response::error(
                req.id,
                error_codes::INVALID_PARAMS,
                "Missing 'params' object".into(),
            )
        }
    };

    let tool_name = match params.get("name").and_then(|n| n.as_str()) {
        Some(name) => name,
        None => {
            return Response::error(
                req.id,
                error_codes::INVALID_PARAMS,
                "Missing 'name' field in params".into(),
            )
        }
    };

    let empty_args = json!({});
    let args = params.get("arguments").unwrap_or(&empty_args);
    let req_id = &req.id;

    // FIX: All tool logic is now wrapped in an async block for clean error handling
    // and receives the shared application state.
    match tool_name {
        "discord_post_message" => {
            let res: Result<Response, Response> = (async {
                let base = state.config.discord_api_url.clone().ok_or_else(|| {
                    Response::error(
                        req_id.clone(),
                        error_codes::INVALID_PARAMS,
                        "DISCORD_API_URL is not configured on the server".into(),
                    )
                })?;
                let message = utils::get_required_arg::<String>(args, "message", req_id)?;
                let username = args.get("username").and_then(|v| v.as_str());
                let url = format!("{}/discord/post", base.trim_end_matches('/'));
                let client = Client::new();
                let payload = json!({ "message": message, "username": username });
                let resp = client.post(url).json(&payload).send().await.map_err(|e| {
                    Response::error(req_id.clone(), error_codes::INTERNAL_ERROR, e.to_string())
                })?;
                let status = resp.status();
                let body: Value = resp
                    .json()
                    .await
                    .unwrap_or_else(|_| json!({"ok": status.is_success()}));
                if !status.is_success() {
                    return Err(Response::error(
                        req_id.clone(),
                        error_codes::INTERNAL_ERROR,
                        format!("discord-api error {}: {}", status, body),
                    ));
                }
                let summary = if let Some(u) = username {
                    format!("Posted to Discord as '{}'", u)
                } else {
                    "Posted to Discord".to_string()
                };
                Ok(Response::success(
                    req_id.clone(),
                    make_texty_result(summary, body),
                ))
            })
            .await;
            res.unwrap_or_else(|err_resp| err_resp)
        }
        "get_discord_service_info" => {
            let res: Result<Response, Response> = (async {
                let base = state.config.discord_api_url.clone().ok_or_else(|| {
                    Response::error(
                        req_id.clone(),
                        error_codes::INVALID_PARAMS,
                        "DISCORD_API_URL is not configured on the server".into(),
                    )
                })?;
                let url = format!("{}/", base.trim_end_matches('/'));
                let client = Client::new();
                let resp = client.get(url).send().await.map_err(|e| {
                    Response::error(req_id.clone(), error_codes::INTERNAL_ERROR, e.to_string())
                })?;
                let status = resp.status();
                let body: Value = resp
                    .json()
                    .await
                    .unwrap_or_else(|_| json!({"ok": status.is_success()}));
                if !status.is_success() {
                    return Err(Response::error(
                        req_id.clone(),
                        error_codes::INTERNAL_ERROR,
                        format!("discord-api error {}: {}", status, body),
                    ));
                }
                let has = body
                    .get("hasWebhook")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let summary = format!(
                    "Discord service {} (webhook configured: {})",
                    if status.is_success() {
                        "reachable"
                    } else {
                        "unreachable"
                    },
                    has
                );
                Ok(Response::success(
                    req_id.clone(),
                    make_texty_result(summary, body),
                ))
            })
            .await;
            res.unwrap_or_else(|err_resp| err_resp)
        }
        "check_discord_health" => {
            let res: Result<Response, Response> = (async {
                let base = state.config.discord_api_url.clone().ok_or_else(|| {
                    Response::error(
                        req_id.clone(),
                        error_codes::INVALID_PARAMS,
                        "DISCORD_API_URL is not configured on the server".into(),
                    )
                })?;
                let url = format!("{}/health", base.trim_end_matches('/'));
                let client = Client::new();
                let resp = client.get(url).send().await.map_err(|e| {
                    Response::error(req_id.clone(), error_codes::INTERNAL_ERROR, e.to_string())
                })?;
                let status = resp.status();
                let body: Value = resp
                    .json()
                    .await
                    .unwrap_or_else(|_| json!({"ok": status.is_success()}));
                if !status.is_success() {
                    return Err(Response::error(
                        req_id.clone(),
                        error_codes::INTERNAL_ERROR,
                        format!("discord-api error {}: {}", status, body),
                    ));
                }
                let port = body.get("port").and_then(|v| v.as_u64()).unwrap_or(0);
                let uptime = body.get("uptimeSecs").and_then(|v| v.as_u64()).unwrap_or(0);
                let summary = format!("Discord health OK on port {} (uptime {}s)", port, uptime);
                Ok(Response::success(
                    req_id.clone(),
                    make_texty_result(summary, body),
                ))
            })
            .await;
            res.unwrap_or_else(|err_resp| err_resp)
        }
        "get_balance" => {
            let res: Result<Response, Response> = (async {
                let address = utils::get_required_arg::<String>(args, "address", req_id)?;
                let mut chain_id = utils::get_required_arg::<String>(args, "chain_id", req_id)?;
                chain_id = normalize_chain_id(&chain_id);
                let rpc_url = match state.config.chain_rpc_urls.get(&chain_id) {
                    Some(u) => u,
                    None => {
                        let keys: Vec<String> =
                            state.config.chain_rpc_urls.keys().cloned().collect();
                        return Err(Response::error(
                            req_id.clone(),
                            error_codes::INVALID_PARAMS,
                            format!(
                                "RPC URL not configured for chain_id '{}'. Available: {}",
                                chain_id,
                                keys.join(", ")
                            ),
                        ));
                    }
                };
                let etherscan_api_key = match state.config.etherscan_api_key.as_ref() {
                    Some(key) => key,
                    None => {
                        return Err(Response::error(
                            req_id.clone(),
                            error_codes::INVALID_PARAMS,
                            "ETHERSCAN_API_KEY is not configured".to_string(),
                        ));
                    }
                };

                let client = Client::new();
                let balance = crate::blockchain::services::balance::get_balance(
                    &client,
                    &chain_id,
                    &address,
                    etherscan_api_key,
                )
                .await
                .map_err(|e| {
                    Response::error(req_id.clone(), error_codes::INTERNAL_ERROR, e.to_string())
                })?;
                let debug_info = json!({
                    "chain_id_normalized": chain_id,
                    "rpc_url": rpc_url,
                    "chain_type": "evm"
                });
                let balance_text = match serde_json::to_string(&balance) {
                    Ok(s) => format!("Balance: {}", s),
                    Err(_) => "Balance fetched".to_string(),
                };
                // Return plain JSON so MCP clients can parse result directly
                Ok(Response::success(
                    req_id.clone(),
                    json!({
                        // Plain fields for Windsurf and generic JSON-RPC clients
                        "balance": balance,
                        "debug": debug_info,
                        "message": balance_text,
                        // Text content for clients that expect a content array
                        "content": [
                            { "type": "text", "text": balance_text }
                        ]
                    }),
                ))
            })
            .await;
            res.unwrap_or_else(|err_resp| err_resp)
        }

        "create_wallet" => {
            let res: Result<Response, Response> = (async {
                // EVM-only wallet creation
                let wallet = crate::blockchain::services::wallet::create_wallet().map_err(|e| {
                    Response::error(req_id.clone(), error_codes::INTERNAL_ERROR, e.to_string())
                })?;

                // Create a comprehensive response with all wallet details
                let comprehensive_wallet = json!({
                    "address": wallet.address,
                    "private_key": wallet.private_key,
                    "mnemonic": wallet.mnemonic,
                    "chain_type": "evm",
                });

                let mnemonic_text = wallet
                    .mnemonic
                    .as_ref()
                    .map(|m| format!("\nMnemonic: {}", m))
                    .unwrap_or_else(|| "\nMnemonic: Not available".to_string());

                let summary = format!(
                    "Created EVM wallet with complete details:\nAddress: {}\nPrivate Key: {}{}",
                    wallet.address, wallet.private_key, mnemonic_text
                );
                Ok(Response::success(
                    req_id.clone(),
                    make_texty_result(summary, comprehensive_wallet),
                ))
            })
            .await;
            res.unwrap_or_else(|err_resp| err_resp)
        }

        "import_wallet" => {
            let res: Result<Response, Response> = (async {
                // Accept either 'mnemonic_or_private_key' (preferred) or legacy 'key'
                let key =
                    if let Some(s) = args.get("mnemonic_or_private_key").and_then(|v| v.as_str()) {
                        s.to_string()
                    } else {
                        utils::get_required_arg::<String>(args, "key", req_id)?
                    };
                let wallet =
                    crate::blockchain::services::wallet::import_wallet(&key).map_err(|e| {
                        Response::error(req_id.clone(), error_codes::INTERNAL_ERROR, e.to_string())
                    })?;

                // Create a comprehensive response with all wallet details
                let comprehensive_wallet = json!({
                    "address": wallet.address,
                    "private_key": wallet.private_key,
                    "mnemonic": wallet.mnemonic,
                    "chain_type": "evm",
                });

                let mnemonic_text = wallet
                    .mnemonic
                    .as_ref()
                    .map(|m| format!("\nMnemonic: {}", m))
                    .unwrap_or_else(|| "\nMnemonic: Not available".to_string());

                let summary = format!(
                    "Imported EVM wallet with complete details:\nAddress: {}\nPrivate Key: {}{}",
                    wallet.address, wallet.private_key, mnemonic_text
                );
                Ok(Response::success(
                    req_id.clone(),
                    make_texty_result(summary, comprehensive_wallet),
                ))
            })
            .await;
            res.unwrap_or_else(|err_resp| err_resp)
        }

        "request_faucet" => {
            let res: Result<Response, Response> = (async {
                let address = utils::get_required_arg::<String>(args, "address", req_id)?;
                let mut chain_id = utils::get_required_arg::<String>(args, "chain_id", req_id)?;
                chain_id = normalize_chain_id(&chain_id);
                let rpc_url = match state.config.chain_rpc_urls.get(&chain_id) {
                    Some(u) => u,
                    None => {
                        let keys: Vec<String> =
                            state.config.chain_rpc_urls.keys().cloned().collect();
                        return Err(Response::error(
                            req_id.clone(),
                            error_codes::INVALID_PARAMS,
                            format!(
                                "RPC URL not configured for chain_id '{}'. Available: {}",
                                chain_id,
                                keys.join(", ")
                            ),
                        ));
                    }
                };
                let tx_hash = crate::blockchain::services::faucet::send_faucet_tokens(
                    &state.config,
                    &address,
                    &state.nonce_manager,
                    rpc_url,
                    &chain_id,
                )
                .await
                .map_err(|e| {
                    Response::error(req_id.clone(), error_codes::INTERNAL_ERROR, e.to_string())
                })?;
                let payload = json!({ "transaction_hash": tx_hash });
                let summary = format!("Faucet sent tokens: tx {}", tx_hash);
                Ok(Response::success(
                    req_id.clone(),
                    make_texty_result(summary, payload),
                ))
            })
            .await;
            res.unwrap_or_else(|err_resp| err_resp)
        }

        // --- Event tools ---
        "search_events" => {
            let res: Result<Response, Response> = (async {
                let chain_id = utils::get_required_arg::<String>(args, "chain_id", req_id)?;
                let etherscan_api_key = match state.config.etherscan_api_key.as_ref() {
                    Some(key) => key,
                    None => {
                        return Err(Response::error(
                            req_id.clone(),
                            error_codes::INVALID_PARAMS,
                            "ETHERSCAN_API_KEY is not configured".to_string(),
                        ));
                    }
                };

                // Determine Etherscan base URL based on chain
                let etherscan_base_url = match chain_id.as_str() {
                    "1" => "https://api.etherscan.io/v2/api",
                    "11155111" => "https://api-sepolia.etherscan.io/v2/api",
                    _ => {
                        return Err(Response::error(
                            req_id.clone(),
                            error_codes::INVALID_PARAMS,
                            format!("Etherscan API not supported for chain_id '{}'", chain_id),
                        ));
                    }
                };

                let address = args
                    .get("contract_address")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        Response::error(
                            req_id.clone(),
                            error_codes::INVALID_PARAMS,
                            "Missing 'contract_address'".into(),
                        )
                    })?;

                let from_block = args.get("from_block").and_then(|v| v.as_str());
                let to_block = args.get("to_block").and_then(|v| v.as_str());
                let topic0 = args.get("topic0").and_then(|v| v.as_str());

                // Helper to normalize block tags: accept hex tags (latest/earliest/pending) or decimal block numbers.
                fn normalize_block_tag(tag: &str) -> String {
                    let t = tag.trim();
                    if t == "latest" || t == "earliest" || t == "pending" || t.starts_with("0x") {
                        return t.to_string();
                    }
                    // Try parse as decimal number
                    if let Ok(n) = u64::from_str_radix(t, 10) {
                        return format!("0x{:x}", n);
                    }
                    t.to_string()
                }

                // Build Etherscan API URL
                let mut url = format!(
                    "{}?chainid={}&module=logs&action=getLogs",
                    etherscan_base_url, chain_id
                );

                if let Some(fb) = from_block {
                    url.push_str(&format!("&fromBlock={}", normalize_block_tag(fb)));
                }
                if let Some(tb) = to_block {
                    url.push_str(&format!("&toBlock={}", normalize_block_tag(tb)));
                }
                if let Some(t0) = topic0 {
                    url.push_str(&format!("&topic0={}", t0));
                    // Add topic0_1_opr=and for additional topic filtering if needed
                    url.push_str("&topic0_1_opr=and");
                }

                // Add contract address
                url.push_str(&format!("&address={}", address));

                // Add pagination and API key
                url.push_str("&page=1&offset=1000");
                url.push_str(&format!("&apikey={}", etherscan_api_key));

                let client = Client::new();
                let resp: serde_json::Value = client
                    .get(&url)
                    .send()
                    .await
                    .map_err(|e| {
                        Response::error(
                            req_id.clone(),
                            error_codes::INTERNAL_ERROR,
                            format!("Etherscan API error: {}", e),
                        )
                    })?
                    .json()
                    .await
                    .map_err(|e| {
                        Response::error(
                            req_id.clone(),
                            error_codes::INTERNAL_ERROR,
                            format!("Invalid Etherscan JSON response: {}", e),
                        )
                    })?;

                // Check for Etherscan API errors
                if let Some(status) = resp.get("status").and_then(|v| v.as_str()) {
                    if status != "1" {
                        let message = resp
                            .get("message")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Unknown error");
                        return Err(Response::error(
                            req_id.clone(),
                            error_codes::INTERNAL_ERROR,
                            format!("Etherscan API error: {}", message),
                        ));
                    }
                }

                // Extract logs from result
                let logs = resp
                    .get("result")
                    .cloned()
                    .unwrap_or_else(|| serde_json::Value::Array(vec![]));
                let count = logs.as_array().map(|a| a.len()).unwrap_or(0);
                let payload = json!({ "logs": logs });
                let summary = format!("Found {} log(s) via Etherscan API", count);
                Ok(Response::success(
                    req_id.clone(),
                    make_texty_result(summary, payload),
                ))
            })
            .await;
            res.unwrap_or_else(|err_resp| err_resp)
        }

        // --- Transfers ---
        // EVM value transfer using a provided private key
        "transfer_evm" => {
            let res: Result<Response, Response> = (async {
                let private_key = utils::get_required_arg::<String>(args, "private_key", req_id)?;
                let chain_id = utils::get_required_arg::<String>(args, "chain_id", req_id)?;
                let to_address = utils::get_required_arg::<String>(args, "to_address", req_id)?;
                let amount_wei = utils::get_required_arg::<String>(args, "amount_wei", req_id)?;

                let to = Address::from_str(&to_address).map_err(|_| {
                    Response::error(
                        req_id.clone(),
                        error_codes::INVALID_PARAMS,
                        "Invalid 'to_address'".into(),
                    )
                })?;
                let value = U256::from_dec_str(&amount_wei).map_err(|_| {
                    Response::error(
                        req_id.clone(),
                        error_codes::INVALID_PARAMS,
                        "Invalid 'amount_wei'".into(),
                    )
                })?;

                let mut tx_request = TransactionRequest::new().to(to).value(value);
                if let Some(g) = args.get("gas_limit").and_then(|v| v.as_str()) {
                    tx_request =
                        tx_request.gas(U256::from_dec_str(g).unwrap_or_else(|_| U256::from(0)));
                }
                if let Some(gp) = args.get("gas_price").and_then(|v| v.as_str()) {
                    tx_request = tx_request
                        .gas_price(U256::from_dec_str(gp).unwrap_or_else(|_| U256::from(0)));
                }

                let response = state
                    .evm_client
                    .send_transaction(&chain_id, &private_key, tx_request, &state.nonce_manager)
                    .await
                    .map_err(|e| {
                        Response::error(req_id.clone(), error_codes::INTERNAL_ERROR, e.to_string())
                    })?;
                let summary = match serde_json::to_string(&response) {
                    Ok(s) => format!("EVM tx sent: {}", s),
                    Err(_) => "EVM tx sent".to_string(),
                };
                Ok(Response::success(
                    req_id.clone(),
                    make_texty_result(summary, json!(response)),
                ))
            })
            .await;
            res.unwrap_or_else(|err_resp| err_resp)
        }

        // EVM ERC-721 transfer
        "transfer_nft_evm" => {
            let res: Result<Response, Response> = (async {
                let private_key = utils::get_required_arg::<String>(args, "private_key", req_id)?;
                let chain_id = utils::get_required_arg::<String>(args, "chain_id", req_id)?;
                let contract_address =
                    utils::get_required_arg::<String>(args, "contract_address", req_id)?;
                let to_address = utils::get_required_arg::<String>(args, "to_address", req_id)?;
                let token_id = utils::get_required_arg::<String>(args, "token_id", req_id)?;

                let wallet = LocalWallet::from_str(&private_key).map_err(|_| {
                    Response::error(
                        req_id.clone(),
                        error_codes::INVALID_PARAMS,
                        "Invalid 'private_key'".into(),
                    )
                })?;
                let from_addr = wallet.address();
                let to = Address::from_str(&to_address).map_err(|_| {
                    Response::error(
                        req_id.clone(),
                        error_codes::INVALID_PARAMS,
                        "Invalid 'to_address'".into(),
                    )
                })?;
                let contract = Address::from_str(&contract_address).map_err(|_| {
                    Response::error(
                        req_id.clone(),
                        error_codes::INVALID_PARAMS,
                        "Invalid 'contract_address'".into(),
                    )
                })?;
                let token_u256 = U256::from_dec_str(&token_id).map_err(|_| {
                    Response::error(
                        req_id.clone(),
                        error_codes::INVALID_PARAMS,
                        "Invalid 'token_id'".into(),
                    )
                })?;

                // Encode safeTransferFrom(address,address,uint256)
                let selector =
                    &keccak256("safeTransferFrom(address,address,uint256)".as_bytes())[0..4];
                let data_bytes = {
                    let mut encoded = selector.to_vec();
                    let tokens = vec![
                        Token::Address(from_addr.into()),
                        Token::Address(to.into()),
                        Token::Uint(token_u256.into()),
                    ];
                    let mut tail = encode(&tokens);
                    encoded.append(&mut tail);
                    Bytes::from(encoded)
                };

                let mut tx_request = TransactionRequest::new()
                    .to(contract)
                    .data(data_bytes)
                    .value(U256::zero());
                if let Some(g) = args.get("gas_limit").and_then(|v| v.as_str()) {
                    tx_request =
                        tx_request.gas(U256::from_dec_str(g).unwrap_or_else(|_| U256::from(0)));
                }
                if let Some(gp) = args.get("gas_price").and_then(|v| v.as_str()) {
                    tx_request = tx_request
                        .gas_price(U256::from_dec_str(gp).unwrap_or_else(|_| U256::from(0)));
                }
                let response = state
                    .evm_client
                    .send_transaction(&chain_id, &private_key, tx_request, &state.nonce_manager)
                    .await
                    .map_err(|e| {
                        Response::error(req_id.clone(), error_codes::INTERNAL_ERROR, e.to_string())
                    })?;
                Ok(Response::success(req_id.clone(), json!(response)))
            })
            .await;
            res.unwrap_or_else(|err_resp| err_resp)
        }

        // --- Secure Wallet Storage Tools ---
        "register_wallet" => {
            let res: Result<Response, Response> = (async {
                let wallet_name = utils::get_required_arg::<String>(args, "wallet_name", req_id)?;
                let master_password =
                    utils::get_required_arg::<String>(args, "master_password", req_id)?;

                // Accept either 'mnemonic_or_private_key' (preferred) or legacy 'private_key'
                let key = if let Some(s) = args
                    .get("mnemonic_or_private_key")
                    .and_then(|v| v.as_str())
                { s.to_string() } else {
                    utils::get_required_arg::<String>(args, "private_key", req_id)?
                };

                let wallet_info: WalletResponse =
                    wallet::import_wallet(&key).map_err(|e| {
                        Response::error(req_id.clone(), error_codes::INVALID_PARAMS, e.to_string())
                    })?;

                // Lazy-initialize or load wallet storage from disk using the provided master password.
                // If in-memory storage is empty (no master password set), replace it with the loaded one.
                {
                    let mut storage = state.wallet_storage.lock().await;
                    if storage.is_master_password_hash_empty() {
                        let loaded = wallet_storage::load_or_create_wallet_storage(&state.wallet_storage_path, &master_password)
                            .map_err(|e| {
                                Response::error(
                                    req_id.clone(),
                                    error_codes::INTERNAL_ERROR,
                                    format!("Failed to initialize wallet storage: {}", e),
                                )
                            })?;
                        *storage = loaded;
                    } else if !storage.verify_master_password(&master_password) {
                        return Err(Response::error(
                            req_id.clone(),
                            error_codes::INTERNAL_ERROR,
                            "Authentication failed".into(),
                        ));
                    }
                }

                // Add wallet into storage
                {
                    let mut storage = state.wallet_storage.lock().await;
                    storage
                        .add_wallet(
                            wallet_name.clone(),
                            wallet_info.private_key.as_str(),
                            wallet_info.address.clone(),
                            &master_password,
                        )
                        .map_err(|e| {
                            Response::error(req_id.clone(), error_codes::INTERNAL_ERROR, e.to_string())
                        })?;
                    // Persist to disk
                    wallet_storage::save_wallet_storage(&state.wallet_storage_path, &storage).map_err(
                        |e| {
                            error!("Failed to save wallet storage: {}", e);
                            Response::error(
                                req_id.clone(),
                                error_codes::INTERNAL_ERROR,
                                "Failed to save wallet to disk".into(),
                            )
                        },
                    )?;
                }

                // Return the derived address too for convenience
                let payload = json!({ "status": "success", "wallet_name": wallet_name, "address": wallet_info.address });
                let summary = format!("Registered wallet {}", wallet_name);
                Ok(Response::success(
                    req_id.clone(),
                    make_texty_result(summary, payload),
                ))
            })
            .await;
            res.unwrap_or_else(|err_resp| err_resp)
        }

        "list_wallets" => {
            let res: Result<Response, Response> = (async {
                let master_password =
                    utils::get_required_arg::<String>(args, "master_password", req_id)?;
                // Lazy-load or initialize storage if needed using the provided master password
                {
                    let mut storage = state.wallet_storage.lock().await;
                    if storage.is_master_password_hash_empty() {
                        let loaded = wallet_storage::load_or_create_wallet_storage(
                            &state.wallet_storage_path,
                            &master_password,
                        )
                        .map_err(|e| {
                            Response::error(
                                req_id.clone(),
                                error_codes::INTERNAL_ERROR,
                                format!("Failed to initialize wallet storage: {}", e),
                            )
                        })?;
                        *storage = loaded;
                    }
                }
                let storage = state.wallet_storage.lock().await;
                if !storage.verify_master_password(&master_password) {
                    return Err(Response::error(
                        req_id.clone(),
                        error_codes::INTERNAL_ERROR,
                        "Authentication failed".into(),
                    ));
                }
                // Return wallet names with their public addresses
                let mut wallets: Vec<serde_json::Value> = Vec::new();
                for w in storage.wallets().values() {
                    wallets.push(json!({
                        "wallet_name": w.wallet_name,
                        "address": w.public_address,
                    }));
                }
                wallets.sort_by(|a, b| {
                    a["wallet_name"]
                        .as_str()
                        .unwrap_or("")
                        .cmp(b["wallet_name"].as_str().unwrap_or(""))
                });
                let count = wallets.len();
                // Build a human-readable list for MCP clients that only display text content
                let mut lines: Vec<String> = Vec::new();
                for w in storage.wallets().values() {
                    lines.push(format!("• {} — {}", w.wallet_name, w.public_address));
                }
                lines.sort();
                let details_text = if lines.is_empty() {
                    "No wallets stored".to_string()
                } else {
                    format!("\n{}", lines.join("\n"))
                };
                let summary = format!("{} wallet(s)", count);
                let content = if lines.is_empty() {
                    vec![json!({ "type": "text", "text": summary })]
                } else {
                    vec![
                        json!({ "type": "text", "text": summary }),
                        json!({ "type": "text", "text": details_text }),
                    ]
                };
                Ok(Response::success(
                    req_id.clone(),
                    json!({
                        "count": count,
                        "wallets": wallets,
                        "content": content
                    }),
                ))
            })
            .await;
            res.unwrap_or_else(|err_resp| err_resp)
        }

        "transfer_from_wallet" => {
            let res: Result<Response, Response> = (async {
                let wallet_name = utils::get_required_arg::<String>(args, "wallet_name", req_id)?;
                let chain_id = utils::get_required_arg::<String>(args, "chain_id", req_id)?;
                let to_address = utils::get_required_arg::<String>(args, "to_address", req_id)?;
                let amount = utils::get_required_arg::<String>(args, "amount", req_id)?;
                let master_password =
                    utils::get_required_arg::<String>(args, "master_password", req_id)?;

                let private_key = {
                    // Scoped lock
                    let storage = state.wallet_storage.lock().await;
                    storage
                        .get_private_key(&wallet_name, &master_password)
                        .map_err(|e| {
                            Response::error(
                                req_id.clone(),
                                error_codes::INTERNAL_ERROR,
                                e.to_string(),
                            )
                        })?
                };

                let to = Address::from_str(&to_address).map_err(|_| {
                    Response::error(
                        req_id.clone(),
                        error_codes::INVALID_PARAMS,
                        "Invalid 'to_address'".into(),
                    )
                })?;
                let value = U256::from_dec_str(&amount).map_err(|_| {
                    Response::error(
                        req_id.clone(),
                        error_codes::INVALID_PARAMS,
                        "Invalid 'amount'".into(),
                    )
                })?;

                let tx_request = TransactionRequest::new().to(to).value(value);

                let response = state
                    .evm_client
                    .send_transaction(&chain_id, &private_key, tx_request, &state.nonce_manager)
                    .await
                    .map_err(|e| {
                        Response::error(req_id.clone(), error_codes::INTERNAL_ERROR, e.to_string())
                    })?;
                let summary = match serde_json::to_string(&response) {
                    Ok(s) => format!("Transfer sent: {}", s),
                    Err(_) => "Transfer sent".to_string(),
                };
                Ok(Response::success(
                    req_id.clone(),
                    make_texty_result(summary, json!(response)),
                ))
            })
            .await;
            res.unwrap_or_else(|err_resp| err_resp)
        }
        "get_contract" => {
            let res: Result<Response, Response> = (async {
                let address = utils::get_required_arg::<String>(args, "address", req_id)?;
                let etherscan_api_key = match state.config.etherscan_api_key.as_ref() {
                    Some(key) => key,
                    None => {
                        return Err(Response::error(
                            req_id.clone(),
                            error_codes::INVALID_PARAMS,
                            "ETHERSCAN_API_KEY is not configured".to_string(),
                        ));
                    }
                };

                // Prefer explicit chain_id, else infer from NL, default to mainnet
                let mut chain = args
                    .get("chain_id")
                    .and_then(|v| v.as_str())
                    .map(normalize_chain_id);
                if chain.is_none() {
                    chain = infer_evm_chain_from_args(args);
                }
                let mut chain_id = chain.unwrap_or_else(|| "1".to_string());
                chain_id = normalize_chain_id(&chain_id);

                // Determine Etherscan base URL based on chain
                let etherscan_base_url = match chain_id.as_str() {
                    "1" => "https://api.etherscan.io/v2/api",
                    "11155111" => "https://api-sepolia.etherscan.io/v2/api",
                    _ => {
                        return Err(Response::error(
                            req_id.clone(),
                            error_codes::INVALID_PARAMS,
                            format!("Etherscan API not supported for chain_id '{}'", chain_id),
                        ));
                    }
                };

                // Build Etherscan API URL for getsourcecode
                let url = format!(
                    "{}?chainid={}&module=contract&action=getsourcecode&address={}&apikey={}",
                    etherscan_base_url, chain_id, address, etherscan_api_key
                );

                let client = Client::new();
                let resp: serde_json::Value = client
                    .get(&url)
                    .send()
                    .await
                    .map_err(|e| {
                        Response::error(
                            req_id.clone(),
                            error_codes::INTERNAL_ERROR,
                            format!("Etherscan API error: {}", e),
                        )
                    })?
                    .json()
                    .await
                    .map_err(|e| {
                        Response::error(
                            req_id.clone(),
                            error_codes::INTERNAL_ERROR,
                            format!("Invalid Etherscan JSON response: {}", e),
                        )
                    })?;

                // Check for Etherscan API errors
                if let Some(status) = resp.get("status").and_then(|v| v.as_str()) {
                    if status != "1" {
                        let message = resp
                            .get("message")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Unknown error");
                        return Err(Response::error(
                            req_id.clone(),
                            error_codes::INTERNAL_ERROR,
                            format!("Etherscan API error: {}", message),
                        ));
                    }
                }

                // Extract contract info from result
                let result = resp
                    .get("result")
                    .and_then(|v| v.as_array())
                    .and_then(|arr| arr.get(0));
                let summary = format!("Contract {} on {}", address, chain_id);
                let pretty = serde_json::to_string_pretty(&result)
                    .unwrap_or_else(|_| "No contract data found".to_string());

                Ok(Response::success(
                    req_id.clone(),
                    json!({
                        "content": [
                            { "type": "text", "text": format!("{}\n\n{}", summary, pretty) }
                        ]
                    }),
                ))
            })
            .await;
            res.unwrap_or_else(|err_resp| err_resp)
        }
        "get_contract_code" => {
            let res: Result<Response, Response> = (async {
                let address = utils::get_required_arg::<String>(args, "address", req_id)?;
                let etherscan_api_key = match state.config.etherscan_api_key.as_ref() {
                    Some(key) => key,
                    None => {
                        return Err(Response::error(
                            req_id.clone(),
                            error_codes::INVALID_PARAMS,
                            "ETHERSCAN_API_KEY is not configured".to_string(),
                        ));
                    }
                };

                let mut chain = args
                    .get("chain_id")
                    .and_then(|v| v.as_str())
                    .map(normalize_chain_id);
                if chain.is_none() {
                    chain = infer_evm_chain_from_args(args);
                }
                let chain_id = chain.unwrap_or_else(|| "1".to_string());

                // Determine Etherscan base URL based on chain
                let etherscan_base_url = match chain_id.as_str() {
                    "1" => "https://api.etherscan.io/v2/api",
                    "11155111" => "https://api-sepolia.etherscan.io/v2/api",
                    _ => {
                        return Err(Response::error(
                            req_id.clone(),
                            error_codes::INVALID_PARAMS,
                            format!("Etherscan API not supported for chain_id '{}'", chain_id),
                        ));
                    }
                };

                // Build Etherscan API URL for getsourcecode
                let url = format!(
                    "{}?chainid={}&module=contract&action=getsourcecode&address={}&apikey={}",
                    etherscan_base_url, chain_id, address, etherscan_api_key
                );

                let client = Client::new();
                let resp: serde_json::Value = client
                    .get(&url)
                    .send()
                    .await
                    .map_err(|e| {
                        Response::error(
                            req_id.clone(),
                            error_codes::INTERNAL_ERROR,
                            format!("Etherscan API error: {}", e),
                        )
                    })?
                    .json()
                    .await
                    .map_err(|e| {
                        Response::error(
                            req_id.clone(),
                            error_codes::INTERNAL_ERROR,
                            format!("Invalid Etherscan JSON response: {}", e),
                        )
                    })?;

                // Check for Etherscan API errors
                if let Some(status) = resp.get("status").and_then(|v| v.as_str()) {
                    if status != "1" {
                        let message = resp
                            .get("message")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Unknown error");
                        return Err(Response::error(
                            req_id.clone(),
                            error_codes::INTERNAL_ERROR,
                            format!("Etherscan API error: {}", message),
                        ));
                    }
                }

                // Extract contract code from result
                let result = resp
                    .get("result")
                    .and_then(|v| v.as_array())
                    .and_then(|arr| arr.get(0));
                let bytecode = result
                    .and_then(|r| r.get("RuntimeCode"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("No bytecode found");
                let summary = format!("Contract bytecode for {} on {}", address, chain_id);

                Ok(Response::success(
                    req_id.clone(),
                    json!({
                        "content": [
                            { "type": "text", "text": format!("{}\n\n{}", summary, bytecode) }
                        ]
                    }),
                ))
            })
            .await;
            res.unwrap_or_else(|err_resp| err_resp)
        }
        "get_contract_transactions" => {
            let res: Result<Response, Response> = (async {
                let address = utils::get_required_arg::<String>(args, "address", req_id)?;
                let etherscan_api_key = match state.config.etherscan_api_key.as_ref() {
                    Some(key) => key,
                    None => {
                        return Err(Response::error(
                            req_id.clone(),
                            error_codes::INVALID_PARAMS,
                            "ETHERSCAN_API_KEY is not configured".to_string(),
                        ));
                    }
                };

                let mut chain = args
                    .get("chain_id")
                    .and_then(|v| v.as_str())
                    .map(normalize_chain_id);
                if chain.is_none() {
                    chain = infer_evm_chain_from_args(args);
                }
                let mut chain_id = chain.unwrap_or_else(|| "1".to_string());
                chain_id = normalize_chain_id(&chain_id);

                // Determine Etherscan base URL based on chain
                let etherscan_base_url = match chain_id.as_str() {
                    "1" => "https://api.etherscan.io/v2/api",
                    "11155111" => "https://api-sepolia.etherscan.io/v2/api",
                    _ => {
                        return Err(Response::error(
                            req_id.clone(),
                            error_codes::INVALID_PARAMS,
                            format!("Etherscan API not supported for chain_id '{}'", chain_id),
                        ));
                    }
                };

                // Build Etherscan API URL for txlist
                let url = format!(
                    "{}?chainid={}&module=account&action=txlist&address={}&startblock=0&endblock=99999999&page=1&offset=10&sort=asc&apikey={}",
                    etherscan_base_url, chain_id, address, etherscan_api_key
                );

                let client = Client::new();
                let resp: serde_json::Value = client
                    .get(&url)
                    .send()
                    .await
                    .map_err(|e| {
                        Response::error(
                            req_id.clone(),
                            error_codes::INTERNAL_ERROR,
                            format!("Etherscan API error: {}", e),
                        )
                    })?
                    .json()
                    .await
                    .map_err(|e| {
                        Response::error(
                            req_id.clone(),
                            error_codes::INTERNAL_ERROR,
                            format!("Invalid Etherscan JSON response: {}", e),
                        )
                    })?;

                // Check for Etherscan API errors
                if let Some(status) = resp.get("status").and_then(|v| v.as_str()) {
                    if status != "1" {
                        let message = resp.get("message").and_then(|v| v.as_str()).unwrap_or("Unknown error");
                        return Err(Response::error(
                            req_id.clone(),
                            error_codes::INTERNAL_ERROR,
                            format!("Etherscan API error: {}", message),
                        ));
                    }
                }

                // Extract transactions from result
                let transactions = resp.get("result").cloned().unwrap_or_else(|| serde_json::Value::Array(vec![]));
                let count = transactions.as_array().map(|a| a.len()).unwrap_or(0);
                let summary = format!("{} transaction(s) found for contract {} on {}", count, address, chain_id);

                Ok(Response::success(
                    req_id.clone(),
                    json!({
                        "content": [
                            { "type": "text", "text": format!("{}\n\n{}", summary, serde_json::to_string_pretty(&transactions).unwrap_or_else(|_| "No transactions found".to_string())) }
                        ]
                    }),
                ))
            })
            .await;
            res.unwrap_or_else(|err_resp| err_resp)
        }
        "get_transaction_history" => {
            let res: Result<Response, Response> = (async {
                let address = utils::get_required_arg::<String>(args, "address", req_id)?;
                let etherscan_api_key = match state.config.etherscan_api_key.as_ref() {
                    Some(key) => key,
                    None => {
                        return Err(Response::error(
                            req_id.clone(),
                            error_codes::INVALID_PARAMS,
                            "ETHERSCAN_API_KEY is not configured".to_string(),
                        ));
                    }
                };

                let mut chain = args
                    .get("chain_id")
                    .and_then(|v| v.as_str())
                    .map(normalize_chain_id);
                if chain.is_none() {
                    chain = infer_evm_chain_from_args(args);
                }
                let mut chain_id = chain.unwrap_or_else(|| "1".to_string());
                chain_id = normalize_chain_id(&chain_id);

                // Determine Etherscan base URL based on chain
                let etherscan_base_url = match chain_id.as_str() {
                    "1" => "https://api.etherscan.io/v2/api",
                    "11155111" => "https://api-sepolia.etherscan.io/v2/api",
                    _ => {
                        return Err(Response::error(
                            req_id.clone(),
                            error_codes::INVALID_PARAMS,
                            format!("Etherscan API not supported for chain_id '{}'", chain_id),
                        ));
                    }
                };

                // Build Etherscan API URL for txlist
                let url = format!(
                    "{}?chainid={}&module=account&action=txlist&address={}&startblock=0&endblock=99999999&page=1&offset=10&sort=asc&apikey={}",
                    etherscan_base_url, chain_id, address, etherscan_api_key
                );

                let client = Client::new();
                let resp: serde_json::Value = client
                    .get(&url)
                    .send()
                    .await
                    .map_err(|e| {
                        Response::error(
                            req_id.clone(),
                            error_codes::INTERNAL_ERROR,
                            format!("Etherscan API error: {}", e),
                        )
                    })?
                    .json()
                    .await
                    .map_err(|e| {
                        Response::error(
                            req_id.clone(),
                            error_codes::INTERNAL_ERROR,
                            format!("Invalid Etherscan JSON response: {}", e),
                        )
                    })?;

                // Check for Etherscan API errors
                if let Some(status) = resp.get("status").and_then(|v| v.as_str()) {
                    if status != "1" {
                        let message = resp.get("message").and_then(|v| v.as_str()).unwrap_or("Unknown error");
                        return Err(Response::error(
                            req_id.clone(),
                            error_codes::INTERNAL_ERROR,
                            format!("Etherscan API error: {}", message),
                        ));
                    }
                }

                // Extract transactions from result
                let transactions = resp.get("result").cloned().unwrap_or_else(|| serde_json::Value::Array(vec![]));
                let count = transactions.as_array().map(|a| a.len()).unwrap_or(0);
                let summary = format!("{} transaction(s) found for address {} on {}", count, address, chain_id);

                Ok(Response::success(
                    req_id.clone(),
                    json!({
                        "content": [
                            { "type": "text", "text": format!("{}\n\n{}", summary, serde_json::to_string_pretty(&transactions).unwrap_or_else(|_| "No transactions found".to_string())) }
                        ]
                    }),
                ))
            })
            .await;
            res.unwrap_or_else(|err_resp| err_resp)
        }
        // --- Token services: ERC20 / ERC721 / ERC1155 ---
        // Aliases: accept both snake_case and hyphen-case used upstream
        "get_token_info" | "get-token-info" => {
            let res: Result<Response, Response> = (async {
                let mut chain_id = args
                    .get("chain_id")
                    .or_else(|| args.get("network"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "1".to_string());
                chain_id = normalize_chain_id(&chain_id);
                let token = utils::get_required_arg::<String>(args, "tokenAddress", req_id)
                    .or_else(|_| {
                        utils::get_required_arg::<String>(args, "token_address", req_id)
                    })?;
                let rpc_url = state.config.chain_rpc_urls.get(&chain_id).ok_or_else(|| {
                    Response::error(
                        req_id.clone(),
                        error_codes::INVALID_PARAMS,
                        format!("RPC URL not configured for chain_id '{}'", chain_id),
                    )
                })?;
                let client = Client::new();
                let v = crate::blockchain::services::token::erc20_info(&client, rpc_url, &token)
                    .await
                    .map_err(|e| {
                        Response::error(req_id.clone(), error_codes::INTERNAL_ERROR, e.to_string())
                    })?;
                Ok(Response::success(
                    req_id.clone(),
                    make_texty_result(format!("Token info {} on {}", token, chain_id), v),
                ))
            })
            .await;
            match res {
                Ok(r) => r,
                Err(e) => e,
            }
        }
        "get_token_balance" | "get-token-balance" => {
            let res: Result<Response, Response> = (async {
                let mut chain_id = args
                    .get("chain_id")
                    .or_else(|| args.get("network"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "1".to_string());
                chain_id = normalize_chain_id(&chain_id);
                let token = utils::get_required_arg::<String>(args, "tokenAddress", req_id)
                    .or_else(|_| {
                        utils::get_required_arg::<String>(args, "token_address", req_id)
                    })?;
                let owner = utils::get_required_arg::<String>(args, "ownerAddress", req_id)
                    .or_else(|_| {
                        utils::get_required_arg::<String>(args, "owner_address", req_id)
                    })?;
                let rpc_url = state.config.chain_rpc_urls.get(&chain_id).ok_or_else(|| {
                    Response::error(
                        req_id.clone(),
                        error_codes::INVALID_PARAMS,
                        format!("RPC URL not configured for chain_id '{}'", chain_id),
                    )
                })?;
                let client = Client::new();
                let v = crate::blockchain::services::token::erc20_balance_of(
                    &client, rpc_url, &token, &owner,
                )
                .await
                .map_err(|e| {
                    Response::error(req_id.clone(), error_codes::INTERNAL_ERROR, e.to_string())
                })?;
                Ok(Response::success(
                    req_id.clone(),
                    make_texty_result(format!("ERC20 balance of {}", owner), v),
                ))
            })
            .await;
            match res {
                Ok(r) => r,
                Err(e) => e,
            }
        }
        "get_token_allowance" | "get-token-allowance" => {
            let res: Result<Response, Response> = (async {
                let mut chain_id = args
                    .get("chain_id")
                    .or_else(|| args.get("network"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "1".to_string());
                chain_id = normalize_chain_id(&chain_id);
                let token = utils::get_required_arg::<String>(args, "tokenAddress", req_id)
                    .or_else(|_| {
                        utils::get_required_arg::<String>(args, "token_address", req_id)
                    })?;
                let owner = utils::get_required_arg::<String>(args, "ownerAddress", req_id)
                    .or_else(|_| {
                        utils::get_required_arg::<String>(args, "owner_address", req_id)
                    })?;
                let spender = utils::get_required_arg::<String>(args, "spenderAddress", req_id)
                    .or_else(|_| {
                        utils::get_required_arg::<String>(args, "spender_address", req_id)
                    })?;
                let rpc_url = state.config.chain_rpc_urls.get(&chain_id).ok_or_else(|| {
                    Response::error(
                        req_id.clone(),
                        error_codes::INVALID_PARAMS,
                        format!("RPC URL not configured for chain_id '{}'", chain_id),
                    )
                })?;
                let client = Client::new();
                let v = crate::blockchain::services::token::erc20_allowance(
                    &client, rpc_url, &token, &owner, &spender,
                )
                .await
                .map_err(|e| {
                    Response::error(req_id.clone(), error_codes::INTERNAL_ERROR, e.to_string())
                })?;
                Ok(Response::success(
                    req_id.clone(),
                    make_texty_result(format!("ERC20 allowance of {} -> {}", owner, spender), v),
                ))
            })
            .await;
            match res {
                Ok(r) => r,
                Err(e) => e,
            }
        }
        "transfer_token" | "transfer-token" => {
            let res: Result<Response, Response> = (async {
                let private_key = utils::get_required_arg::<String>(args, "private_key", req_id)?;
                let mut chain_id = args
                    .get("chain_id")
                    .or_else(|| args.get("network"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "1".to_string());
                chain_id = normalize_chain_id(&chain_id);
                let token = utils::get_required_arg::<String>(args, "tokenAddress", req_id)
                    .or_else(|_| {
                        utils::get_required_arg::<String>(args, "token_address", req_id)
                    })?;
                let to = utils::get_required_arg::<String>(args, "toAddress", req_id)
                    .or_else(|_| utils::get_required_arg::<String>(args, "to_address", req_id))?;
                let amount = utils::get_required_arg::<String>(args, "amount", req_id)
                    .or_else(|_| utils::get_required_arg::<String>(args, "amount_wei", req_id))?;
                let mut tx =
                    crate::blockchain::services::token::erc20_transfer_tx(&token, &to, &amount)
                        .map_err(|e| {
                            Response::error(
                                req_id.clone(),
                                error_codes::INVALID_PARAMS,
                                e.to_string(),
                            )
                        })?;
                if let Some(g) = args.get("gas_limit").and_then(|v| v.as_str()) {
                    tx = tx.gas(U256::from_dec_str(g).unwrap_or_else(|_| U256::from(0)));
                }
                if let Some(gp) = args.get("gas_price").and_then(|v| v.as_str()) {
                    tx = tx.gas_price(U256::from_dec_str(gp).unwrap_or_else(|_| U256::from(0)));
                }
                let resp = state
                    .evm_client
                    .send_transaction(&chain_id, &private_key, tx, &state.nonce_manager)
                    .await
                    .map_err(|e| {
                        Response::error(req_id.clone(), error_codes::INTERNAL_ERROR, e.to_string())
                    })?;
                Ok(Response::success(
                    req_id.clone(),
                    make_texty_result("ERC20 transfer sent".into(), json!(resp)),
                ))
            })
            .await;
            match res {
                Ok(r) => r,
                Err(e) => e,
            }
        }
        "approve_token_spending" | "approve-token-spending" => {
            let res: Result<Response, Response> = (async {
                let private_key = utils::get_required_arg::<String>(args, "private_key", req_id)?;
                let mut chain_id = args
                    .get("chain_id")
                    .or_else(|| args.get("network"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "1".to_string());
                chain_id = normalize_chain_id(&chain_id);
                let token = utils::get_required_arg::<String>(args, "tokenAddress", req_id)
                    .or_else(|_| {
                        utils::get_required_arg::<String>(args, "token_address", req_id)
                    })?;
                let spender = utils::get_required_arg::<String>(args, "spenderAddress", req_id)
                    .or_else(|_| {
                        utils::get_required_arg::<String>(args, "spender_address", req_id)
                    })?;
                let amount = utils::get_required_arg::<String>(args, "amount", req_id)
                    .or_else(|_| utils::get_required_arg::<String>(args, "amount_wei", req_id))?;
                let mut tx =
                    crate::blockchain::services::token::erc20_approve_tx(&token, &spender, &amount)
                        .map_err(|e| {
                            Response::error(
                                req_id.clone(),
                                error_codes::INVALID_PARAMS,
                                e.to_string(),
                            )
                        })?;
                if let Some(g) = args.get("gas_limit").and_then(|v| v.as_str()) {
                    tx = tx.gas(U256::from_dec_str(g).unwrap_or_else(|_| U256::from(0)));
                }
                if let Some(gp) = args.get("gas_price").and_then(|v| v.as_str()) {
                    tx = tx.gas_price(U256::from_dec_str(gp).unwrap_or_else(|_| U256::from(0)));
                }
                let resp = state
                    .evm_client
                    .send_transaction(&chain_id, &private_key, tx, &state.nonce_manager)
                    .await
                    .map_err(|e| {
                        Response::error(req_id.clone(), error_codes::INTERNAL_ERROR, e.to_string())
                    })?;
                Ok(Response::success(
                    req_id.clone(),
                    make_texty_result("ERC20 approve sent".into(), json!(resp)),
                ))
            })
            .await;
            match res {
                Ok(r) => r,
                Err(e) => e,
            }
        }
        "get_nft_info" | "get-nft-info" => {
            let res: Result<Response, Response> = (async {
                let mut chain_id = args
                    .get("chain_id")
                    .or_else(|| args.get("network"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "1".to_string());
                chain_id = normalize_chain_id(&chain_id);
                let token = utils::get_required_arg::<String>(args, "tokenAddress", req_id)
                    .or_else(|_| {
                        utils::get_required_arg::<String>(args, "token_address", req_id)
                    })?;
                let token_id = utils::get_required_arg::<String>(args, "tokenId", req_id)
                    .or_else(|_| utils::get_required_arg::<String>(args, "token_id", req_id))?;
                let rpc_url = state.config.chain_rpc_urls.get(&chain_id).ok_or_else(|| {
                    Response::error(
                        req_id.clone(),
                        error_codes::INVALID_PARAMS,
                        format!("RPC URL not configured for chain_id '{}'", chain_id),
                    )
                })?;
                let client = Client::new();
                let uri = crate::blockchain::services::token::erc721_token_uri(
                    &client, rpc_url, &token, &token_id,
                )
                .await
                .map_err(|e| {
                    Response::error(req_id.clone(), error_codes::INTERNAL_ERROR, e.to_string())
                })?;
                Ok(Response::success(
                    req_id.clone(),
                    make_texty_result("ERC721 tokenURI".into(), uri),
                ))
            })
            .await;
            match res {
                Ok(r) => r,
                Err(e) => e,
            }
        }
        "check_nft_ownership" | "check-nft-ownership" => {
            let res: Result<Response, Response> = (async {
                let mut chain_id = args.get("chain_id").or_else(|| args.get("network")).and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or_else(|| "1".to_string());
                chain_id = normalize_chain_id(&chain_id);
                let token = utils::get_required_arg::<String>(args, "tokenAddress", req_id).or_else(|_| utils::get_required_arg::<String>(args, "token_address", req_id))?;
                let token_id = utils::get_required_arg::<String>(args, "tokenId", req_id).or_else(|_| utils::get_required_arg::<String>(args, "token_id", req_id))?;
                let owner = utils::get_required_arg::<String>(args, "ownerAddress", req_id).or_else(|_| utils::get_required_arg::<String>(args, "owner_address", req_id))?;
                let rpc_url = state.config.chain_rpc_urls.get(&chain_id).ok_or_else(|| Response::error(req_id.clone(), error_codes::INVALID_PARAMS, format!("RPC URL not configured for chain_id '{}'", chain_id)))?;
                let client = Client::new();
                let res_owner = crate::blockchain::services::token::erc721_owner_of(&client, rpc_url, &token, &token_id).await
                    .map_err(|e| Response::error(req_id.clone(), error_codes::INTERNAL_ERROR, e.to_string()))?;
                Ok(Response::success(req_id.clone(), json!({
                    "data": res_owner,
                    "content": [{"type":"text","text": format!("ownerOf == {}? (raw hex encoded)", owner)}]
                })))
            }).await;
            match res {
                Ok(r) => r,
                Err(e) => e,
            }
        }
        "get_nft_balance" | "get-nft-balance" => {
            let res: Result<Response, Response> = (async {
                let mut chain_id = args
                    .get("chain_id")
                    .or_else(|| args.get("network"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "1".to_string());
                chain_id = normalize_chain_id(&chain_id);
                let token = utils::get_required_arg::<String>(args, "tokenAddress", req_id)
                    .or_else(|_| {
                        utils::get_required_arg::<String>(args, "token_address", req_id)
                    })?;
                let owner = utils::get_required_arg::<String>(args, "ownerAddress", req_id)
                    .or_else(|_| {
                        utils::get_required_arg::<String>(args, "owner_address", req_id)
                    })?;
                let rpc_url = state.config.chain_rpc_urls.get(&chain_id).ok_or_else(|| {
                    Response::error(
                        req_id.clone(),
                        error_codes::INVALID_PARAMS,
                        format!("RPC URL not configured for chain_id '{}'", chain_id),
                    )
                })?;
                let client = Client::new();
                let v = crate::blockchain::services::token::erc721_balance_of(
                    &client, rpc_url, &token, &owner,
                )
                .await
                .map_err(|e| {
                    Response::error(req_id.clone(), error_codes::INTERNAL_ERROR, e.to_string())
                })?;
                Ok(Response::success(
                    req_id.clone(),
                    make_texty_result("ERC721 balanceOf".into(), v),
                ))
            })
            .await;
            match res {
                Ok(r) => r,
                Err(e) => e,
            }
        }
        "get_erc1155_token_uri" | "get-erc1155-token-uri" => {
            let res: Result<Response, Response> = (async {
                let mut chain_id = args
                    .get("chain_id")
                    .or_else(|| args.get("network"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "1".to_string());
                chain_id = normalize_chain_id(&chain_id);
                let token = utils::get_required_arg::<String>(args, "tokenAddress", req_id)
                    .or_else(|_| {
                        utils::get_required_arg::<String>(args, "token_address", req_id)
                    })?;
                let token_id = utils::get_required_arg::<String>(args, "tokenId", req_id)
                    .or_else(|_| utils::get_required_arg::<String>(args, "token_id", req_id))?;
                let rpc_url = state.config.chain_rpc_urls.get(&chain_id).ok_or_else(|| {
                    Response::error(
                        req_id.clone(),
                        error_codes::INVALID_PARAMS,
                        format!("RPC URL not configured for chain_id '{}'", chain_id),
                    )
                })?;
                let client = Client::new();
                let v = crate::blockchain::services::token::erc1155_uri(
                    &client, rpc_url, &token, &token_id,
                )
                .await
                .map_err(|e| {
                    Response::error(req_id.clone(), error_codes::INTERNAL_ERROR, e.to_string())
                })?;
                Ok(Response::success(
                    req_id.clone(),
                    make_texty_result("ERC1155 uri".into(), v),
                ))
            })
            .await;
            match res {
                Ok(r) => r,
                Err(e) => e,
            }
        }
        "get_erc1155_balance" | "get-erc1155-balance" => {
            let res: Result<Response, Response> = (async {
                let mut chain_id = args
                    .get("chain_id")
                    .or_else(|| args.get("network"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "1".to_string());
                chain_id = normalize_chain_id(&chain_id);
                let token = utils::get_required_arg::<String>(args, "tokenAddress", req_id)
                    .or_else(|_| {
                        utils::get_required_arg::<String>(args, "token_address", req_id)
                    })?;
                let owner = utils::get_required_arg::<String>(args, "ownerAddress", req_id)
                    .or_else(|_| {
                        utils::get_required_arg::<String>(args, "owner_address", req_id)
                    })?;
                let token_id = utils::get_required_arg::<String>(args, "tokenId", req_id)
                    .or_else(|_| utils::get_required_arg::<String>(args, "token_id", req_id))?;
                let rpc_url = state.config.chain_rpc_urls.get(&chain_id).ok_or_else(|| {
                    Response::error(
                        req_id.clone(),
                        error_codes::INVALID_PARAMS,
                        format!("RPC URL not configured for chain_id '{}'", chain_id),
                    )
                })?;
                let client = Client::new();
                let v = crate::blockchain::services::token::erc1155_balance_of(
                    &client, rpc_url, &token, &owner, &token_id,
                )
                .await
                .map_err(|e| {
                    Response::error(req_id.clone(), error_codes::INTERNAL_ERROR, e.to_string())
                })?;
                Ok(Response::success(
                    req_id.clone(),
                    make_texty_result("ERC1155 balanceOf".into(), v),
                ))
            })
            .await;
            match res {
                Ok(r) => r,
                Err(e) => e,
            }
        }
        "transfer_erc1155" | "transfer-erc1155" => {
            let res: Result<Response, Response> = (async {
                let private_key = utils::get_required_arg::<String>(args, "private_key", req_id)?;
                let mut chain_id = args
                    .get("chain_id")
                    .or_else(|| args.get("network"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "1".to_string());
                chain_id = normalize_chain_id(&chain_id);
                let token = utils::get_required_arg::<String>(args, "tokenAddress", req_id)
                    .or_else(|_| {
                        utils::get_required_arg::<String>(args, "token_address", req_id)
                    })?;
                let from = utils::get_required_arg::<String>(args, "fromAddress", req_id)
                    .or_else(|_| utils::get_required_arg::<String>(args, "from_address", req_id))?;
                let to = utils::get_required_arg::<String>(args, "toAddress", req_id)
                    .or_else(|_| utils::get_required_arg::<String>(args, "to_address", req_id))?;
                let token_id = utils::get_required_arg::<String>(args, "tokenId", req_id)
                    .or_else(|_| utils::get_required_arg::<String>(args, "token_id", req_id))?;
                let amount = utils::get_required_arg::<String>(args, "amount", req_id)?;
                let mut tx = crate::blockchain::services::token::erc1155_safe_transfer_from_tx(
                    &token, &from, &to, &token_id, &amount,
                )
                .map_err(|e| {
                    Response::error(req_id.clone(), error_codes::INVALID_PARAMS, e.to_string())
                })?;
                if let Some(g) = args.get("gas_limit").and_then(|v| v.as_str()) {
                    tx = tx.gas(U256::from_dec_str(g).unwrap_or_else(|_| U256::from(0)));
                }
                if let Some(gp) = args.get("gas_price").and_then(|v| v.as_str()) {
                    tx = tx.gas_price(U256::from_dec_str(gp).unwrap_or_else(|_| U256::from(0)));
                }
                let resp = state
                    .evm_client
                    .send_transaction(&chain_id, &private_key, tx, &state.nonce_manager)
                    .await
                    .map_err(|e| {
                        Response::error(req_id.clone(), error_codes::INTERNAL_ERROR, e.to_string())
                    })?;
                Ok(Response::success(
                    req_id.clone(),
                    make_texty_result("ERC1155 transfer sent".into(), json!(resp)),
                ))
            })
            .await;
            match res {
                Ok(r) => r,
                Err(e) => e,
            }
        }
        // --- Generic contract utils ---
        "is_contract" | "is-contract" => {
            let res: Result<Response, Response> = (async {
                let mut chain_id = args.get("chain_id").or_else(|| args.get("network")).and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or_else(|| "1".to_string());
                chain_id = normalize_chain_id(&chain_id);
                let address = utils::get_required_arg::<String>(args, "address", req_id)?;
                let etherscan_api_key = match state.config.etherscan_api_key.as_ref() {
                    Some(key) => key,
                    None => {
                        return Err(Response::error(
                            req_id.clone(),
                            error_codes::INVALID_PARAMS,
                            "ETHERSCAN_API_KEY is not configured".to_string(),
                        ));
                    }
                };

                // For non-EVM addresses, return false
                if !address.starts_with("0x") {
                    return Ok(Response::success(req_id.clone(), json!({"is_contract": false, "verified": false, "content": [{"type":"text","text": format!("{} is not an EVM address", address)}]})));
                }

                // Determine Etherscan base URL based on chain
                let etherscan_base_url = match chain_id.as_str() {
                    "1" => "https://api.etherscan.io/v2/api",
                    "11155111" => "https://api-sepolia.etherscan.io/v2/api",
                    _ => {
                        return Err(Response::error(
                            req_id.clone(),
                            error_codes::INVALID_PARAMS,
                            format!("Etherscan API not supported for chain_id '{}'", chain_id),
                        ));
                    }
                };

                // Build Etherscan API URL for getsourcecode
                let url = format!(
                    "{}?chainid={}&module=contract&action=getsourcecode&address={}&apikey={}",
                    etherscan_base_url, chain_id, address, etherscan_api_key
                );

                let client = Client::new();
                let resp: serde_json::Value = client
                    .get(&url)
                    .send()
                    .await
                    .map_err(|e| {
                        Response::error(
                            req_id.clone(),
                            error_codes::INTERNAL_ERROR,
                            format!("Etherscan API error: {}", e),
                        )
                    })?
                    .json()
                    .await
                    .map_err(|e| {
                        Response::error(
                            req_id.clone(),
                            error_codes::INTERNAL_ERROR,
                            format!("Invalid Etherscan JSON response: {}", e),
                        )
                    })?;

                // Check for Etherscan API errors
                if let Some(status) = resp.get("status").and_then(|v| v.as_str()) {
                    if status != "1" {
                        let message = resp.get("message").and_then(|v| v.as_str()).unwrap_or("Unknown error");
                        return Err(Response::error(
                            req_id.clone(),
                            error_codes::INTERNAL_ERROR,
                            format!("Etherscan API error: {}", message),
                        ));
                    }
                }

                // Check if contract has source code (verified contract)
                let result = resp.get("result").and_then(|v| v.as_array()).and_then(|arr| arr.get(0));
                let has_source_code = result.and_then(|r| r.get("SourceCode")).and_then(|v| v.as_str()).map(|s| !s.is_empty()).unwrap_or(false);
                let contract_name = result.and_then(|r| r.get("ContractName")).and_then(|v| v.as_str()).unwrap_or("Unknown");

                let is_verified = has_source_code && !contract_name.is_empty();
                let message = if is_verified {
                    format!("{} is a verified contract (ContractName: {})", address, contract_name)
                } else {
                    format!("{} has no verified source code on Etherscan", address)
                };

                Ok(Response::success(req_id.clone(), json!({"is_contract": true, "verified": is_verified, "contract_name": contract_name, "content": [{"type":"text","text": message}]})))
            }).await;
            match res {
                Ok(r) => r,
                Err(e) => e,
            }
        }
        "read_contract" | "read-contract" => {
            let res: Result<Response, Response> = (async {
                let mut chain_id = args
                    .get("chain_id")
                    .or_else(|| args.get("network"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "1".to_string());
                chain_id = normalize_chain_id(&chain_id);
                let contract = utils::get_required_arg::<String>(args, "contractAddress", req_id)
                    .or_else(|_| {
                    utils::get_required_arg::<String>(args, "contract_address", req_id)
                })?;
                let abi = utils::get_required_arg::<String>(args, "abi", req_id)?;
                let function = utils::get_required_arg::<String>(args, "functionName", req_id)
                    .or_else(|_| {
                        utils::get_required_arg::<String>(args, "function_name", req_id)
                    })?;
                let rpc_url = state.config.chain_rpc_urls.get(&chain_id).ok_or_else(|| {
                    Response::error(
                        req_id.clone(),
                        error_codes::INVALID_PARAMS,
                        format!("RPC URL not configured for chain_id '{}'", chain_id),
                    )
                })?;
                let args_vec = args.get("args").and_then(|v| v.as_array()).cloned();
                let client = Client::new();
                let v = crate::blockchain::services::token::read_contract_via_abi(
                    &client, rpc_url, &contract, &abi, &function, args_vec,
                )
                .await
                .map_err(|e| {
                    Response::error(req_id.clone(), error_codes::INTERNAL_ERROR, e.to_string())
                })?;
                Ok(Response::success(
                    req_id.clone(),
                    make_texty_result(format!("Read {}.{}", contract, function), v),
                ))
            })
            .await;
            match res {
                Ok(r) => r,
                Err(e) => e,
            }
        }
        "write_contract" | "write-contract" => {
            let res: Result<Response, Response> = (async {
                let private_key = utils::get_required_arg::<String>(args, "private_key", req_id)?;
                let mut chain_id = args
                    .get("chain_id")
                    .or_else(|| args.get("network"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "1".to_string());
                chain_id = normalize_chain_id(&chain_id);
                let contract = utils::get_required_arg::<String>(args, "contractAddress", req_id)
                    .or_else(|_| {
                    utils::get_required_arg::<String>(args, "contract_address", req_id)
                })?;
                let abi = utils::get_required_arg::<String>(args, "abi", req_id)?;
                let function = utils::get_required_arg::<String>(args, "functionName", req_id)
                    .or_else(|_| {
                        utils::get_required_arg::<String>(args, "function_name", req_id)
                    })?;
                let args_vec = args.get("args").and_then(|v| v.as_array()).cloned();
                let mut tx = crate::blockchain::services::token::write_contract_tx(
                    &contract, &abi, &function, args_vec,
                )
                .map_err(|e| {
                    Response::error(req_id.clone(), error_codes::INVALID_PARAMS, e.to_string())
                })?;
                if let Some(g) = args.get("gas_limit").and_then(|v| v.as_str()) {
                    tx = tx.gas(U256::from_dec_str(g).unwrap_or_else(|_| U256::from(0)));
                }
                if let Some(gp) = args.get("gas_price").and_then(|v| v.as_str()) {
                    tx = tx.gas_price(U256::from_dec_str(gp).unwrap_or_else(|_| U256::from(0)));
                }
                let resp = state
                    .evm_client
                    .send_transaction(&chain_id, &private_key, tx, &state.nonce_manager)
                    .await
                    .map_err(|e| {
                        Response::error(req_id.clone(), error_codes::INTERNAL_ERROR, e.to_string())
                    })?;
                Ok(Response::success(
                    req_id.clone(),
                    make_texty_result(format!("write {}.{} sent", contract, function), json!(resp)),
                ))
            })
            .await;
            match res {
                Ok(r) => r,
                Err(e) => e,
            }
        }
        _ => Response::error(
            req.id,
            error_codes::METHOD_NOT_FOUND,
            format!("Tool not found: {}", tool_name),
        ),
    }
}

/// Handles the 'initialize' request.
fn handle_initialize(req: &Request) -> Response {
    let server_info = json!({
        "name": "evm_mcp",
        "version": "0.1.0"
    });
    let capabilities = json!({ "tools": { "listChanged": false } });
    let instructions =
        "EVM blockchain MCP server for secure wallet operations, balance queries, and transaction management.";

    Response::success(
        req.id.clone(),
        json!({
            "serverInfo": server_info,
            "protocolVersion": "2025-06-18",
            "capabilities": capabilities,
            "instructions": instructions
        }),
    )
}

/// Handles the 'tools/list' request by returning a JSON definition of all available tools.
// FIX: The tool list is now updated, secure, and functional.
fn handle_tools_list(req: &Request) -> Response {
    let tools = json!([
        {
            "name": "get_balance",
            "description": "Get the EVM balance of an address on a specific chain.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "chain_id": {"type": "string", "description": "The blockchain chain ID (e.g., '1' for Ethereum mainnet)"},
                    "address": {"type": "string", "description": "The 0x... EVM wallet address to check."}
                },
                "required": ["chain_id", "address"]
            }
        },
        {
            "name": "create_wallet",
            "description": "Create a new EVM wallet. Returns address, private key, and mnemonic.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
        },
        {
            "name": "import_wallet",
            "description": "Import a wallet from a mnemonic phrase or private key.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "mnemonic_or_private_key": {"type": "string", "description": "Mnemonic phrase or private key."},
                    "key": {"type": "string", "description": "Alias for mnemonic_or_private_key (back-compat)."},
                    "chain_type": {"type": "string", "description": "'evm' (default) or 'native'"}
                },
                "oneOf": [
                    {"required": ["mnemonic_or_private_key"]},
                    {"required": ["key"]}
                ],
                "additionalProperties": false
            }
        },
        {
            "name": "search_events",
            "description": "Search EVM logs via Etherscan API. Supports Ethereum Mainnet and Sepolia Testnet.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "chain_id": {"type": "string", "description": "Chain ID (1 for Ethereum, 11155111 for Sepolia)"},
                    "contract_address": {"type": "string", "description": "Contract address to search logs for"},
                    "topic0": {"type": "string", "description": "Keccak topic0 (event signature hash)"},
                    "from_block": {"type": "string", "description": "Starting block number (decimal or hex)"},
                    "to_block": {"type": "string", "description": "Ending block number (decimal or hex)"}
                },
                "required": ["chain_id", "contract_address"],
                "additionalProperties": false
            }
        },
        {
            "name": "request_faucet",
            "description": "Request testnet tokens from the faucet for an EVM address.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "chain_id": {"type": "string", "description": "Target chain id configured in CHAIN_RPC_URLS."},
                    "address": {"type": "string", "description": "The EVM (0x...) address to receive tokens."}
                },
                "required": ["chain_id", "address"],
                "additionalProperties": false
            }
        },
        {
            "name": "register_wallet",
            "description": "Encrypt and securely store a wallet under a name using a mnemonic or private key.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "wallet_name": {"type": "string", "description": "A unique name for the wallet (e.g., 'my-primary-wallet')."},
                    "mnemonic_or_private_key": {"type": "string", "description": "Mnemonic phrase or private key to register."},
                    "private_key": {"type": "string", "description": "Alias input for compatibility (private key)."},
                    "master_password": {"type": "string", "description": "The master password to encrypt the wallet. This password will be required for any future actions with this wallet."},
                    "chain_type": {"type": "string", "description": "'evm' (default) or 'native'"}
                },
                "oneOf": [
                    {"required": ["wallet_name", "mnemonic_or_private_key", "master_password"]},
                    {"required": ["wallet_name", "private_key", "master_password"]}
                ],
                "additionalProperties": false
            }
        },
        {
            "name": "list_wallets",
            "description": "List the names of all wallets currently stored in the secure storage.",
            "inputSchema": {
                "type": "object",
                "properties": {
                     "master_password": {"type": "string", "description": "The master password for the wallet storage."}
                },
                "required": ["master_password"]
            }
        },
        {
            "name": "transfer_from_wallet",
            "description": "Transfer tokens from a securely stored wallet.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "wallet_name": {"type": "string", "description": "The name of the stored wallet to transfer from."},
                    "chain_id": {"type": "string", "description": "The blockchain chain ID (e.g., 'testnet')."},
                    "to_address": {"type": "string", "description": "The recipient's 0x... EVM address."},
                    "amount": {"type": "string", "description": "The amount to transfer in wei."},
                    "master_password": {"type": "string", "description": "The master password to unlock the wallet for this transaction."}
                },
                "required": ["wallet_name", "chain_id", "to_address", "amount", "master_password"]
            }
        },
        {
            "name": "transfer_evm",
            "description": "Send an EVM value transfer using a provided private key.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "private_key": {"type": "string"},
                    "chain_id": {"type": "string"},
                    "to_address": {"type": "string"},
                    "amount_wei": {"type": "string"},
                    "gas_limit": {"type": "string"},
                    "gas_price": {"type": "string"}
                },
                "required": ["private_key", "chain_id", "to_address", "amount_wei"],
                "additionalProperties": false
            }
        },
        {
            "name": "transfer_nft_evm",
            "description": "Transfer an ERC-721 token.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "private_key": {"type": "string"},
                    "chain_id": {"type": "string"},
                    "contract_address": {"type": "string"},
                    "to_address": {"type": "string"},
                    "token_id": {"type": "string"}
                },
                "required": ["private_key", "chain_id", "contract_address", "to_address", "token_id"],
                "additionalProperties": false
            }
        },
         {
             "name": "get_contract",
             "description": "Get verified contract details from Etherscan API.",
             "inputSchema": {
                 "type": "object",
                 "properties": {
                     "address": {"type": "string", "description": "The address of the smart contract."},
                     "chain_id": {"type": "string", "description": "Chain ID (1 for Ethereum, 11155111 for Sepolia)."}
                 },
                 "required": ["address"]
             }
         },
        {
            "name": "get_contract_code",
            "description": "Get verified contract bytecode from Etherscan API.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "address": {"type": "string", "description": "The address of the smart contract."},
                    "chain_id": {"type": "string", "description": "Chain ID (1 for Ethereum, 11155111 for Sepolia)."}
                },
                "required": ["address"]
            }
        },
        {
            "name": "discord_post_message",
            "description": "Post a message to Discord via webhook or bot token (configured in server env).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "message": {"type": "string", "description": "Text to post. Code or natural language supported."},
                    "username": {"type": "string", "description": "Optional display username (webhook mode)."}
                },
                "required": ["message"],
                "additionalProperties": false
            }
        },
        {
            "name": "get_discord_service_info",
            "description": "Fetch basic info from the external discord-api root endpoint (requires DISCORD_API_URL).",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
        },
        {
            "name": "check_discord_health",
            "description": "Check health of the external discord-api (requires DISCORD_API_URL).",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
        },
        {
            "name": "get_contract_transactions",
            "description": "Get the transactions of a smart contract via Etherscan API.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "address": {"type": "string", "description": "The address of the smart contract."},
                    "chain_id": {"type": "string", "description": "Chain ID (1 for Ethereum, 11155111 for Sepolia)."}
                },
                "required": ["address"]
            }
        },
        {
            "name": "get_transaction_history",
            "description": "Get transaction history for any EVM address via Etherscan API.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "address": {"type": "string", "description": "The EVM address to get transaction history for."},
                    "chain_id": {"type": "string", "description": "Chain ID (1 for Ethereum, 11155111 for Sepolia)."}
                },
                "required": ["address"]
            }
        },
        // --- Added: Token services (ERC20) ---
        {
            "name": "get_token_info",
            "description": "Get ERC20 token metadata.",
            "inputSchema": {"type": "object", "properties": {"tokenAddress": {"type": "string"}, "chain_id": {"type": "string"}, "network": {"type": "string"}}, "required": ["tokenAddress"], "additionalProperties": false}
        },
        {
            "name": "get_token_balance",
            "description": "Check ERC20 token balance.",
            "inputSchema": {"type": "object", "properties": {"tokenAddress": {"type": "string"}, "ownerAddress": {"type": "string"}, "chain_id": {"type": "string"}, "network": {"type": "string"}}, "required": ["tokenAddress", "ownerAddress"], "additionalProperties": false}
        },
        {
            "name": "get_token_allowance",
            "description": "Check ERC20 allowance between owner and spender.",
            "inputSchema": {"type": "object", "properties": {"tokenAddress": {"type": "string"}, "ownerAddress": {"type": "string"}, "spenderAddress": {"type": "string"}, "chain_id": {"type": "string"}, "network": {"type": "string"}}, "required": ["tokenAddress", "ownerAddress", "spenderAddress"], "additionalProperties": false}
        },
        {
            "name": "transfer_token",
            "description": "Transfer ERC20 tokens.",
            "inputSchema": {"type": "object", "properties": {"private_key": {"type": "string"}, "tokenAddress": {"type": "string"}, "toAddress": {"type": "string"}, "amount": {"type": "string"}, "chain_id": {"type": "string"}, "network": {"type": "string"}, "gas_limit": {"type": "string"}, "gas_price": {"type": "string"}}, "required": ["private_key", "tokenAddress", "toAddress", "amount"]}
        },
        {
            "name": "approve_token_spending",
            "description": "Approve ERC20 allowances.",
            "inputSchema": {"type": "object", "properties": {"private_key": {"type": "string"}, "tokenAddress": {"type": "string"}, "spenderAddress": {"type": "string"}, "amount": {"type": "string"}, "chain_id": {"type": "string"}, "network": {"type": "string"}, "gas_limit": {"type": "string"}, "gas_price": {"type": "string"}}, "required": ["private_key", "tokenAddress", "spenderAddress", "amount"]}
        },
        // --- Added: ERC721 ---
        {
            "name": "get_nft_info",
            "description": "Get ERC721 token metadata (tokenURI).",
            "inputSchema": {"type": "object", "properties": {"tokenAddress": {"type": "string"}, "tokenId": {"type": "string"}, "chain_id": {"type": "string"}, "network": {"type": "string"}}, "required": ["tokenAddress", "tokenId"]}
        },
        {
            "name": "check_nft_ownership",
            "description": "Verify ERC721 NFT ownership (ownerOf).",
            "inputSchema": {"type": "object", "properties": {"tokenAddress": {"type": "string"}, "tokenId": {"type": "string"}, "ownerAddress": {"type": "string"}, "chain_id": {"type": "string"}, "network": {"type": "string"}}, "required": ["tokenAddress", "tokenId", "ownerAddress"]}
        },
        {
            "name": "get_nft_balance",
            "description": "Count ERC721 NFTs owned (balanceOf).",
            "inputSchema": {"type": "object", "properties": {"tokenAddress": {"type": "string"}, "ownerAddress": {"type": "string"}, "chain_id": {"type": "string"}, "network": {"type": "string"}}, "required": ["tokenAddress", "ownerAddress"]}
        },
        // --- Added: ERC1155 ---
        {
            "name": "get_erc1155_token_uri",
            "description": "Get ERC1155 token URI.",
            "inputSchema": {"type": "object", "properties": {"tokenAddress": {"type": "string"}, "tokenId": {"type": "string"}, "chain_id": {"type": "string"}, "network": {"type": "string"}}, "required": ["tokenAddress", "tokenId"]}
        },
        {
            "name": "get_erc1155_balance",
            "description": "Check ERC1155 token balance.",
            "inputSchema": {"type": "object", "properties": {"tokenAddress": {"type": "string"}, "tokenId": {"type": "string"}, "ownerAddress": {"type": "string"}, "chain_id": {"type": "string"}, "network": {"type": "string"}}, "required": ["tokenAddress", "tokenId", "ownerAddress"]}
        },
        {
            "name": "transfer_erc1155",
            "description": "Transfer ERC1155 tokens (safeTransferFrom).",
            "inputSchema": {"type": "object", "properties": {"private_key": {"type": "string"}, "tokenAddress": {"type": "string"}, "fromAddress": {"type": "string"}, "toAddress": {"type": "string"}, "tokenId": {"type": "string"}, "amount": {"type": "string"}, "chain_id": {"type": "string"}, "network": {"type": "string"}, "gas_limit": {"type": "string"}, "gas_price": {"type": "string"}}, "required": ["private_key", "tokenAddress", "fromAddress", "toAddress", "tokenId", "amount"]}
        },
        // --- Added: contract utils ---
        {
            "name": "is_contract",
            "description": "Check if an address is a verified contract on Etherscan.",
            "inputSchema": {"type": "object", "properties": {"address": {"type": "string"}, "chain_id": {"type": "string"}, "network": {"type": "string"}}, "required": ["address"]}
        },
        {
            "name": "read_contract",
            "description": "Read a contract function via ABI (eth_call).",
            "inputSchema": {"type": "object", "properties": {"contractAddress": {"type": "string"}, "abi": {"type": "string"}, "functionName": {"type": "string"}, "args": {"type": "array"}, "chain_id": {"type": "string"}, "network": {"type": "string"}}, "required": ["contractAddress", "abi", "functionName"]}
        },
        {
            "name": "write_contract",
            "description": "Write to a contract via ABI (signed tx).",
            "inputSchema": {"type": "object", "properties": {"private_key": {"type": "string"}, "contractAddress": {"type": "string"}, "abi": {"type": "string"}, "functionName": {"type": "string"}, "args": {"type": "array"}, "chain_id": {"type": "string"}, "network": {"type": "string"}, "gas_limit": {"type": "string"}, "gas_price": {"type": "string"}}, "required": ["private_key", "contractAddress", "abi", "functionName"]}
        },
    ]);
    Response::success(req.id.clone(), json!({ "tools": tools }))
}
