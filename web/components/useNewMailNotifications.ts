"use client";
import { useCallback, useEffect, useRef, useState } from "react";
import { buildPopUps, notificationPermission, pickNewMessages, type PopUp } from "./newMailNotify";

type NotifSettings = { desktop_sound: boolean; new_mail_banner: boolean };
const DEFAULT_SETTINGS: NotifSettings = { desktop_sound: true, new_mail_banner: true };
const POLL_MS = 20000;

// Shared across hook instances so the gesture unlock applies everywhere.
let sharedCtx: AudioContext | null = null;

/** Event the inbox page listens for to open a message (pop-up click). */
export const OPEN_MESSAGE_EVENT = "aivory:open-message";
const ICON = "/notification-icon.png"; // PNG: Chrome doesn't render SVG notification icons

function showPopUp(p: PopUp) {
  try {
    const n = new Notification(p.title, { body: p.body, icon: ICON, tag: p.tag });
    n.onclick = () => {
      window.focus();
      if (p.messageId) {
        window.dispatchEvent(new CustomEvent(OPEN_MESSAGE_EVENT, { detail: { id: p.messageId } }));
      }
      n.close();
    };
  } catch {
    // Some mobile browsers only allow notifications from a service worker.
  }
}

/**
 * Permission state for desktop pop-ups plus a request() that must be called
 * from a click. Browsers ignore or silently mute a permission prompt that
 * isn't triggered by a user gesture, which is why asking on page load (the
 * old behaviour) left permission at "default" and no pop-up ever appeared.
 */
export function useDesktopNotificationPermission() {
  const [permission, setPermission] = useState<NotificationPermission | "unsupported">("default");
  useEffect(() => {
    const sync = () => setPermission(notificationPermission());
    sync();
    // The user can change it in the browser's site settings at any time.
    document.addEventListener("visibilitychange", sync);
    window.addEventListener("focus", sync);
    return () => {
      document.removeEventListener("visibilitychange", sync);
      window.removeEventListener("focus", sync);
    };
  }, []);
  const request = useCallback(async () => {
    if (notificationPermission() === "unsupported") return "unsupported" as const;
    try {
      const result = await Notification.requestPermission();
      setPermission(result);
      return result;
    } catch {
      setPermission(notificationPermission());
      return notificationPermission();
    }
  }, []);
  const sendTest = useCallback(() => {
    if (notificationPermission() !== "granted") return false;
    showPopUp({ title: "Aivory Mail", body: "Desktop notifications are on. New mail will show up like this.", tag: "aivory-mail-test" });
    return true;
  }, []);
  return { permission, request, sendTest };
}

/**
 * Gmail-web-style "new mail" notifications: a short chime + a desktop
 * Notification when the newest Inbox message changes while the tab is
 * unfocused, gated on the existing Settings > Notifications toggles
 * (previously wired up but never actually consumed).
 */
