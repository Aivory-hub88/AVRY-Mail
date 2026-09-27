"use client";
/**
 * Web Push subscription for the signed-in mailbox: registers /sw.js and
 * hands the browser's push subscription to the API, so new-mail
 * notifications arrive even when no Aivory Mail tab is open.
 *
 * Needs notification permission first (asked from a click, see
 * useDesktopNotificationPermission). Everything here is best-effort: on an
 * unsupported browser or a server without VAPID keys it quietly reports
 * "unavailable" and the in-tab pop-ups keep working as before.
 */

type AuthFetch = (path: string, init?: RequestInit) => Promise<Response>;

export type PushState = "active" | "unavailable" | "needs-permission";

export function pushSupported(): boolean {
  return (
    typeof window !== "undefined" &&
    "serviceWorker" in navigator &&
    "PushManager" in window &&
    "Notification" in window
  );
}

/** base64url → bytes, for PushManager's applicationServerKey. */
export function urlBase64ToUint8Array(base64: string): Uint8Array {
  const padding = "=".repeat((4 - (base64.length % 4)) % 4);
  const b64 = (base64 + padding).replace(/-/g, "+").replace(/_/g, "/");
  const raw = atob(b64);
  const out = new Uint8Array(raw.length);
  for (let i = 0; i < raw.length; i++) out[i] = raw.charCodeAt(i);
  return out;
}

/**
 * Make sure this browser is subscribed and the API knows it. Safe to call
 * on every load: the API upserts by endpoint, which also re-binds the
 * browser to whichever mailbox is signed in now.
 */
export async function ensurePushSubscription(authFetch: AuthFetch): Promise<PushState> {
  if (!pushSupported()) return "unavailable";
  if (Notification.permission !== "granted") return "needs-permission";
  try {
    const cfg = await authFetch("/v1/push/config").then((r) => r.json());
    const publicKey: string | null = cfg?.data?.enabled ? cfg.data.public_key : null;
    if (!publicKey) return "unavailable";

    const reg = await navigator.serviceWorker.register("/sw.js", { scope: "/" });
    await navigator.serviceWorker.ready;
    let sub = await reg.pushManager.getSubscription();
    // A subscription made with a different server key can't receive our
    // pushes (the key was rotated): replace it.
    const current = sub?.options?.applicationServerKey;
    if (sub && current) {
      const want = urlBase64ToUint8Array(publicKey);
      const have = new Uint8Array(current as ArrayBuffer);
      if (have.length !== want.length || have.some((b, i) => b !== want[i])) {
        await sub.unsubscribe();
        sub = null;
      }
    }
    if (!sub) {
      sub = await reg.pushManager.subscribe({
        userVisibleOnly: true,
        applicationServerKey: urlBase64ToUint8Array(publicKey) as BufferSource,
      });
    }
    const json = sub.toJSON();
    const r = await authFetch("/v1/push/subscriptions", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ endpoint: json.endpoint, keys: json.keys }),
    });
    return r.ok ? "active" : "unavailable";
  } catch {
    return "unavailable";
  }
}

/** Forget this browser (sign-out): the API stops pushing to it. */
export async function removePushSubscription(authFetch: AuthFetch): Promise<void> {
  if (!pushSupported()) return;
  try {
    const reg = await navigator.serviceWorker.getRegistration("/");
    const sub = await reg?.pushManager.getSubscription();
    if (!sub) return;
    await authFetch("/v1/push/subscriptions", {
      method: "DELETE",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ endpoint: sub.endpoint }),
    }).catch(() => {});
    await sub.unsubscribe();
  } catch {
    // Nothing to clean up, or the browser already dropped it.
  }
}
