const axios = require('axios');

const MCP_SERVER_URL = process.env.MCP_SERVER_URL || 'https://evm-mcp.onrender.com';

module.exports = async function handler(req, res) {
  if (req.method !== 'POST') {
    res.setHeader('Allow', 'POST');
    return res.status(405).json({ error: 'Method Not Allowed' });
  }
  try {
    const { method, params } = req.body || {};
    if (!method || typeof method !== 'string') {
      return res.status(400).json({ error: 'method is required' });
    }
    const payload = { jsonrpc: '2.0', id: Date.now(), method, params: params || {} };
    const response = await axios.post(`${MCP_SERVER_URL}/api/rpc`, payload, { headers: { 'Content-Type': 'application/json' } });
    return res.json({ status: 'ok', result: response.data.result ?? null, raw: response.data });
  } catch (error) {
    const status = error.response?.status || 502;
    return res.status(status).json({ status: 'error', message: 'MCP call failed', details: error.message, data: error.response?.data });
  }
};
