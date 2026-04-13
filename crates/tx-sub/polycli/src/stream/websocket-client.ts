import WebSocket from "ws";
import type { MarketMessage, MarketSubscriptionMessage } from "./types";

export interface WebSocketClientOptions {
  url: string;
  assetIds: string[];
  onMessage: (message: MarketMessage) => void;
  onError: (error: Error) => void;
  onConnect?: () => void;
  onDisconnect?: () => void;
  enablePing?: boolean;
  pingIntervalMs?: number;
}

/**
 * WebSocket client for Polymarket market channel
 * Handles connection, subscription, ping/pong, and auto-reconnection
 */
export class PolymarketWebSocketClient {
  private ws: WebSocket | null = null;
  private pingInterval?: NodeJS.Timeout;
  private reconnectAttempts = 0;
  private readonly maxReconnectAttempts = 5;
  private isClosing = false;
  private readonly options: WebSocketClientOptions;

  constructor(options: WebSocketClientOptions) {
    this.options = {
      enablePing: true,
      pingIntervalMs: 10000,
      ...options,
    };
  }

  /**
   * Connect to WebSocket and send subscription message
   */
  connect(): void {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      return;
    }

    this.ws = new WebSocket(this.options.url);

    this.ws.on("open", () => {
      this.reconnectAttempts = 0;
      this.subscribe();

      if (this.options.enablePing) {
        this.startPingLoop();
      }

      if (this.options.onConnect) {
        this.options.onConnect();
      }
    });

    this.ws.on("message", (data: WebSocket.Data) => {
      this.handleMessage(data.toString());
    });

    this.ws.on("error", (error) => {
      this.options.onError(error);
    });

    this.ws.on("close", () => {
      this.stopPingLoop();

      if (this.options.onDisconnect) {
        this.options.onDisconnect();
      }

      if (!this.isClosing) {
        this.reconnect();
      }
    });
  }

  /**
   * Send subscription message for market channel
   * Format: {"assets_ids": ["..."], "type": "market"}
   */
  private subscribe(): void {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      return;
    }

    const subscriptionMessage: MarketSubscriptionMessage = {
      assets_ids: this.options.assetIds,
      type: "market",
    };

    this.ws.send(JSON.stringify(subscriptionMessage));
  }

  /**
   * Start ping loop (send "PING" every interval)
   */
  private startPingLoop(): void {
    this.stopPingLoop();

    this.pingInterval = setInterval(() => {
      if (this.ws && this.ws.readyState === WebSocket.OPEN) {
        this.ws.send("PING");
      }
    }, this.options.pingIntervalMs);
  }

  /**
   * Stop ping loop
   */
  private stopPingLoop(): void {
    if (this.pingInterval) {
      clearInterval(this.pingInterval);
      this.pingInterval = undefined;
    }
  }

  /**
   * Handle incoming messages
   */
  private handleMessage(data: string): void {
    // Ignore PONG responses
    if (data === "PONG") {
      return;
    }

    try {
      const message = JSON.parse(data) as MarketMessage;
      this.options.onMessage(message);
    } catch (error) {
      const parseError =
        error instanceof Error ? error : new Error(String(error));
      this.options.onError(
        new Error(`Failed to parse message: ${parseError.message}`)
      );
    }
  }

  /**
   * Reconnect with exponential backoff
   */
  private reconnect(): void {
    if (
      this.isClosing ||
      this.reconnectAttempts >= this.maxReconnectAttempts
    ) {
      if (this.reconnectAttempts >= this.maxReconnectAttempts) {
        this.options.onError(
          new Error(
            `Max reconnection attempts (${this.maxReconnectAttempts}) exceeded`
          )
        );
      }
      return;
    }

    this.reconnectAttempts++;
    const backoffMs = Math.pow(2, this.reconnectAttempts - 1) * 1000;

    setTimeout(() => {
      if (!this.isClosing) {
        this.connect();
      }
    }, backoffMs);
  }

  /**
   * Gracefully disconnect
   */
  disconnect(): void {
    this.isClosing = true;
    this.stopPingLoop();

    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
  }
}
