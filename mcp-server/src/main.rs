// src/main.rs

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use evm_mcp_server::{
    api::{
        balance::get_balance_handler,
        contract::{
            get_contract_code_handler, get_contract_handler, get_contract_transactions_handler,
            get_is_contract_handler,
        },
        health::health_handler,
        history::get_transaction_history_handler,
        tx::send_transaction_handler,
        wallet,
    },
    blockchain::{
        client::EvmClient,
        nonce_manager::NonceManager,
        wallet_manager::WalletManager,
    },
    config::Config,
    mcp::{
        handler::handle_mcp_request,
        protocol::{error_codes, Request, Response},
        wallet_storage::{get_wallet_storage_path, WalletStorage},
    },
    AppState,
};
use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt};
use tokio::sync::Mutex;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{debug, error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// --- HTTP Server Logic ---
async fn run_http_server(state: AppState) {
    // Create the API router with all routes
    let api_router = Router::new()
        // Health check
        .route("/health", get(health_handler))
        
        // Wallet management
        .merge(wallet::create_wallet_router())
        
        // Blockchain data
        .route("/balance/:chain_id/:address", get(get_balance_handler))
        .route(
            "/history/:chain_id/:address",
            get(get_transaction_history_handler),
        )
        .route("/tx/send", post(send_transaction_handler))
        
        // Contract interaction
        .route("/contract/:chain_id/:address", get(get_contract_handler))
        .route(
            "/contract/:chain_id/:address/code",
            get(get_contract_code_handler),
        )
        .route(
            "/contract/:chain_id/:address/transactions",
            get(get_contract_transactions_handler),
        )
        .route(
            "/contract/:chain_id/:address/is_contract",
            get(get_is_contract_handler),
        )
        
        // JSON-RPC endpoint for MCP tool calls
        .route("/rpc", post(rpc_handler));

    // Create the main app with the API router under /api
    let app = Router::new()
        .nest("/api", api_router)
        .with_state(state.clone())
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive());

    let addr = SocketAddr::from(([127, 0, 0, 1], state.config.port));
    info!("🚀 HTTP Server listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await.unwrap();
}

// Forward JSON-RPC requests over HTTP to the MCP handler
async fn rpc_handler(
    State(state): State<AppState>,
    Json(req): Json<Request>,
 ) -> Json<Response> {
    match handle_mcp_request(req, state).await {
        Some(resp) => Json(resp),
        None => Json(Response::error(
            serde_json::Value::Null,
            error_codes::INVALID_REQUEST,
            "Notifications are not supported over HTTP".into(),
        )),
    }
}

// --- MCP Server Logic ---
async fn run_mcp_server(state: AppState) {
    info!("🚀 Starting MCP server on stdin/stdout...");

    let mut stdin = io::BufReader::new(io::stdin());
    let mut stdout = io::stdout();

    loop {
        let mut line = String::new();

        match stdin.read_line(&mut line).await {
            Ok(0) => {
                info!("EOF received, shutting down MCP server");
                break;
            }
            Ok(_) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                debug!("Received: {}", line);

                let response = match serde_json::from_str::<Request>(line) {
                    Ok(request) => handle_mcp_request(request, state.clone()).await,
                    Err(parse_error) => {
                        error!("JSON parse error: {}", parse_error);
                        Some(Response::error(
                            serde_json::Value::Null,
                            error_codes::PARSE_ERROR,
                            format!("Parse error: {}", parse_error),
                        ))
                    }
                };

                if let Some(response) = response {
                    if let Ok(response_json) = serde_json::to_string(&response) {
                        debug!("Sending: {}", response_json);
                        if let Err(e) = stdout
                            .write_all(format!("{}\n", response_json).as_bytes())
                            .await
                        {
                            error!("Failed to write response: {}", e);
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                error!("Failed to read from stdin: {}", e);
                break;
            }
        }
    }

    info!("MCP server shutting down");
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "evm_mcp_server=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    // Load configuration
    let config = match Config::from_env() {
        Ok(cfg) => cfg,
        Err(e) => {
            error!("❌ Failed to load configuration: {}", e);
            return;
        }
    };

    // Initialize EVM client with RPC URLs
    let evm_client = match EvmClient::new(&config.chain_rpc_urls) {
        Ok(client) => client,
        Err(e) => {
            error!("❌ Failed to initialize EVM client: {}", e);
            return;
        }
    };
    
    let nonce_manager = NonceManager::new();

    // Initialize wallet storage
    let wallet_storage_path = config.wallet_storage_path.clone()
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            // Default to a path in the user's home directory if not specified in config
            let mut path = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
            path.push(".evm-mcp");
            path.push("wallets.json");
            path
        });

    // Create parent directory if it doesn't exist
    if let Some(parent) = wallet_storage_path.parent() {
        if !parent.exists() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                error!("Failed to create wallet storage directory: {}", e);
                return;
            }
        }
    }

    // Create or load wallet storage
    let wallet_storage = match WalletStorage::load_or_create(wallet_storage_path, &config.master_password) {
        Ok(storage) => storage,
        Err(e) => {
            error!("Failed to initialize wallet storage: {}", e);
            return;
        }
    };
    
    info!("Wallet storage initialized at: {}", wallet_storage.storage_path.display());

    // Create wallet manager
    let wallet_manager = WalletManager::new(wallet_storage.clone());

    // Create app state
    let app_state = AppState {
        config,
        evm_client,
        nonce_manager,
        wallet_manager,
        wallet_storage: Arc::new(Mutex::new(wallet_storage)),
        wallet_storage_path: Arc::new(wallet_storage_path),
    };

    // Check if running in MCP mode (stdin/stdout) or HTTP server mode
    let args: Vec<String> = env::args().collect();
    if args.contains(&"--mcp".to_string()) || env::var("MCP_MODE").is_ok() {
        run_mcp_server(app_state).await;
    } else {
        run_http_server(app_state).await;
    }
}
