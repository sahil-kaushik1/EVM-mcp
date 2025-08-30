const axios = require('axios');

const MCP_SERVER_URL = process.env.MCP_SERVER_URL || 'http://localhost:8080';

module.exports = async function handler(req, res) {
  if (req.method !== 'GET') {
    res.setHeader('Allow', 'GET');
    return res.status(405).json({ error: 'Method Not Allowed' });
  }
  try {
    const payload = {
      jsonrpc: '2.0',
      id: Date.now(),
      method: 'get_balance',
      params: { chain_id: '1', address: '0x0000000000000000000000000000000000000000' },
    };
    const response = await axios.post(`${MCP_SERVER_URL}/api/rpc`, payload, { headers: { 'Content-Type': 'application/json' } });
    return res.json({ status: 'ok', result: response.data.result || null });
  } catch (error) {
    return res.status(502).json({ status: 'error', message: 'Failed to reach MCP', details: error.message });
  }
};
