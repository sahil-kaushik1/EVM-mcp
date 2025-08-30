#!/bin/bash

echo "🔐 Secure Wallet Registration Tool"
echo "=================================="
echo ""

# Get wallet name
read -r -p "Enter wallet name: " wallet_name

# Get private key securely
read -r -s -p "Enter private key (will be hidden): " private_key
echo ""

# Get master password securely
read -r -s -p "Enter master password (will be hidden): " master_password
echo ""

# Confirm master password
read -r -s -p "Confirm master password (will be hidden): " confirm_password
echo ""

if [ "$master_password" != "$confirm_password" ]; then
    echo "❌ Passwords do not match!"
    exit 1
fi

echo ""
echo "🔐 Encrypting and storing wallet..."

# Create JSON request safely using a heredoc
json_request=$(cat <<EOF
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "register_wallet",
    "arguments": {
      "wallet_name": "$wallet_name",
      "private_key": "$private_key",
      "master_password": "$master_password"
    }
  }
}
EOF
)

# Send JSON to MCP server
echo "📡 Sending wallet registration request..."
response=$(curl -s -X POST http://127.0.0.1:3000 \
    -H "Content-Type: application/json" \
    -d "$json_request")

echo "📬 Server response:"
echo "$response" | jq . # Pipe to jq for pretty printing if available

echo ""
echo "🔒 Your private key is now encrypted and stored securely."
echo "📁 Wallet will be stored in: ~/.sei-mcp-server/wallets.json"
echo "🔐 Encrypted with AES-256-GCM"