const axios = require('axios');

// Environment/config
const OPENROUTER_API_URL = 'https://openrouter.ai/api/v1/chat/completions';
const OPENROUTER_API_KEY = process.env.OPENROUTER_API_KEY;
const OPENROUTER_MODEL = process.env.OPENROUTER_MODEL || 'meta-llama/llama-3.3-8b-instruct:free';
const OPENROUTER_SITE_URL = process.env.OPENROUTER_SITE_URL || 'http://localhost:3000';
const OPENROUTER_APP_NAME = process.env.OPENROUTER_APP_NAME || 'EVM Sorcerer';

const GROQ_API_URL = 'https://api.groq.com/openai/v1/chat/completions';
const GROQ_API_KEY = process.env.GROQ_API_KEY;
const GROQ_MODEL = process.env.GROQ_MODEL || 'llama3-8b-8192';

const SEND_TOOLS = String(process.env.SEND_TOOLS || 'false').toLowerCase() === 'true';
const LLM_PROVIDER = (process.env.LLM_PROVIDER || (GROQ_API_KEY ? 'groq' : 'openrouter')).toLowerCase();
const MCP_SERVER_URL = process.env.MCP_SERVER_URL || 'https://evm-mcp.onrender.com';

function getLlmConfig() {
  if (LLM_PROVIDER === 'groq') {
    return {
      provider: 'groq',
      url: GROQ_API_URL,
      model: GROQ_MODEL,
      headers: {
        Authorization: `Bearer ${GROQ_API_KEY}`,
        'Content-Type': 'application/json',
      },
    };
  }
  return {
    provider: 'openrouter',
    url: OPENROUTER_API_URL,
    model: OPENROUTER_MODEL,
    headers: {
      Authorization: `Bearer ${OPENROUTER_API_KEY}`,
      'Content-Type': 'application/json',
      'HTTP-Referer': OPENROUTER_SITE_URL,
      'X-Title': OPENROUTER_APP_NAME,
    },
  };
}

