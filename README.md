# EVM MCP Monorepo

This repo contains:

- frontend/: React app (Create React App) deployed on Vercel with Serverless API routes
- mcp-server/: Rust Axum server exposing EVM blockchain tools over HTTP and MCP (stdin/stdout)

The frontend talks to LLM providers (OpenRouter/Groq) and optionally invokes blockchain tools via the MCP server through serverless API routes under `frontend/api/`.

## Architecture

- `frontend/src/App.js` renders the UI and calls relative API endpoints `/api/*`.
- `frontend/api/*` are Vercel Serverless Functions that:
  - `/api/chat`: invokes LLM, optionally calls MCP tools via JSON-RPC
  - `/api/mcp/health`: health check via JSON-RPC to MCP
  - `/api/mcp/tools`: list tools from MCP
  - `/api/mcp/call`: generic JSON-RPC proxy
- `mcp-server/` is a standalone HTTP server with routes described in `mcp-server/README.md`, and a JSON-RPC bridge at `/api/rpc`.

Note: Vercel cannot host the Rust long-running server; deploy it separately (Render/Railway/Fly/etc.) and set `MCP_SERVER_URL` in Vercel env.

## Local Development

Requirements:
- Node 18+
- Rust (rustup) for the MCP server

Steps:

1) Start MCP server
```bash
cd mcp-server
cp env.example .env  # edit CHAIN_RPC_URLS
PORT=8080 cargo run
```

2) Start frontend (serverless functions run locally via Vercel dev or use CRA dev)
- CRA dev only (API calls expect serverless routes):
  - Use `vercel dev` at repo root (recommended), or
  - Start CRA dev and rely on absolute URLs if testing locally.

```bash
cd frontend
npm install
npm run start
# If using serverless locally, run `vercel dev` at repo root to serve /api/*
```

## Deployments

### MCP server (Render)

- Follow `mcp-server/README.md` for Render deployment.
- After deploy, obtain base URL (e.g., `https://your-service.onrender.com`).

### Frontend (Vercel)

- Import this repository in Vercel.
- Project settings:
  - Root Directory: `frontend/`
  - Build Command: `npm run build`
  - Output Directory: `build`
- Environment Variables:
  - `OPENROUTER_API_KEY` (required for OpenRouter)
  - `OPENROUTER_SITE_URL` = `https://<your-project>.vercel.app`
  - `OPENROUTER_APP_NAME` (optional)
  - `LLM_PROVIDER` = `openrouter` or `groq`
  - `GROQ_API_KEY`, `GROQ_MODEL` (if using Groq)
  - `SEND_TOOLS` = `true` to send the tools schema to the model (if supported)
  - `MCP_SERVER_URL` = `https://your-service.onrender.com` (public MCP server URL)

Deploy, then test:
- `GET https://<your-project>.vercel.app/api/health`
- `GET https://<your-project>.vercel.app/api/mcp/health`
- Use the app UI at the root URL.

## CI/CD

- `.github/workflows/rust-release.yml`:
  - On tag push `v*`, builds `mcp-server` and attaches Linux AMD64 binary to the GitHub release.
- `.github/workflows/render-keepalive.yml`:
  - Every 10 minutes, pings `RENDER_PING_URL` (set as repo secret) to keep the Render instance awake.

## Install script (MCP)

- `mcp-server/scripts/install.sh` builds the release binary, prepares `.env`, and prints run instructions.

## Env Vars (MCP)

See `mcp-server/README.md`. Minimum is `CHAIN_RPC_URLS` JSON map.

## Notes

- The legacy `frontend/server.js` Express server is replaced by serverless functions for Vercel. You may keep it for local experimentation but it is not used on Vercel.
- Ensure you never commit secrets. Configure environment variables in Vercel/Render dashboards or GitHub secrets as needed.
