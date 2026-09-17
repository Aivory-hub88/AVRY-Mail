"use client";
import { useEffect, useRef } from "react";

const API = process.env.NEXT_PUBLIC_MAIL_API || "http://localhost:8095";

function storedToken(): string | null {
  if (typeof window === "undefined") return null;
  return localStorage.getItem("aivory_mail_token") || sessionStorage.getItem("aivory_mail_token");
}

function wsBase(): string {
  if (API.startsWith("https://")) return API.replace(/^https:\/\//, "wss://");
  if (API.startsWith("http://")) return API.replace(/^http:\/\//, "ws://");
  return API;
}

/**
 * Live inbox updates over the backend's /v1/realtime/ws socket (JWT via
 * ?token= — browsers can't send Authorization on upgrade). On a new_message
 * event for our mailbox the inbox list refreshes by itself; no manual
 * reload. Reconnects with backoff, pings to keep idle sockets alive.
 */
export function useRealtimeInbox(opts: {
  mailboxId: string;
  enabled: boolean;
  onNewMessage: (msg: any) => void;
}) {
  const { mailboxId, enabled, onNewMessage } = opts;
  const cbRef = useRef(onNewMessage);
  cbRef.current = onNewMessage;

  useEffect(() => {
    if (!enabled || !mailboxId) return;
    const token = storedToken();
    if (!token) return;
    let ws: WebSocket | null = null;
    let closed = false;
    let backoff = 1000;
    let pingIv: ReturnType<typeof setInterval> | null = null;

    const connect = () => {
      if (closed) return;
      try {
        ws = new WebSocket(
          `${wsBase()}/v1/realtime/ws?mailbox_id=${encodeURIComponent(mailboxId)}&token=${encodeURIComponent(token)}`
        );
      } catch {
        schedule();
        return;
      }
      ws.onopen = () => {
        backoff = 1000;
        if (pingIv) clearInterval(pingIv);
        pingIv = setInterval(() => {
          try { ws?.send(JSON.stringify({ ping: Date.now() })); } catch {}
        }, 25000);
      };
      ws.onmessage = (ev) => {
        try {
          const data = JSON.parse(ev.data);
          if (data?.type === "new_message" && data?.message) {
            cbRef.current(data.message);
          }
        } catch {}
      };
      ws.onclose = () => {
        if (pingIv) { clearInterval(pingIv); pingIv = null; }
        schedule();
      };
      ws.onerror = () => {
        try { ws?.close(); } catch {}
      };
    };
    const schedule = () => {
      if (closed) return;
      const wait = Math.min(backoff, 30000);
      backoff *= 2;
      setTimeout(connect, wait);
    };
    connect();
    return () => {
      closed = true;
      if (pingIv) clearInterval(pingIv);
      try { ws?.close(); } catch {}
    };
  }, [enabled, mailboxId]);
}

export default useRealtimeInbox;
