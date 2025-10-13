'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import { WebSocketMessage } from '@/types';

interface UseAgentDevSocketOptions {
  taskId?: string;
  agentId?: string;
  autoConnect?: boolean;
  reconnectInterval?: number;
  maxReconnectAttempts?: number;
}

export function useAgentDevSocket(options: UseAgentDevSocketOptions = {}) {
  const {
    taskId,
    agentId,
    autoConnect = false,
    reconnectInterval = 3000,
    maxReconnectAttempts = 5,
  } = options;

  const [isConnected, setIsConnected] = useState(false);
  const [connectionState, setConnectionState] = useState<'disconnected' | 'connecting' | 'connected' | 'error'>('disconnected');
  const [lastMessage, setLastMessage] = useState<WebSocketMessage | null>(null);
  const [error, setError] = useState<string | null>(null);

  const websocket = useRef<WebSocket | null>(null);
  const reconnectAttempts = useRef(0);
  const reconnectTimeout = useRef<NodeJS.Timeout | null>(null);

  // Connect to WebSocket
  const connect = useCallback(() => {
    if (!taskId || !agentId) {
      setError('Task ID and Agent ID are required');
      return;
    }

    if (websocket.current?.readyState === WebSocket.OPEN) {
      return; // Already connected
    }

    setConnectionState('connecting');
    setError(null);

    const wsUrl = `ws://localhost:3000/ws/tasks/${taskId}/agents/${agentId}/attach`;
    const ws = new WebSocket(wsUrl);

    ws.onopen = () => {
      console.log('WebSocket connected');
      setIsConnected(true);
      setConnectionState('connected');
      reconnectAttempts.current = 0; // Reset reconnect attempts
      
      // Send connection message
      const message: WebSocketMessage = {
        type: 'connected',
        data: 'Connected to agent session',
        timestamp: Date.now(),
      };
      setLastMessage(message);
    };

    ws.onmessage = (event) => {
      try {
        // Try to parse as JSON, otherwise treat as plain text
        let messageData;
        try {
          messageData = JSON.parse(event.data);
        } catch {
          messageData = event.data;
        }

        const message: WebSocketMessage = {
          type: 'output',
          data: typeof messageData === 'string' ? messageData : JSON.stringify(messageData),
          timestamp: Date.now(),
        };
        
        setLastMessage(message);
      } catch (err) {
        console.error('Error processing WebSocket message:', err);
      }
    };

    ws.onclose = (event) => {
      console.log('WebSocket disconnected:', event.code, event.reason);
      setIsConnected(false);
      setConnectionState('disconnected');
      
      const message: WebSocketMessage = {
        type: 'disconnected',
        data: `Connection closed (${event.code})`,
        timestamp: Date.now(),
      };
      setLastMessage(message);

      // Attempt to reconnect if it wasn't a manual close
      if (event.code !== 1000 && reconnectAttempts.current < maxReconnectAttempts) {
        reconnectAttempts.current++;
        console.log(`Attempting to reconnect (${reconnectAttempts.current}/${maxReconnectAttempts})...`);
        
        reconnectTimeout.current = setTimeout(() => {
          connect();
        }, reconnectInterval);
      }
    };

    ws.onerror = (event) => {
      console.error('WebSocket error:', event);
      setConnectionState('error');
      setError('WebSocket connection error');
      
      const message: WebSocketMessage = {
        type: 'error',
        data: 'Connection error occurred',
        timestamp: Date.now(),
      };
      setLastMessage(message);
    };

    websocket.current = ws;
  }, [taskId, agentId, reconnectInterval, maxReconnectAttempts]);

  // Disconnect from WebSocket
  const disconnect = useCallback(() => {
    if (reconnectTimeout.current) {
      clearTimeout(reconnectTimeout.current);
      reconnectTimeout.current = null;
    }

    if (websocket.current) {
      websocket.current.close(1000, 'Manual disconnect');
      websocket.current = null;
    }

    setIsConnected(false);
    setConnectionState('disconnected');
    reconnectAttempts.current = 0;
  }, []);

  // Send message to WebSocket
  const sendMessage = useCallback((data: string) => {
    if (websocket.current?.readyState === WebSocket.OPEN) {
      websocket.current.send(data);
      
      // Create input message for local echo
      const message: WebSocketMessage = {
        type: 'input',
        data,
        timestamp: Date.now(),
      };
      setLastMessage(message);
      
      return true;
    } else {
      console.warn('WebSocket is not connected');
      return false;
    }
  }, []);

  // Auto-connect effect
  useEffect(() => {
    if (autoConnect && taskId && agentId) {
      connect();
    }

    return () => {
      disconnect();
    };
  }, [autoConnect, taskId, agentId, connect, disconnect]);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      disconnect();
    };
  }, [disconnect]);

  return {
    isConnected,
    connectionState,
    lastMessage,
    error,
    connect,
    disconnect,
    sendMessage,
  };
}
