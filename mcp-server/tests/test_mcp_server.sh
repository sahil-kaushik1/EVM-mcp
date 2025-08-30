#!/bin/bash

# This script tests the basic functionality of the MCP server.
# It starts the server, sends initialize and tools/list requests,
# and checks for valid responses.

echo "🚀 Starting MCP Server Test..."

# 1. Start the MCP server in the background
# Ensure the project is built in release mode first
cargo build --release
./target/release/seiyn_mcp --mcp &
SERVER_PID=$!

# Give the server a moment to start up
sleep 2

# 2. Create a temporary file for server output
OUTPUT_FILE=$(mktemp)

# 3. Send requests and capture output
(
    # Send initialize request
    echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}'
    sleep 1 # Wait for a response

    # Send tools/list request
    echo '{"jsonrpc":"2.0","id":2,"method":"tools/list"}'
    sleep 1 # Wait for a response

) | nc localhost 8080 > "$OUTPUT_FILE" &

# Allow time for communication
sleep 3

# 4. Kill the server and the netcat process
kill $SERVER_PID
kill %1

echo "🔍 Analyzing server responses..."

# 5. Check the initialize response
if grep -q '"name":"seiyn_mcp"' "$OUTPUT_FILE"; then
    echo "✅ Test Passed: 'initialize' method returned correct server info."
else
    echo "❌ Test Failed: Did not receive a valid 'initialize' response."
    cat "$OUTPUT_FILE"
    exit 1
fi

# 6. Check the tools/list response
if grep -q '"name":"get_balance"' "$OUTPUT_FILE" && grep -q '"name":"transfer_sei"' "$OUTPUT_FILE"; then
    echo "✅ Test Passed: 'tools/list' method returned a list of tools."
else
    echo "❌ Test Failed: Did not receive a valid 'tools/list' response."
    cat "$OUTPUT_FILE"
    exit 1
fi

# 7. Clean up
rm "$OUTPUT_FILE"

echo "🎉 All MCP server basic tests passed!"
exit 0
