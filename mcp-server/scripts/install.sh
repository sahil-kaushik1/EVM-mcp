#!/usr/bin/env bash
set -euo pipefail

# Simple installer for the EVM MCP Server
# - Builds the release binary
# - Creates .env from env.example if missing
# - Prints run instructions for HTTP and MCP modes

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")"/.. && pwd)"
cd "$ROOT_DIR"

if ! command -v cargo >/dev/null 2>&1; then
  echo "Error: cargo (Rust) is not installed. Install Rust from https://rustup.rs/" >&2
  exit 1
fi

echo "[1/3] Building release binary..."
cargo build --release
BIN_PATH="$ROOT_DIR/target/release/evm_mcp"
if [ ! -f "$BIN_PATH" ]; then
  echo "Build failed: binary not found at $BIN_PATH" >&2
  exit 1
fi

echo "[2/3] Preparing environment file..."
if [ ! -f "$ROOT_DIR/.env" ]; then
  if [ -f "$ROOT_DIR/env.example" ]; then
    cp "$ROOT_DIR/env.example" "$ROOT_DIR/.env"
    echo "Created $ROOT_DIR/.env from env.example"
  else
    cat > "$ROOT_DIR/.env" <<'EOF'
# Minimum required configuration
# CHAIN_RPC_URLS is a JSON object: {"1":"https://mainnet.infura.io/v3/YOUR_KEY","11155111":"https://sepolia.infura.io/v3/YOUR_KEY"}
CHAIN_RPC_URLS=
PORT=8080
NATIVE_DENOM=wei
# Optional
# TX_PRIVATE_KEY=
# DEFAULT_SENDER_ADDRESS=
# DEFAULT_GAS_LIMIT=300000
# DEFAULT_GAS_PRICE=20000000000
EOF
    echo "Created $ROOT_DIR/.env"
  fi
else
  echo ".env already exists; leaving as-is"
fi

cat <<EOF
[3/3] Done.

Binary: $BIN_PATH

Run (HTTP server):
  cd "$ROOT_DIR" && PORT=8080 "$BIN_PATH"

Run (MCP mode over stdin/stdout):
  cd "$ROOT_DIR" && "$BIN_PATH" --mcp

Environment:
  Edit $ROOT_DIR/.env and set CHAIN_RPC_URLS and other values as needed.

Systemd (optional):
  Create /etc/systemd/system/evm-mcp.service with:

  [Unit]
  Description=EVM MCP Server
  After=network.target

  [Service]
  Type=simple
  WorkingDirectory=$ROOT_DIR
  EnvironmentFile=$ROOT_DIR/.env
  ExecStart=$BIN_PATH
  Restart=on-failure

  [Install]
  WantedBy=multi-user.target

Then:
  sudo systemctl daemon-reload && sudo systemctl enable --now evm-mcp

EOF
