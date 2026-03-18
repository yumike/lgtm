import type { WsMessage } from './types';

type MessageHandler = (msg: WsMessage) => void;
type ResyncHandler = () => Promise<void>;

export function createWsClient(onMessage: MessageHandler, onResync: ResyncHandler) {
  let ws: WebSocket | null = null;
  let backoff = 1000;
  const maxBackoff = 30000;
  let stopped = false;

  function connect() {
    if (stopped) return;

    const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
    ws = new WebSocket(`${protocol}//${location.host}/ws`);

    ws.onopen = () => {
      backoff = 1000;
      onResync();
    };

    ws.onmessage = (event) => {
      try {
        const msg: WsMessage = JSON.parse(event.data);
        onMessage(msg);
      } catch {
        // Ignore malformed messages
      }
    };

    ws.onclose = () => {
      if (stopped) return;
      setTimeout(connect, backoff);
      backoff = Math.min(backoff * 2, maxBackoff);
    };

    ws.onerror = () => {
      ws?.close();
    };
  }

  connect();

  return {
    stop() {
      stopped = true;
      ws?.close();
    },
  };
}
