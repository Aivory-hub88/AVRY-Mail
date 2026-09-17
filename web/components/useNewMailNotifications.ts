"use client";
import { useCallback, useEffect, useRef, useState } from "react";

type NotifSettings = { desktop_sound: boolean; new_mail_banner: boolean };
const DEFAULT_SETTINGS: NotifSettings = { desktop_sound: true, new_mail_banner: true };
const POLL_MS = 20000;

// Shared across hook instances so the gesture unlock applies everywhere.
let sharedCtx: AudioContext | null = null;

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
        if (next.new_mail_banner && typeof window !== "undefined" && "Notification" in window && Notification.permission === "default") {
          Notification.requestPermission().catch(() => {});
        }
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
      const r = await authFetch(`/v1/messages?folder=Inbox&page=1&per_page=1&mailbox_id=${encodeURIComponent(mailboxId)}`);
      if (!r.ok) return;
      const j = await r.json();
      const top = (j?.data || [])[0];
      if (!top?.id) return;

      if (!baselinedRef.current) {
        // First check after a mailbox switch/mount: record the current
        // newest message as the baseline, don't notify for it.
        baselinedRef.current = true;
        lastSeenIdRef.current = top.id;
        return;
      }
      if (top.id === lastSeenIdRef.current) return;
      lastSeenIdRef.current = top.id;

      const s = settingsRef.current;
      if (s.desktop_sound) playChime();
      const tabHidden = typeof document !== "undefined" && (document.hidden || !document.hasFocus());
      if (s.new_mail_banner && tabHidden && typeof window !== "undefined" && "Notification" in window && Notification.permission === "granted") {
        const n = new Notification(top.from || "New message", {
          body: top.subject || top.snippet || "You have new mail",
          icon: "/Favicon_Aivory-Mail.svg",
          tag: "aivory-mail-new-mail",
        });
        n.onclick = () => {
          window.focus();
          n.close();
        };
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
