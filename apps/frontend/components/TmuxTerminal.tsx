'use client';

import { useEffect, useRef, useState } from 'react';
import type { Terminal } from '@xterm/xterm';
import type { FitAddon } from '@xterm/addon-fit';
import type { WebLinksAddon } from '@xterm/addon-web-links';

interface TmuxTerminalProps {
  taskId: string | null;
  agentId: string | null;
  connected: boolean;
  onConnect: () => void;
  disabled?: boolean;
}

export default function TmuxTerminal({
  taskId,
  agentId,
  connected,
  onConnect,
  disabled = false,
}: TmuxTerminalProps) {
  const terminalRef = useRef<HTMLDivElement>(null);
  const terminal = useRef<Terminal | null>(null);
  const fitAddon = useRef<FitAddon | null>(null);
  const websocket = useRef<WebSocket | null>(null);
  const [connectionStatus, setConnectionStatus] = useState<'disconnected' | 'connecting' | 'connected'>('disconnected');

  // Initialize terminal
  useEffect(() => {
    if (!terminalRef.current || terminal.current) return;

    let isMounted = true;

    const initTerminal = async () => {
      try {
        // Dynamically import xterm modules
        const [
          { Terminal },
          { FitAddon },
          { WebLinksAddon }
        ] = await Promise.all([
          import('@xterm/xterm'),
          import('@xterm/addon-fit'),
          import('@xterm/addon-web-links')
        ]);

        if (!isMounted || !terminalRef.current) return;

        // Create terminal instance
        const term = new Terminal({
          cursorBlink: true,
          fontFamily: '"Monaco", "Menlo", "Ubuntu Mono", monospace',
          fontSize: 14,
          theme: {
            background: '#1a1a1a',
            foreground: '#ffffff',
            cursor: '#ffffff',
            selectionBackground: '#ffffff40',
          },
        });

        // Add addons
        const fit = new FitAddon();
        const webLinks = new WebLinksAddon();
        term.loadAddon(fit);
        term.loadAddon(webLinks);

        // Open terminal
        term.open(terminalRef.current);
        fit.fit();

        // Store references
        terminal.current = term;
        fitAddon.current = fit;
      } catch (error) {
        console.error('Failed to initialize terminal:', error);
      }
    };

    initTerminal();

    // Handle window resize
    const handleResize = () => {
      if (fitAddon.current) {
        fitAddon.current.fit();
      }
    };
    window.addEventListener('resize', handleResize);

    return () => {
      isMounted = false;
      window.removeEventListener('resize', handleResize);
      if (terminal.current) {
        terminal.current.dispose();
        terminal.current = null;
      }
    };
  }, []);

  // Handle connection
  useEffect(() => {
    if (!connected || !taskId || !agentId || !terminal.current) {
      return;
    }

    setConnectionStatus('connecting');

    // Create WebSocket connection
    const wsUrl = `ws://localhost:3000/ws/tasks/${taskId}/agents/${agentId}/attach`;
    const ws = new WebSocket(wsUrl);
    websocket.current = ws;

    ws.onopen = () => {
      setConnectionStatus('connected');
      terminal.current?.clear();
      terminal.current?.write('\r\n\x1b[32m✓ Connected to agent session\x1b[0m\r\n\r\n');
    };

    ws.onmessage = (event) => {
      if (terminal.current) {
        const data = event.data;
        if (data.startsWith('output:')) {
          // Handle tmux output from backend
          const output = data.slice(7); // Remove 'output:' prefix
          terminal.current.clear();
          terminal.current.write(output);
        } else if (data.startsWith('Error:')) {
          // Handle error messages
          terminal.current.write(`\r\n\x1b[31m${data}\x1b[0m\r\n`);
        } else {
          // Handle other messages (like initial connection message)
          terminal.current.write(`\r\n\x1b[33m${data}\x1b[0m\r\n`);
        }
      }
    };

    ws.onclose = () => {
      setConnectionStatus('disconnected');
      terminal.current?.write('\r\n\x1b[31m✗ Connection closed\x1b[0m\r\n');
    };

    ws.onerror = (error) => {
      setConnectionStatus('disconnected');
      console.error('WebSocket error:', error);
      terminal.current?.write('\r\n\x1b[31m✗ Connection error\x1b[0m\r\n');
    };

    // Handle terminal input
    const handleData = (data: string) => {
      if (ws.readyState === WebSocket.OPEN) {
        // Handle special key combinations
        if (data === '\r') {
          // Enter key
          ws.send('enter');
        } else if (data === '\u0003') {
          // Ctrl+C
          ws.send('input:\u0003');
        } else if (data === '\u0004') {
          // Ctrl+D
          ws.send('input:\u0004');
        } else if (data === '\u001b[A' || data === '\u001b[B' || data === '\u001b[C' || data === '\u001b[D') {
          // Arrow keys
          ws.send(`input:${data}`);
        } else if (data.length === 1 && data.charCodeAt(0) < 32) {
          // Other control characters
          ws.send(`input:${data}`);
        } else {
          // Regular text input
          ws.send(`input:${data}`);
        }
      }
    };

    terminal.current.onData(handleData);

    return () => {
      if (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING) {
        ws.close();
      }
      websocket.current = null;
    };
  }, [connected, taskId, agentId]);

  // Fit terminal when panel size changes
  useEffect(() => {
    if (fitAddon.current) {
      const timer = setTimeout(() => {
        fitAddon.current?.fit();
      }, 100);
      return () => clearTimeout(timer);
    }
  }, []);

  const getStatusColor = () => {
    switch (connectionStatus) {
      case 'connected':
        return 'text-green-500';
      case 'connecting':
        return 'text-yellow-500';
      default:
        return 'text-gray-500';
    }
  };

  const getStatusText = () => {
    switch (connectionStatus) {
      case 'connected':
        return 'Connected';
      case 'connecting':
        return 'Connecting...';
      default:
        return 'Disconnected';
    }
  };

  return (
    <div className="h-full flex flex-col bg-gray-900">
      {/* Terminal header */}
      <div className="flex-none px-4 py-2 bg-gray-800 border-b border-gray-700">
        <div className="flex items-center justify-between">
          <div className="flex items-center space-x-3">
            <h3 className="text-sm font-medium text-white">Terminal</h3>
            {taskId && agentId && (
              <span className="text-xs text-gray-400">
                Task: {taskId.slice(0, 8)}... | Agent: {agentId.slice(0, 8)}...
              </span>
            )}
          </div>
          <div className="flex items-center space-x-3">
            <div className="flex items-center space-x-2">
              <div className={`w-2 h-2 rounded-full ${
                connectionStatus === 'connected' ? 'bg-green-500' : 
                connectionStatus === 'connecting' ? 'bg-yellow-500' : 'bg-gray-500'
              }`} />
              <span className={`text-xs ${getStatusColor()}`}>
                {getStatusText()}
              </span>
            </div>
            {!connected && !disabled && (
              <button
                onClick={onConnect}
                className="px-3 py-1 text-xs bg-blue-600 text-white rounded hover:bg-blue-700 transition-colors"
              >
                Connect
              </button>
            )}
          </div>
        </div>
      </div>

      {/* Terminal content */}
      <div className="flex-1 relative">
        {disabled ? (
          <div className="absolute inset-0 flex items-center justify-center bg-gray-900">
            <div className="text-center text-gray-400">
              <div className="mb-4">
                <svg className="mx-auto h-12 w-12" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z" />
                </svg>
              </div>
              <p className="text-lg">No agent selected</p>
              <p className="text-sm mt-1">Select an agent from the task tree to connect</p>
            </div>
          </div>
        ) : (
          <div
            ref={terminalRef}
            className="h-full w-full"
            style={{ backgroundColor: '#1a1a1a' }}
          />
        )}
      </div>
    </div>
  );
}