export function useNewMailNotifications(opts: {
  authFetch: (path: string, init?: RequestInit) => Promise<Response>;
  mailboxId: string;
  enabled: boolean;
}) {
  const { authFetch, mailboxId, enabled } = opts;
  const [settings, setSettings] = useState<NotifSettings>(DEFAULT_SETTINGS);
  const settingsRef = useRef(settings);
  settingsRef.current = settings;
  const lastSeenIdRef = useRef<string | null>(null);
  const baselinedRef = useRef(false);

  // Load the notification settings for this mailbox and, if the "new mail
  // banner" toggle is on, ask for permission (a no-op if already
  // granted/denied — browsers only prompt on "default").
  useEffect(() => {
    if (!enabled || !mailboxId) return;
    let cancelled = false;
    authFetch(`/v1/settings?category=notifications&mailbox_id=${encodeURIComponent(mailboxId)}`)
      .then(r => r.json())
      .then(j => {
        if (cancelled) return;
        const d = j?.data || {};
        const next: NotifSettings = {
          desktop_sound: (d.desktop_sound ?? "true") === "true",
          new_mail_banner: (d.new_mail_banner ?? "true") === "true",
        };
        setSettings(next);
        // Permission is requested from a click (banner / Settings), never here.
      })
      .catch(() => {});
    return () => { cancelled = true; };
  }, [authFetch, mailboxId, enabled]);

  // Browsers start AudioContext suspended until a user gesture — without
  // this unlock the "desktop sound" chime is created every time but never
  // audible. One shared context, resumed on first interaction.
  useEffect(() => {
    const unlock = () => {
      try {
        const Ctx = window.AudioContext || (window as any).webkitAudioContext;
        if (!Ctx) return;
        sharedCtx = sharedCtx || new Ctx();
        if (sharedCtx.state === "suspended") sharedCtx.resume().catch(() => {});
      } catch {}
    };
    window.addEventListener("pointerdown", unlock);
    window.addEventListener("keydown", unlock);
    return () => {
      window.removeEventListener("pointerdown", unlock);
      window.removeEventListener("keydown", unlock);
    };
  }, []);

  // A short two-tone chime, synthesized so the feature needs no bundled
  // audio asset (and nothing to license).
  const playChime = useCallback(() => {
    try {
      const Ctx = window.AudioContext || (window as any).webkitAudioContext;
      if (!Ctx) return;
      const ctx = sharedCtx || (sharedCtx = new Ctx());
      if (ctx.state === "suspended") ctx.resume().catch(() => {});
      const now = ctx.currentTime;
      [880, 1318.5].forEach((freq, i) => {
        const osc = ctx.createOscillator();
        const gain = ctx.createGain();
        osc.type = "sine";
        osc.frequency.value = freq;
        const start = now + i * 0.09;
        gain.gain.setValueAtTime(0, start);
        gain.gain.linearRampToValueAtTime(0.16, start + 0.01);
        gain.gain.exponentialRampToValueAtTime(0.0001, start + 0.32);
        osc.connect(gain).connect(ctx.destination);
        osc.start(start);
        osc.stop(start + 0.34);
      });
      // Shared context stays open for the next chime (closing it would
      // force a re-create that starts suspended again).
    } catch {}
  }, []);

  const pollRef = useRef<() => Promise<void>>(async () => {});
  pollRef.current = useCallback(async () => {
    if (!enabled || !mailboxId) return;
    try {
      const r = await authFetch(`/v1/messages?folder=Inbox&page=1&per_page=10&mailbox_id=${encodeURIComponent(mailboxId)}`);
      if (!r.ok) return;
      const j = await r.json();
      const list = (j?.data || []) as any[];
      const top = list[0];
      if (!top?.id) return;

      if (!baselinedRef.current) {
        // First check after a mailbox switch/mount: record the current
        // newest message as the baseline, don't notify for it.
        baselinedRef.current = true;
        lastSeenIdRef.current = top.id;
        return;
      }
      if (top.id === lastSeenIdRef.current) return;
      // Every message since the last check, not just the newest one.
      const fresh = pickNewMessages(list, lastSeenIdRef.current);
      lastSeenIdRef.current = top.id;
      if (fresh.length === 0) return;

      const s = settingsRef.current;
      if (s.desktop_sound) playChime();
      // Pop-ups only while you're looking elsewhere; in the tab, the
      // in-app toast already says it (Gmail does the same).
      const tabHidden = typeof document !== "undefined" && (document.hidden || !document.hasFocus());
      if (s.new_mail_banner && tabHidden && notificationPermission() === "granted") {
        buildPopUps(fresh).forEach(showPopUp);
      }
    } catch {}
  }, [authFetch, mailboxId, enabled, playChime]);

  useEffect(() => {
    if (!enabled || !mailboxId) return;
    baselinedRef.current = false;
    lastSeenIdRef.current = null;
    pollRef.current();
    const iv = setInterval(() => pollRef.current(), POLL_MS);
    // The realtime socket dispatches this on new mail — re-check now
    // instead of waiting for the next poll so the banner/chime is instant.
    const onPush = () => { pollRef.current(); };
    window.addEventListener("aivory:new-mail", onPush);
    return () => { clearInterval(iv); window.removeEventListener("aivory:new-mail", onPush); };
  }, [enabled, mailboxId]);
}

export default useNewMailNotifications;
