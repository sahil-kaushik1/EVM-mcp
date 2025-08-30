import React, { useState, useRef, useEffect } from 'react';
import './App.css';
import FluidCursor from './components/FluidCursor';

function App() {
  const [messages, setMessages] = useState([]);
  const [input, setInput] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const chatRef = useRef(null);
  const messagesEndRef = useRef(null);
  const maxInputHeight = 140; // should match CSS .composer-input max-height

  // Auto-scroll to bottom when new messages are added (natural order)
  const scrollToBottom = () => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth', block: 'end' });
  };

  useEffect(() => {
    scrollToBottom();
  }, [messages]);

  // Auto-resize textarea to its content up to max height
  const autoResize = () => {
    const el = chatRef.current;
    if (!el) return;
    el.style.height = 'auto';
    const next = Math.min(el.scrollHeight, maxInputHeight);
    el.style.height = `${next}px`;
  };

  useEffect(() => {
    autoResize();
  }, [input]);

  // GET-only test for MCP health (no private key)
  const testMCPHealth = async () => {
    try {
      // Use proxy GET that performs internal POST to MCP
      const res = await fetch('http://localhost:3001/api/mcp/health', { method: 'GET' });
      if (!res.ok) throw new Error('Proxy /api/mcp/health not OK');
      const data = await res.json();
      setMessages(prev => [
        ...prev,
        { role: 'assistant', content: `MCP health (via proxy GET): ${JSON.stringify(data)}` }
      ]);
    } catch (e) {
      try {
        // Fallback to frontend server health
        const res2 = await fetch('http://localhost:3001/api/health', { method: 'GET' });
        const data2 = await res2.json();
        setMessages(prev => [
          ...prev,
          { role: 'assistant', content: `Frontend health (GET): ${JSON.stringify(data2)} | MCP proxy failed` }
        ]);
      } catch (e2) {
        setMessages(prev => [
          ...prev,
          { role: 'assistant', content: `GET test failed: ${e.message}` }
        ]);
      }
    }
  };

  // Handle sending message to AI
  const handleSendMessage = async () => {
    if (!input.trim()) return;

    const userMessage = { role: 'user', content: input };
    setMessages(prev => [...prev, userMessage]);
    setInput('');
    setIsLoading(true);
    // reset composer height immediately after submit
    requestAnimationFrame(() => autoResize());

    try {
      // Call backend API that integrates with Together AI (proxy)
      const response = await fetch('http://localhost:3001/api/chat', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ message: input, messages: [...messages, userMessage] })
      });

      const data = await response.json();
      const responseText =
        typeof data.response === 'string' ? data.response : JSON.stringify(data.response);
      const aiMessage = { role: 'assistant', content: responseText, agent: !!data.agent?.usedTools };

      // Render assistant reply
      let newMsgs = [...messages, userMessage, aiMessage];

      // Render tool results if proxy executed tools
      if (Array.isArray(data.toolResults) && data.toolResults.length > 0) {
        data.toolResults.forEach((tr, idx) => {
          const summary = JSON.stringify(tr.result ?? tr.error ?? {}, null, 2);
          newMsgs.push({ role: 'tool', content: summary, pre: true });
        });
      }

      setMessages(newMsgs);
    } catch (error) {
      console.error('Error sending message:', error);
      setMessages(prev => [...prev, { role: 'assistant', content: 'Sorry, there was an error processing your message.' }]);
    } finally {
      setIsLoading(false);
    }
  };

  // Note: Tool execution is handled on the proxy server now. We only render `toolResults`.

  return (
    <div className="app" style={{ backgroundColor: '#fff', color: '#000' }}>
      <FluidCursor />
      {/* Utility row with GET-only test */}
      <div className="utility-row">
        <div className="conversation-title">
          <h1 style={{ color: '#000' }}>EVM Sorcerer</h1>
        </div>
        <div className="utility-actions">
          <button className="action-btn" title="Test MCP GET health" onClick={testMCPHealth}>
            <svg width="18" height="18" viewBox="0 0 24 24" fill="#111" aria-hidden>
              <path d="M19 3H5c-1.1 0-2 .9-2 2v14a2 2 0 0 0 2 2h14c1.1 0 2-.9 2-2V5a2 2 0 0 0-2-2zm-2.41 6.41-4.59 4.58-2.59-2.58L8 13l4 4 6-6-1.41-1.59z"/>
            </svg>
          </button>
        </div>
      </div>
      {/* Main Content Area */}
      <div className="main-content">
        {/* Conversation Column */}
        <div className="conversation-column">
          {/* Messages Display or Welcome Screen */}
          {messages.length > 0 ? (
            <div className="messages-container">
              <div className="messages-stack">
                {messages.map((msg, index) => (
                  <div
                    key={index}
                    className={`message-group ${msg.role}`}
                  >
                    <div
                      className={`message ${msg.role}`}
                      style={{
                        animationDelay: `${index * 0.05}s`,
                        backgroundColor: msg.role === 'assistant' ? '#f7f7f9' : (msg.role === 'tool' ? '#f3f8ff' : '#f5f5f5'),
                        color: '#111',
                        border: '1px solid #e5e5e5'
                      }}
                    >
                      <div className="message-content">
                        {msg.agent && msg.role === 'assistant' ? (
                          <div style={{ fontSize: 12, fontWeight: 600, marginBottom: 6, color: '#2563eb' }}>Agent</div>
                        ) : null}
                        {msg.pre ? (
                          <details className="tool-details">
                            <summary>Details</summary>
                            <pre style={{ margin: 0, whiteSpace: 'pre-wrap' }}>{msg.content}</pre>
                          </details>
                        ) : (
                          <>{msg.content}</>
                        )}
                      </div>
                    </div>
                    {msg.role === 'assistant' && (
                      <div className="message-controls">
                        <button className="control-btn">
                          <svg width="14" height="14" fill="#fff" viewBox="0 0 24 24">
                            <path d="M16 1H4c-1.1 0-2 .9-2 2v14h2V3h12V1zm3 4H8c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h11c1.1 0 2-.9 2-2V7c0-1.1-.9-2-2-2zm0 16H8V7h11v14z"/>
                          </svg>
                        </button>
                        <button className="control-btn">
                          <svg width="14" height="14" fill="#fff" viewBox="0 0 24 24">
                            <path d="M1 21h4V9H1v12zm22-11c0-1.1-.9-2-2-2h-6.31l.95-4.57.03-.32c0-.41-.17-.79-.44-1.06L14.17 1 7.59 7.59C7.22 7.95 7 8.45 7 9v10c0 1.1.9 2 2 2h9c.83 0 1.54-.5 1.84-1.22l3.02-7.05c.09-.23.14-.47.14-.73v-1.91l-.01-.01L23 10z"/>
                          </svg>
                        </button>
                        <button className="control-btn">
                          <svg width="14" height="14" fill="#fff" viewBox="0 0 24 24">
                            <path d="M15 3H6c-.83 0-1.54.5-1.84 1.22l-3.02 7.05c-.09.23-.14.47-.14.73v1.91l.01.01L1 14c0 1.1.9 2 2 2h6.31l-.95 4.57.03.32c0 .41.17.79.44 1.06L9.83 23l6.59-6.59c.36-.36.58-.86.58-1.41V5c0-1.1-.9-2-2-2zm4 0v14h4V3h-4z"/>
                          </svg>
                        </button>
                      </div>
                    )}
                  </div>
                ))}
                  <div ref={messagesEndRef} />
                {isLoading && (
                  <div className="message-group assistant">
                    <div className="message assistant loading">
                      <div className="loading-dots">
                        <div className="dot"></div>
                        <div className="dot"></div>
                        <div className="dot"></div>
                      </div>
                    </div>
                  </div>
                )}
              </div>
            </div>
          ) : (
            <div className="welcome-screen">
              <div className="welcome-content">
                <h1 style={{ color: '#000' }}>Welcome to EVM Sorcerer</h1>
                <p style={{ color: '#000' }}>Your intelligent assistant for blockchain queries and operations.</p>
                <div className="welcome-examples">
                  <p style={{ color: '#000' }}>Try asking:</p>
                  <ul>
                    <li style={{ color: '#000' }}>
                      "What is the balance of 0x742d...f44e on Ethereum mainnet?"
                    </li>
                    <li style={{ color: '#000' }}>
                      "Create a new EVM wallet for me"
                    </li>
                    <li style={{ color: '#000' }}>
                      "Show recent transactions for 0x742d...f44e on Sepolia"
                    </li>
                    <li style={{ color: '#000' }}>
                      "Get contract info for 0xdAC17F958D2ee523a2206206994597C13D831ec7"
                    </li>
                    <li style={{ color: '#000' }}>
                      "Check if 0x123...abc is a smart contract"
                    </li>
                    <li style={{ color: '#000' }}>
                      "Get USDC token balance for 0x742d...f44e"
                    </li>
                    <li style={{ color: '#000' }}>
                      "Search for Transfer events on contract 0xabc...123"
                    </li>
                  </ul>
                </div>
                <p className="welcome-tip">Start by typing your query below...</p>
              </div>
            </div>
          )}

          {/* Composer */}
          <div className="composer">
            <form
              onSubmit={(e) => {
                e.preventDefault();
                if (input.trim()) handleSendMessage();
              }}
              className="composer-form"
            >
              <div className="composer-container">
                <button type="button" className="composer-action">
                  <svg width="20" height="20" fill="currentColor" viewBox="0 0 24 24">
                    <path d="M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z"/>
                  </svg>
                </button>
                <div className="input-wrapper">
                  <textarea
                    ref={chatRef}
                    value={input}
                    onChange={(e) => setInput(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter' && !e.shiftKey) {
                        e.preventDefault();
                        if (input.trim()) {
                          handleSendMessage();
                        }
                      }
                    }}
                    placeholder="Cast your blockchain query..."
                    className="composer-input"
                    disabled={isLoading}
                    rows="1"
                  />
                </div>
                <div className="composer-actions">
                  <button type="button" className="composer-action">
                    <svg width="20" height="20" fill="currentColor" viewBox="0 0 24 24">
                      <path d="M12 14c1.66 0 2.99-1.34 2.99-3L15 5c0-1.66-1.34-3-3-3S9 3.34 9 5v6c0 1.66 1.34 2.99 3 2.99zm5.3-3c0 3-2.54 5.1-5.3 5.1S6.7 14 6.7 11H5c0 3.41 2.72 6.23 6 6.72V21h2v-3.28c3.28-.48 6-3.3 6-6.72h-1.7z"/>
                    </svg>
                  </button>
                  <button
                    type="submit"
                    disabled={isLoading || !input.trim()}
                    className="composer-submit"
                  >
                    <svg width="20" height="20" fill="currentColor" viewBox="0 0 24 24">
                      <path d="M2.01 21L23 12 2.01 3 2 10l15 2-15 2z"/>
                    </svg>
                  </button>
                </div>
              </div>
            </form>
          </div>
        </div>
      </div>
    </div>
  );
}

export default App;
