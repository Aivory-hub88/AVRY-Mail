"use client";

// Inbox tabs (Primary / Promotions / Social / Updates / Forums). The server
// decides which tabs a mailbox shows (Settings → Inbox → Categories) and
// files every new message into one; these components only render the tabs
// and let the user move a conversation, which also teaches the server where
// that sender belongs.

export type InboxTab = { id: string; unread: number };

export const TAB_LABELS: Record<string, string> = {
  primary: "Primary",
  promotions: "Promotions",
  social: "Social",
  updates: "Updates",
  forums: "Forums",
};

export function InboxTabs({ tabs, active, onChange }: { tabs: InboxTab[]; active: string; onChange: (id: string) => void }) {
  if (tabs.length <= 1) return null;
  return (
    <div role="tablist" aria-label="Inbox tabs" className="flex gap-0.5 overflow-x-auto px-3 pb-2 [scrollbar-width:none]">
      {tabs.map((t) => {
        const on = t.id === active;
        return (
          <button
            key={t.id}
            role="tab"
            aria-selected={on}
            onClick={() => onChange(t.id)}
            className={`flex shrink-0 items-center gap-1 rounded-lg px-2 py-1 text-xs font-medium transition active:scale-[0.97] ${
              on
                ? "bg-zinc-900 text-white dark:bg-white dark:text-zinc-900"
                : "text-zinc-600 hover:bg-black/[0.05] dark:text-zinc-300 dark:hover:bg-white/10"
            }`}
          >
            {TAB_LABELS[t.id] || t.id}
            {t.unread > 0 && (
              <span className={`rounded px-1 text-[10px] font-semibold ${on ? "bg-white/20" : "bg-black/[0.06] dark:bg-white/10"}`}>{t.unread}</span>
            )}
          </button>
        );
      })}
    </div>
  );
}

/** "Move to tab" for an opened message or conversation. */
export function MoveToTab({ tabs, messageId, onMoved, authFetch }: {
  tabs: InboxTab[];
  messageId: string;
  onMoved: (tab: string) => void;
  authFetch: (path: string, init?: RequestInit) => Promise<Response>;
}) {
  if (tabs.length <= 1 || !messageId) return null;
  async function move(tab: string) {
    if (!tab) return;
    const r = await authFetch(`/v1/messages/${messageId}/category`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ category: tab, apply_to_sender: true }),
    });
    if (r.ok) onMoved(tab);
  }
  return (
    <select
      value=""
      onChange={(e) => move(e.target.value)}
      title="Move this conversation, and future mail from this sender, to a tab"
      aria-label="Move to tab"
      className="rounded-full border border-black/10 bg-white px-3 py-2 text-xs font-medium text-zinc-600 dark:border-zinc-600 dark:bg-zinc-800 dark:text-zinc-300"
    >
      <option value="">Move to tab…</option>
      {tabs.map((t) => (
        <option key={t.id} value={t.id}>{TAB_LABELS[t.id] || t.id}</option>
      ))}
    </select>
  );
}