const MCP_TOOLS = [
  { name: 'get_balance', description: 'Get the balance of an EVM address', parameters: { type: 'object', properties: { chain_id: { type: 'string' }, address: { type: 'string' } }, required: ['chain_id', 'address'] } },
  { name: 'create_wallet', description: 'Create a new EVM wallet with address, private key, and mnemonic', parameters: { type: 'object', properties: {}, additionalProperties: false } },
  { name: 'import_wallet', description: 'Import a wallet from mnemonic or private key', parameters: { type: 'object', properties: { mnemonic_or_private_key: { type: 'string' }, key: { type: 'string' } }, oneOf: [{ required: ['mnemonic_or_private_key'] }, { required: ['key'] }] } },
  { name: 'search_events', description: 'Search EVM log events via Etherscan API', parameters: { type: 'object', properties: { chain_id: { type: 'string' }, contract_address: { type: 'string' }, topic0: { type: 'string' }, from_block: { type: 'string' }, to_block: { type: 'string' } }, required: ['chain_id', 'contract_address'] } },
  { name: 'request_faucet', description: 'Request testnet tokens from faucet', parameters: { type: 'object', properties: { chain_id: { type: 'string' }, address: { type: 'string' } }, required: ['chain_id', 'address'] } },
  { name: 'register_wallet', description: 'Securely store a wallet with encryption', parameters: { type: 'object', properties: { wallet_name: { type: 'string' }, mnemonic_or_private_key: { type: 'string' }, private_key: { type: 'string' }, master_password: { type: 'string' } }, oneOf: [{ required: ['wallet_name', 'mnemonic_or_private_key', 'master_password'] }, { required: ['wallet_name', 'private_key', 'master_password'] }] } },
  { name: 'list_wallets', description: 'List all stored wallets', parameters: { type: 'object', properties: { master_password: { type: 'string' } }, required: ['master_password'] } },
  { name: 'transfer_from_wallet', description: 'Transfer tokens from a stored wallet', parameters: { type: 'object', properties: { wallet_name: { type: 'string' }, chain_id: { type: 'string' }, to_address: { type: 'string' }, amount: { type: 'string' }, master_password: { type: 'string' } }, required: ['wallet_name', 'chain_id', 'to_address', 'amount', 'master_password'] } },
  { name: 'transfer_evm', description: 'Send EVM value transfer using private key', parameters: { type: 'object', properties: { private_key: { type: 'string' }, chain_id: { type: 'string' }, to_address: { type: 'string' }, amount_wei: { type: 'string' }, gas_limit: { type: 'string' }, gas_price: { type: 'string' } }, required: ['private_key', 'chain_id', 'to_address', 'amount_wei'] } },
  { name: 'transfer_nft_evm', description: 'Transfer ERC-721 NFT', parameters: { type: 'object', properties: { private_key: { type: 'string' }, chain_id: { type: 'string' }, contract_address: { type: 'string' }, to_address: { type: 'string' }, token_id: { type: 'string' } }, required: ['private_key', 'chain_id', 'contract_address', 'to_address', 'token_id'] } },
  { name: 'get_contract', description: 'Get verified contract details from Etherscan', parameters: { type: 'object', properties: { address: { type: 'string' }, chain_id: { type: 'string' } }, required: ['address'] } },
  { name: 'get_contract_code', description: 'Get verified contract bytecode from Etherscan', parameters: { type: 'object', properties: { address: { type: 'string' }, chain_id: { type: 'string' } }, required: ['address'] } },
  { name: 'get_contract_transactions', description: 'Get contract transaction history', parameters: { type: 'object', properties: { address: { type: 'string' }, chain_id: { type: 'string' } }, required: ['address'] } },
  { name: 'get_transaction_history', description: 'Get transaction history for any EVM address', parameters: { type: 'object', properties: { address: { type: 'string' }, chain_id: { type: 'string' } }, required: ['address'] } },
  { name: 'get_token_info', description: 'Get ERC20 token metadata', parameters: { type: 'object', properties: { tokenAddress: { type: 'string' }, chain_id: { type: 'string' }, network: { type: 'string' } }, required: ['tokenAddress'] } },
  { name: 'get_token_balance', description: 'Check ERC20 token balance', parameters: { type: 'object', properties: { tokenAddress: { type: 'string' }, ownerAddress: { type: 'string' }, chain_id: { type: 'string' }, network: { type: 'string' } }, required: ['tokenAddress', 'ownerAddress'] } },
  { name: 'transfer_token', description: 'Transfer ERC20 tokens', parameters: { type: 'object', properties: { private_key: { type: 'string' }, tokenAddress: { type: 'string' }, toAddress: { type: 'string' }, amount: { type: 'string' }, chain_id: { type: 'string' }, network: { type: 'string' }, gas_limit: { type: 'string' }, gas_price: { type: 'string' } }, required: ['private_key', 'tokenAddress', 'toAddress', 'amount'] } },
  { name: 'get_nft_info', description: 'Get ERC721 token metadata (tokenURI)', parameters: { type: 'object', properties: { tokenAddress: { type: 'string' }, tokenId: { type: 'string' }, chain_id: { type: 'string' }, network: { type: 'string' } }, required: ['tokenAddress', 'tokenId'] } },
  { name: 'check_nft_ownership', description: 'Verify ERC721 NFT ownership', parameters: { type: 'object', properties: { tokenAddress: { type: 'string' }, tokenId: { type: 'string' }, ownerAddress: { type: 'string' }, chain_id: { type: 'string' }, network: { type: 'string' } }, required: ['tokenAddress', 'tokenId', 'ownerAddress'] } },
  { name: 'get_nft_balance', description: 'Count ERC721 NFTs owned', parameters: { type: 'object', properties: { tokenAddress: { type: 'string' }, ownerAddress: { type: 'string' }, chain_id: { type: 'string' }, network: { type: 'string' } }, required: ['tokenAddress', 'ownerAddress'] } },
  { name: 'is_contract', description: 'Check if address is a verified contract', parameters: { type: 'object', properties: { address: { type: 'string' }, chain_id: { type: 'string' }, network: { type: 'string' } }, required: ['address'] } },
  { name: 'read_contract', description: 'Read contract function via ABI', parameters: { type: 'object', properties: { contractAddress: { type: 'string' }, abi: { type: 'string' }, functionName: { type: 'string' }, args: { type: 'array' }, chain_id: { type: 'string' }, network: { type: 'string' } }, required: ['contractAddress', 'abi', 'functionName'] } },
  { name: 'write_contract', description: 'Write to contract via ABI', parameters: { type: 'object', properties: { private_key: { type: 'string' }, contractAddress: { type: 'string' }, abi: { type: 'string' }, functionName: { type: 'string' }, args: { type: 'array' }, chain_id: { type: 'string' }, network: { type: 'string' }, gas_limit: { type: 'string' }, gas_price: { type: 'string' } }, required: ['private_key', 'contractAddress', 'abi', 'functionName'] } },
  { name: 'get_block_number', description: 'Get current block number', parameters: { type: 'object', properties: { chain_id: { type: 'string' }, network: { type: 'string' } }, additionalProperties: false } },
];

