const axios = require('axios');

const MCP_SERVER_URL = process.env.MCP_SERVER_URL || 'https://evm-mcp.onrender.com';

module.exports = async function handler(req, res) {
  if (req.method !== 'GET') {
    res.setHeader('Allow', 'GET');
    return res.status(405).json({ error: 'Method Not Allowed' });
  }
  try {
    const payload = { jsonrpc: '2.0', id: Date.now(), method: 'tools/list', params: {} };
    const response = await axios.post(`${MCP_SERVER_URL}/api/rpc`, payload, { headers: { 'Content-Type': 'application/json' } });
    return res.json({ status: 'ok', tools: response.data.result || [] });
  } catch (error) {
    return res.status(502).json({ status: 'error', message: 'Failed to list tools', details: error.message });
  }
};
