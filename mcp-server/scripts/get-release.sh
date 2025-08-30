#!/usr/bin/env bash
set -euo pipefail

# Download and install the latest (or specified) release binary for EVM MCP Server
# Usage:
#   bash scripts/get-release.sh [TAG]
# Options:
#   TAG: Optional Git tag like v0.1.0. If omitted, uses latest release.
# Env:
#   GITHUB_REPO: owner/repo override. Auto-detected from git remote if not set.
#   INSTALL_DIR: where to place the binary (default: ~/.local/bin)

TAG="${1:-}"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

# Determine repo
if [[ -z "${GITHUB_REPO:-}" ]]; then
  if git remote -v >/dev/null 2>&1; then
    # Try to infer from origin URL
    ORIGIN_URL=$(git remote get-url origin 2>/dev/null || echo "")
    case "$ORIGIN_URL" in
      git@github.com:*) GITHUB_REPO=${ORIGIN_URL#git@github.com:}; GITHUB_REPO=${GITHUB_REPO%.git} ;;
      https://github.com/*) GITHUB_REPO=${ORIGIN_URL#https://github.com/}; GITHUB_REPO=${GITHUB_REPO%.git} ;;
      *) GITHUB_REPO="" ;;
    esac
  fi
fi

if [[ -z "${GITHUB_REPO:-}" ]]; then
  echo "Error: Could not determine GITHUB_REPO. Set env GITHUB_REPO=owner/repo and re-run." >&2
  exit 1
fi

# Detect platform target
uname_s=$(uname -s)
uname_m=$(uname -m)
case "$uname_s-$uname_m" in
  Linux-x86_64)   TARGET_NAME=linux-amd64 ; EXT=tar.gz ;;
  Darwin-x86_64)  TARGET_NAME=macos-amd64 ; EXT=tar.gz ;;
  Darwin-arm64)   TARGET_NAME=macos-arm64 ; EXT=tar.gz ;;
  *) echo "Unsupported platform: $uname_s $uname_m" >&2; exit 1 ;;
esac

API_BASE="https://api.github.com/repos/$GITHUB_REPO/releases"
if [[ -n "$TAG" ]]; then
  API_URL="$API_BASE/tags/$TAG"
else
  API_URL="$API_BASE/latest"
fi

echo "Fetching release metadata from: $API_URL"
JSON=$(curl -fsSL "$API_URL")

# Find asset by name
ASSET_NAME="evm_mcp-${TARGET_NAME}.${EXT}"
ASSET_URL=$(echo "$JSON" | grep -E '"browser_download_url":' | sed -E 's/.*"browser_download_url"\s*:\s*"([^"]+)".*/\1/' | grep "$ASSET_NAME" || true)
if [[ -z "$ASSET_URL" ]]; then
  echo "Error: Asset $ASSET_NAME not found in release." >&2
  echo "Available assets:" >&2
  echo "$JSON" | grep -E '"name":' | sed -E 's/.*"name"\s*:\s*"([^"]+)".*/\1/' >&2
  exit 1
fi

echo "Downloading: $ASSET_URL"
TMP_DIR=$(mktemp -d)
TARBALL="$TMP_DIR/$ASSET_NAME"
curl -fL "$ASSET_URL" -o "$TARBALL"

# Verify checksum if present
SHA_URL=$(echo "$JSON" | grep -E '"browser_download_url":' | sed -E 's/.*"browser_download_url"\s*:\s*"([^"]+)".*/\1/' | grep "${TARGET_NAME}\.sha256" || true)
if [[ -n "$SHA_URL" ]]; then
  echo "Fetching checksum: $SHA_URL"
  curl -fsSL "$SHA_URL" -o "$TMP_DIR/${TARGET_NAME}.sha256"
  (cd "$TMP_DIR" && sha256sum -c "${TARGET_NAME}.sha256" 2>/dev/null | grep ": OK" ) || echo "Warning: checksum verification skipped/failed"
else
  echo "No checksum found; skipping verification"
fi

# Extract and install
mkdir -p "$INSTALL_DIR"
case "$EXT" in
  tar.gz)
    tar -xzf "$TARBALL" -C "$TMP_DIR"
    if [[ ! -f "$TMP_DIR/evm_mcp" ]]; then
      echo "Error: evm_mcp binary not found inside archive" >&2
      exit 1
    fi
    mv "$TMP_DIR/evm_mcp" "$INSTALL_DIR/evm_mcp"
    chmod +x "$INSTALL_DIR/evm_mcp"
    ;;
  *) echo "Unsupported archive extension: $EXT" >&2; exit 1 ;;
endcase

echo "Installed to: $INSTALL_DIR/evm_mcp"
case ":$PATH:" in
  *:"$INSTALL_DIR":*) ;;
  *) echo "Note: $INSTALL_DIR is not in PATH. Add it to your shell profile." ;;
esac

echo "Done. Run: evm_mcp --help"