async function executeMCPTool(toolCall) {
  const { name, arguments: args } = toolCall;
  const directMethods = [
    'get_balance',
    'request_faucet',
    'transfer_evm',
    'transfer_nft_evm',
    'search_events',
    'get_contract',
    'get_contract_code',
    'get_contract_transactions',
    'get_transaction_history',
  ];

  try {
    let response;
    if (directMethods.includes(name) || (typeof name === 'string' && name.startsWith('tools/'))) {
      response = await axios.post(`${MCP_SERVER_URL}/api/rpc`, {
        jsonrpc: '2.0',
        id: Date.now(),
        method: name,
        params: args,
      }, { headers: { 'Content-Type': 'application/json' } });
    } else {
      response = await axios.post(`${MCP_SERVER_URL}/api/rpc`, {
        jsonrpc: '2.0',
        id: Date.now(),
        method: 'tools/call',
        params: { name, arguments: args },
      }, { headers: { 'Content-Type': 'application/json' } });
    }
    return response.data.result;
  } catch (error) {
    throw new Error(`MCP tool execution failed: ${error.message}`);
  }
}

function normalizeContent(content) {
  if (typeof content === 'string') return content;
  if (Array.isArray(content)) {
    try {
      return content
        .map((part) => {
          if (typeof part === 'string') return part;
          if (part && typeof part.text === 'string') return part.text;
          if (part && typeof part.content === 'string') return part.content;
          return '';
        })
        .join(' ')
        .trim();
    } catch {
      return '';
    }
  }
  return '';
}

function extractJsonObject(str) {
  if (typeof str !== 'string') return null;
  try { return JSON.parse(str); } catch (_) {}
  const first = str.indexOf('{');
  const last = str.lastIndexOf('}');
  if (first !== -1 && last !== -1 && last > first) {
    const candidate = str.slice(first, last + 1);
    try { return JSON.parse(candidate); } catch (_) {}
  }
  const match = str.match(/```json\n([\s\S]*?)\n```/i);
  if (match && match[1]) {
    try { return JSON.parse(match[1]); } catch (_) {}
  }
  return null;
}

