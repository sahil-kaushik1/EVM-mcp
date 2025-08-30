# EVM Sorcerer Frontend

An authentic recreation of the sei-sorcerer UI with advanced WebGL fluid simulation, Together AI Meta Llama integration, and full MCP server tool support.

## Features

- **Full-Screen Fluid Simulation**: Advanced WebGL-based fluid dynamics covering the entire viewport with realistic, interactive color effects based on mouse movement
- **Clean Pixel Design**: Modern pixel-art inspired UI with clean typography, sharp borders, and retro styling
- **White Background Theme**: Crisp white background with black borders and pixel-perfect shadows
- **AI Chat Interface**: Powered by Together AI Meta Llama model with blockchain expertise
- **MCP Tool Integration**: AI agent can use all MCP server tools for comprehensive blockchain operations
- **Real-time Tool Execution**: Seamless tool calls executed via MCP server with results displayed in chat
- **Courier New Typography**: Classic monospace font for that authentic pixel/terminal aesthetic
- **Responsive Design**: Works beautifully on all screen sizes with pixel-perfect scaling
- **Professional Animations**: Smooth transitions and typing indicators

## Setup

1. **Install dependencies**:
   ```bash
   npm install
   ```

2. **Environment Configuration**:
   ```bash
   cp .env.example .env
   # Edit .env with your Together AI API key
   ```

3. **Get Together AI API Key**:
   - Sign up at [Together AI](https://together.ai)
   - Get your API key and add it to `.env`:
   ```
   TOGETHER_API_KEY=your_api_key_here
   ```

## Running the Application

### Development Mode (Frontend + Backend)
```bash
npm run dev
```
This runs both the React frontend (port 3002) and Express backend (port 3001) concurrently.

### Production Build
```bash
npm run build
npm run server
```

### Individual Services
```bash
# Frontend only (port 3002)
npm start

# Backend only (port 3001)
npm run server
```

## Current Status

✅ **Fully Functional**: The application compiles and runs successfully with:
- Advanced WebGL fluid simulation from sei-sorcerer
- Together AI Meta Llama integration
- Complete MCP server tool support
- Responsive UI with magical theming

## MCP Server Integration

The frontend connects to the MCP server at `http://localhost:8080`. Make sure the MCP server is running:

```bash
cd ../mcp-server
cargo run
```

## Available MCP Tools

The AI agent can use these tools:

- **get_balance**: Check EVM address balance
- **get_transaction_history**: Get transaction history
- **send_transaction**: Send blockchain transactions
- **get_contract_info**: Get smart contract information

## Usage

1. Start both the MCP server and frontend
2. Move your mouse to see the dynamic color background
3. Chat with the AI agent about blockchain operations
4. The AI will automatically use MCP tools when needed

## Architecture

```
Frontend (React) ←→ Backend (Express) ←→ Together AI API
                        ↓
                   MCP Server (Rust)
                        ↓
                 EVM Blockchains
```

## Technologies

- **Frontend**: React, TypeScript-style components
- **Backend**: Express.js, Axios
- **AI**: Together AI Meta Llama
- **Blockchain**: MCP server integration
- **Graphics**: WebGL fluid simulation
- **Styling**: Tailwind CSS with custom design tokens
- **Animation**: Framer Motion, custom WebGL shaders
