# Sei MCP Server (binary: `seiyn_mcp`)

Rust-based HTTP + MCP server for interacting with Sei (Cosmos/EVM) networks. It exposes:

- HTTP REST API (Axum) for wallet, balance, faucet, contract inspection, and SeiStream-like queries
- MCP (Model Context Protocol) over stdin/stdout for tool integration; also bridged via HTTP `/rpc`

Source layout:
- `src/main.rs` — boots HTTP server or MCP server depending on flags
- `src/lib.rs` — `AppState` and module exports
- `src/config.rs` — environment-driven configuration
- `src/api/` — HTTP route handlers (balance, faucet, wallet, contracts, etc.)
- `src/blockchain/` — chain clients, services, nonce manager
- `src/mcp/` — MCP protocol, handler, wallet storage

## Quick start

1) Copy and edit environment

```bash
cp env.example .env
# Edit .env with your values (see "Configuration")
```

2) Build and run HTTP server (default)

```bash
cargo run
# or specify a port
PORT=8080 cargo run
```

3) Run in MCP mode (stdin/stdout)

```bash
cargo run -- --mcp
# or
MCP_MODE=1 cargo run
```

4) Build release

```bash
cargo build --release
```

## Configuration

Configuration is loaded from environment variables in `.env` via `Config::from_env()` (`src/config.rs`). Required/important keys:

- CHAIN_RPC_URLS (required): JSON map of `chain_id -> RPC URL`.
  Example:
  ```json
  {"sei-evm-testnet":"https://evm-rpc-testnet.sei-apis.com","atlantic-2":"https://rpc-testnet.sei-apis.com","sei-evm-mainnet":"https://evm-rpc.sei-apis.com","pacific-1":"https://sei-rpc.polkachu.com"}
  ```
- FAUCET_API_URL (required): Base URL of faucet HTTP service the server proxies to.
- PORT (optional, default 8080): HTTP server port.
- WEBSOCKET_URL (optional): Websocket endpoint if needed by clients/services.
- DISCORD_API_URL (optional): External Discord API base URL to proxy to.
- TX_PRIVATE_KEY_EVM (optional): EVM private key used for non-faucet transaction paths.
  - Fallbacks: FAUCET_PRIVATE_KEY_EVM, FAUCET_PRIVATE_KEY.
- DEFAULT_SENDER_ADDRESS (optional): Default address for transactions.
  - Fallback: FAUCET_ADDRESS.
- NATIVE_DENOM (optional, default `usei`). Fallback: FAUCET_DENOM.
- NATIVE_GAS_LIMIT (optional, default `200000`). Fallback: FAUCET_GAS_LIMIT.
- NATIVE_FEE_AMOUNT (optional, default `5000`). Fallback: FAUCET_FEE_AMOUNT.
- NATIVE_CHAIN_ID (optional, default `atlantic-2`).
- NATIVE_BECH32_HRP (optional, default `sei`).
- Discord (all optional): DISCORD_WEBHOOK_URL, DISCORD_BOT_TOKEN, DISCORD_CHANNEL_ID.

See `env.example` for a reference template.

## HTTP API

Defined in `src/main.rs` with Axum routes.

- GET `/api/health`
- POST `/api/wallet/create`
- POST `/api/wallet/import`
- GET `/api/balance/:chain_id/:address`
- GET `/api/history/:chain_id/:address`
- GET `/contract/:chain_id/:address`
- GET `/contract/:chain_id/:address/code`
- GET `/contract/:chain_id/:address/transactions`
- GET `/contract/:chain_id/:address/is_contract`
- POST `/api/discord/post`
- GET `/redirect/seidocs`
- POST `/api/faucet/request`
- POST `/api/tx/send`
- GET `/api/chain/network`
- GET `/api/transactions/evm/:hash`
- GET `/api/accounts/evm/:address/transactions`
- GET `/api/tokens/evm/erc721/:address/items`
- POST `/rpc` — JSON-RPC endpoint that forwards MCP tool calls over HTTP

Notes:
- Server binds to `127.0.0.1:PORT` and enables permissive CORS and HTTP tracing.

### Curl examples

Health:
```bash
curl -s http://127.0.0.1:8080/api/health
```

Balance:
```bash
curl -s http://127.0.0.1:8080/api/balance/atlantic-2/<sei_or_evm_address>
```

Faucet request:
```bash
curl -X POST http://127.0.0.1:8080/api/faucet/request \
  -H 'Content-Type: application/json' \
  -d '{"chain_id":"atlantic-2","address":"<address>","amount": "100000"}'
```

MCP over HTTP `/rpc`:
```bash
curl -X POST http://127.0.0.1:8080/rpc \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"<mcp_method>","params":{}}'
```

## MCP integration

- Native MCP mode: start with `--mcp` or `MCP_MODE=1` to serve requests over stdin/stdout.
- Example VS Code MCP client configuration in `mcp.json`:

```json
{
  "mcpServers": {
    "sei-mcp-release": {
      "command": "PATH_TO_REPO/sei-mcp/mcp-server/target/release/seiyn_mcp",
      "args": ["--mcp"],
      "env": {
        "CHAIN_RPC_URLS": "{...}",
        "FAUCET_API_URL": "https://sei-mcp.onrender.com",
        "DISCORD_API_URL": "https://sei-mcp-tdj3.onrender.com"
      }
    }
  }
}
```

## Wallet storage

Wallet material is handled by the MCP wallet storage module and persisted on disk at a path derived by `get_wallet_storage_path()`. Storage is initialized on first wallet registration/import.

## Development

- Run tests:
  ```bash
  cargo test
  ```
- Useful scripts:
  - `tests/test_mcp_server.sh`
  - `tests/test_persistent_wallet.sh`

## Dependencies

Key crates (see `Cargo.toml`):
- Web: `axum`, `tokio`, `tower`, `tower-http`
- Serialization: `serde`, `serde_json`, `chrono`, `uuid`
- Blockchain: `ethers-*`, `cosmrs`, `cosmos-sdk-proto`, `tendermint-rpc`
- Crypto/keys: `bip39`, `bip32`, `k256`, `aes-gcm`, `argon2`, `bech32`, `sha2`, `ripemd`, `secrecy`, `zeroize`
- Utils: `reqwest`, `tracing`, `tracing-subscriber`, `dashmap`, `anyhow`, `thiserror`

## License

MIT (or project default).