module.exports = async function handler(req, res) {
  if (req.method !== 'POST') {
    res.setHeader('Allow', 'POST');
    return res.status(405).json({ error: 'Method Not Allowed' });
  }

  try {
    const { message, messages } = req.body || {};

    const latestUserText = typeof message === 'string' && message.trim()
      ? message
      : (Array.isArray(messages) ? [...messages].reverse().find((m) => m.role === 'user')?.content : '') || '';

    const addrRe = /(0x[a-fA-F0-9]{40})/;
    const chainRe = /chain\s*(id)?\s*[:#-]?\s*(\d{1,6})|on\s+chain\s+(\d{1,6})/i;
    const balanceHint = /\bbalance\b|\bcheck\s+balance\b|\bhow\s+much\s+(eth|wei)\b/i;
    const addrMatch = latestUserText.match(addrRe);
    const chainMatch = latestUserText.match(chainRe);
    const L = latestUserText.toLowerCase();
    const chainAliases = [
      { re: /(eth(ereum)?\s+mainnet|ethereum\b|\beth\b)/, id: '1' },
      { re: /sepolia\b/, id: '11155111' },
      { re: /(goerli|gorli)\b/, id: '5' },
      { re: /(bsc\b|binance\s+smart\s+chain|bnb\s+chain)/, id: '56' },
      { re: /(polygon\b|matic\b)/, id: '137' },
      { re: /arbitrum\b/, id: '42161' },
      { re: /(optimism\b|op\s+mainnet)/, id: '10' },
      { re: /base\b/, id: '8453' },
      { re: /(avalanche\b|avax\b)/, id: '43114' },
      { re: /fantom\b/, id: '250' },
    ];
    let aliasChainId = null;
    if (!chainMatch) {
      for (const a of chainAliases) { if (a.re.test(L)) { aliasChainId = a.id; break; } }
    }
    const wantsBalance = balanceHint.test(latestUserText) && !!addrMatch && (!!chainMatch || !!aliasChainId);

    if (wantsBalance) {
      const address = addrMatch[1];
      const chain_id = ((chainMatch && (chainMatch[2] || chainMatch[3])) ? (chainMatch[2] || chainMatch[3]) : aliasChainId || '').trim();
      try {
        const result = await executeMCPTool({ name: 'get_balance', arguments: { chain_id, address }, id: 'local-intent' });
        function formatWeiToEth(weiStr) {
          try { const wei = BigInt(weiStr); const base = 1000000000000000000n; const whole = wei / base; const frac = wei % base; if (frac === 0n) return `${whole.toString()} ETH`; const decimals = 6n; const scale = 10n ** decimals; const fracScaled = (frac * scale) / base; const fracStr = fracScaled.toString().padStart(Number(decimals), '0').replace(/0+$/, ''); return `${whole.toString()}.${fracStr} ETH`; } catch { return `${weiStr} wei`; }
        }
        let finalContent;
        if (result?.balance?.amount && result?.balance?.denom === 'wei') {
          finalContent = `Here is the balance of ${address} on chain ${chain_id}: ${formatWeiToEth(result.balance.amount)} (${result.balance.amount} wei).`;
        } else {
          finalContent = `Result: ${JSON.stringify(result || {})}`;
        }
        return res.json({
          response: finalContent,
          toolCalls: [{ id: 'local-intent', function: { name: 'get_balance', arguments: JSON.stringify({ chain_id, address }) } }],
          toolResults: [{ tool_call_id: 'local-intent', result }],
          agent: { usedTools: true, final: true },
        });
      } catch (e) {
        // fall through to LLM
      }
    }

    const conversation = [
      {
        role: 'system',
        content: `You are an AI assistant with comprehensive access to EVM blockchain tools.\n\nAvailable tools: ${MCP_TOOLS.map((t) => t.name).join(', ')}\n\nWhen you need to use a tool, respond with a tool call in the format:\n{"tool_calls": [{"name": "tool_name", "arguments": {...}}]}`,
      },
      ...(Array.isArray(messages) ? messages : []),
    ];

    const toolsForTogether = MCP_TOOLS.map((t) => ({
      type: 'function',
      function: { name: t.name, description: t.description, parameters: t.parameters },
    }));

    let response;
    try {
      const llm = getLlmConfig();
      const primaryBody = { model: llm.model, messages: conversation, max_tokens: 1000, temperature: 0.7 };
      if (SEND_TOOLS) primaryBody.tools = toolsForTogether;
      response = await axios.post(llm.url, primaryBody, { headers: llm.headers });
    } catch (err) {
      const status = err.response?.status;
      const data = err.response?.data;
      const msg = typeof data === 'object' ? JSON.stringify(data) : String(data || '');
      const looksLikeToolIssue = status === 404 || (status === 400 && /tool|function/i.test(msg));
      if (looksLikeToolIssue) {
        const llm = getLlmConfig();
        response = await axios.post(llm.url, { model: llm.model, messages: conversation, max_tokens: 1000, temperature: 0.7 }, { headers: llm.headers });
      } else {
        throw err;
      }
    }

    const aiResponse = response.data.choices[0].message;
    let toolCalls = Array.isArray(aiResponse.tool_calls) ? aiResponse.tool_calls : [];
    if (toolCalls.length === 0) {
      const contentStr = normalizeContent(aiResponse.content);
      const parsed = extractJsonObject(contentStr);
      if (parsed && Array.isArray(parsed.tool_calls)) {
        toolCalls = parsed.tool_calls.map((tc, idx) => ({ id: tc.id || String(idx), function: { name: tc.name || tc.function?.name, arguments: typeof tc.arguments === 'string' ? tc.arguments : JSON.stringify(tc.arguments || {}) } }));
      }
    }

    let toolResults = [];
    if (toolCalls.length > 0) {
      for (const toolCall of toolCalls) {
        try {
          const fn = toolCall.function || {};
          const name = fn.name;
          let args = {};
          try { args = fn.arguments ? JSON.parse(fn.arguments) : {}; } catch { args = {}; }
          const result = await executeMCPTool({ name, arguments: args, id: toolCall.id });
          toolResults.push({ tool_call_id: toolCall.id, result });
        } catch (error) {
          toolResults.push({ tool_call_id: toolCall.id, error: error.message });
        }
      }
    }

    function formatWeiToEth(weiStr) {
      try {
        const wei = BigInt(weiStr);
        const base = 1000000000000000000n;
        const whole = wei / base;
        const frac = wei % base;
        if (frac === 0n) return `${whole.toString()} ETH`;
        const decimals = 6n;
        const scale = 10n ** decimals;
        const fracScaled = (frac * scale) / base;
        const fracStr = fracScaled.toString().padStart(Number(decimals), '0').replace(/0+$/, '');
        return `${whole.toString()}.${fracStr} ETH`;
      } catch { return `${weiStr} wei`; }
    }

    function summarizeToolResults(results, calls) {
      try {
        const lines = [];
        for (let i = 0; i < results.length; i++) {
          const tr = results[i];
          if (tr.error) { lines.push(`Tool ${tr.tool_call_id}: error: ${tr.error}`); continue; }
          const r = tr.result || {};
          const matchingCall = Array.isArray(calls) ? calls.find((c) => c.id === tr.tool_call_id) || calls[i] : undefined;
          let chain = r?.debug?.chain_id_normalized || r?.chain_id || (matchingCall ? JSON.parse(matchingCall.function?.arguments || '{}').chain_id : undefined) || '?';
          let address = (matchingCall ? JSON.parse(matchingCall.function?.arguments || '{}').address : undefined) || r?.address || undefined;
          if (r.balance?.amount && r.balance?.denom === 'wei') {
            const eth = formatWeiToEth(r.balance.amount);
            if (address && chain) { lines.push(`Here is the balance of ${address} on chain ${chain}: ${eth} (${r.balance.amount} wei).`); }
            else { lines.push(`Balance: ${eth} (${r.balance.amount} wei).`); }
          } else if (r.message) { lines.push(r.message); }
          else { lines.push(`Result: ${JSON.stringify(r)}`); }
        }
        return lines.join('\n');
      } catch { return JSON.stringify(results); }
    }

    let finalContent = aiResponse.content;
    if (toolResults.length > 0) {
      finalContent = summarizeToolResults(toolResults, toolCalls);
      const summary = toolResults.map((tr, i) => ({ idx: i, ...tr })).slice(0, 5);
      const augmented = [
        ...conversation,
        { role: 'assistant', content: normalizeContent(aiResponse.content) || 'Called tools.' },
        { role: 'system', content: `Tool execution results (JSON): ${JSON.stringify(summary)}` },
        { role: 'system', content: 'Using the tool results above, provide a concise final answer for the user. Do not emit tool_calls; respond as plain text.' },
      ];

      try {
        const llm = getLlmConfig();
        const followUp = await axios.post(llm.url, { model: llm.model, messages: augmented, max_tokens: 800, temperature: 0.5 }, { headers: llm.headers });
        const maybe = followUp.data.choices?.[0]?.message?.content;
        const maybeStr = normalizeContent(maybe);
        const maybeJson = extractJsonObject(maybeStr);
        const stillLooksLikeToolCall = /tool_calls/i.test(maybeStr) || (maybeJson && Array.isArray(maybeJson.tool_calls));
        if (maybeStr && !stillLooksLikeToolCall) { finalContent = maybeStr; }
      } catch (e) { /* keep summary */ }

      const fcStr = normalizeContent(finalContent);
      const parsedMaybe = extractJsonObject(fcStr);
      const looksLikeToolCall = /tool_calls/i.test(fcStr) || (parsedMaybe && Array.isArray(parsedMaybe.tool_calls));
      if (!fcStr || looksLikeToolCall) { finalContent = summarizeToolResults(toolResults, toolCalls); }
    }

    return res.json({ response: finalContent, toolCalls, toolResults, agent: { usedTools: toolResults.length > 0, final: true } });
  } catch (error) {
    const status = error.response?.status;
    const data = error.response?.data;
    return res.status(status || 500).json({ error: 'Failed to process chat request', details: error.message, router_status: status || null, router_error: data || null });
  }
};
