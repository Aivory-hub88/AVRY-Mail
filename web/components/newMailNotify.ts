/**
 * Pure logic behind the desktop "new mail" pop-ups (no DOM, no fetch), so it
 * can be tested on its own. See useNewMailNotifications for the wiring.
 */

export type InboxItem = {
  id: string;
  from?: string | null;
  from_name?: string | null;
  subject?: string | null;
  snippet?: string | null;
};

export type PopUp = {
  title: string;
  body: string;
  /** Same tag replaces an older pop-up instead of stacking a duplicate. */
  tag: string;
  /** Message to open when the pop-up is clicked; absent on the summary. */
  messageId?: string;
};

/** Pop-ups shown one by one before the rest collapse into a summary. */
export const MAX_INDIVIDUAL = 3;

/**
 * Messages that arrived since `lastSeenId`, newest first. `list` is the Inbox
 * newest-first. If `lastSeenId` has scrolled out of the page (more new mail
 * than the page holds), everything on the page is new.
 */
export function pickNewMessages(list: InboxItem[], lastSeenId: string | null): InboxItem[] {
  if (!lastSeenId) return [];
  const seenAt = list.findIndex((m) => m.id === lastSeenId);
  return seenAt === -1 ? list.slice() : list.slice(0, seenAt);
}

function sender(m: InboxItem): string {
  return (m.from_name || m.from || "").trim() || "New message";
}

function preview(m: InboxItem): string {
  return (m.subject || m.snippet || "").trim() || "You have new mail";
}

/**
 * One pop-up per new message, oldest first so the newest ends on top. More
 * than MAX_INDIVIDUAL: show the newest few plus one summary for the rest,
 * instead of flooding the screen.
 */
export function buildPopUps(fresh: InboxItem[]): PopUp[] {
  if (fresh.length === 0) return [];
  const shown = fresh.slice(0, MAX_INDIVIDUAL);
  const popUps: PopUp[] = shown
    .slice()
    .reverse()
    .map((m) => ({
      title: sender(m),
      body: preview(m),
      tag: `aivory-mail-${m.id}`,
      messageId: m.id,
    }));
  const rest = fresh.length - shown.length;
  if (rest > 0) {
    popUps.unshift({
      title: "Aivory Mail",
      body: `${rest} more new ${rest === 1 ? "message" : "messages"} in your Inbox`,
      tag: "aivory-mail-summary",
    });
  }
  return popUps;
}

/** Browser support and the current permission, without touching `window`
 *  during SSR. */
export function notificationPermission(): NotificationPermission | "unsupported" {
  if (typeof window === "undefined" || !("Notification" in window)) return "unsupported";
  return Notification.permission;
}
