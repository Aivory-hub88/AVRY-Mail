"use client";
import { useEffect, useState } from "react";

const API = process.env.NEXT_PUBLIC_MAIL_API || "http://localhost:8095";
type Msg = { id: string; from: string; subject: string; snippet: string; created_at: string; is_read: boolean };

export default function InboxPage() {
  const [msgs, setMsgs] = useState<Msg[]>([]);
  const [selected, setSelected] = useState<any>(null);
  const [activeFolder, setActiveFolder] = useState("Inbox");

  useEffect(() => {
    fetch(`${API}/v1/messages?folder=${activeFolder}&per_page=20`)
      .then((r) => r.json())
      .then((j) => setMsgs(j.data || []))
      .catch(() => {});
  }, [activeFolder, selected]);

  async function open(id: string) {
    const r = await fetch(`${API}/v1/messages/${id}`);
    const j = await r.json();
    setSelected(j.data);
  }

  return (
    <div className="flex h-screen bg-zinc-50 text-zinc-900">
      {/* Sidebar */}
      <aside className="flex w-[280px] shrink-0 flex-col border-r border-zinc-200 bg-white">
        <div className="border-b border-zinc-100 px-4 py-5">
          <h1 className="text-xl font-bold tracking-tight">Aivory Mail</h1>
          <p className="mt-1 text-xs leading-relaxed text-zinc-500">
            Business email, without
            <br /> the email tax.
          </p>
        </div>

        <nav className="flex flex-1 flex-col gap-1.5 px-3 py-4">
          {["Inbox", "Sent", "Drafts", "Spam", "Trash"].map((f) => (
            <button
              key={f}
              onClick={() => setActiveFolder(f)}
              className={`rounded-lg border px-3 py-2.5 text-left text-sm font-medium transition ${
                f === activeFolder
                  ? "border-zinc-900 bg-zinc-900 text-white shadow-sm"
                  : "border-zinc-200 bg-white text-zinc-700 hover:bg-zinc-50 hover:border-zinc-300"
              }`}
            >
              <span className="flex items-center justify-between">
                {f}
                {f === "Inbox" && msgs.length > 0 && (
                  <span
                    className={`ml-2 rounded-full px-2 py-0.5 text-xs font-semibold ${
                      activeFolder === "Inbox" ? "bg-white text-zinc-900" : "bg-zinc-900 text-white"
                    }`}
                  >
                    {msgs.length}
                  </span>
                )}
              </span>
            </button>
          ))}
        </nav>

        <div className="mx-3 mb-4 rounded-xl border border-zinc-100 bg-zinc-50 p-3">
          <div className="text-xs font-semibold text-zinc-900">AI Triage</div>
          <div className="mt-1 text-[11px] leading-relaxed text-zinc-500">
            Email → Intelligence → Workflow → Action
          </div>
          <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-zinc-200">
            <div className="h-full w-2/3 rounded-full bg-zinc-900" />
          </div>
          <div className="mt-1.5 text-[10px] text-zinc-400">Heuristic + Cerveau gateway</div>
        </div>

        <div className="border-t border-zinc-100 px-3 py-3">
          <div className="text-[11px] text-zinc-400">MAIL_MODE: vps · storage: local</div>
          <a
            href="http://localhost:8095/health"
            target="_blank"
            className="text-[11px] font-medium text-zinc-600 underline decoration-zinc-300 underline-offset-2 hover:text-zinc-900"
          >
            API health ↗
          </a>
        </div>
      </aside>

      {/* List + Detail */}
      <section className="flex min-w-0 flex-1">
        {/* Message list */}
        <div className="flex w-[400px] shrink-0 flex-col border-r border-zinc-200 bg-white">
          <div className="sticky top-0 z-10 flex items-center justify-between border-b border-zinc-200 bg-white px-4 py-3">
            <span className="text-sm font-semibold">
              {activeFolder} — {msgs.length}
            </span>
            <span className="rounded-full bg-zinc-900 px-2 py-0.5 text-[11px] font-semibold text-white">
              {msgs.filter((m) => !m.is_read).length} new
            </span>
          </div>

          <div className="flex-1 overflow-y-auto">
            {msgs.length === 0 && (
              <div className="p-8 text-center">
                <div className="mx-auto flex h-10 w-10 items-center justify-center rounded-full bg-zinc-100 text-zinc-400">
                  ✉️
                </div>
                <p className="mt-3 text-sm font-medium text-zinc-700">No messages yet</p>
                <p className="mt-1 text-xs text-zinc-500">Send a test email to your mailbox.</p>
              </div>
            )}
            {msgs.map((m) => (
              <button
                key={m.id}
                onClick={() => open(m.id)}
                className={`flex w-full flex-col gap-1 border-b border-zinc-100 px-4 py-3 text-left transition hover:bg-zinc-50 ${
                  selected?.id === m.id ? "bg-zinc-50 ring-1 ring-inset ring-zinc-900" : "bg-white"
                }`}
              >
                <div className="flex items-center gap-2">
                  <span
                    className={`truncate text-[13px] ${m.is_read ? "font-normal text-zinc-700" : "font-semibold text-zinc-900"}`}
                  >
                    {m.from}
                  </span>
                  {!m.is_read && <span className="h-2 w-2 shrink-0 rounded-full bg-blue-500" />}
                  <span className="ml-auto shrink-0 text-[11px] text-zinc-400">
                    {new Date(m.created_at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
                  </span>
                </div>
                <div className="truncate text-[13px] font-medium text-zinc-900">
                  {m.subject || "(no subject)"}
                </div>
                <div className="line-clamp-2 text-xs leading-relaxed text-zinc-500">{m.snippet}</div>
              </button>
            ))}
          </div>
        </div>

        {/* Detail */}
        <div className="flex min-w-0 flex-1 flex-col bg-zinc-50">
          {!selected ? (
            <div className="flex flex-1 flex-col items-center justify-center p-10 text-center">
              <div className="rounded-2xl border border-dashed border-zinc-300 bg-white px-8 py-10">
                <div className="mx-auto flex h-12 w-12 items-center justify-center rounded-xl bg-zinc-900 text-white">
                  ✉️
                </div>
                <p className="mt-4 text-sm font-semibold text-zinc-900">Select a message</p>
                <p className="mt-1 max-w-[260px] text-xs leading-relaxed text-zinc-500">
                  Click a message on the left. Intelligence panel will show intent, urgency, and suggested actions.
                </p>
              </div>
            </div>
          ) : (
            <div className="flex flex-1 flex-col overflow-y-auto">
              <div className="border-b border-zinc-200 bg-white px-6 py-5">
                <h2 className="text-lg font-bold leading-tight text-zinc-900">{selected.subject}</h2>
                <div className="mt-2 flex flex-wrap items-center gap-2 text-xs text-zinc-500">
                  <span className="rounded-full border border-zinc-200 bg-white px-2.5 py-1 font-medium text-zinc-700">
                    From {selected.from}
                  </span>
                  <span>{new Date(selected.created_at).toLocaleString()}</span>
                  <span className="rounded-full bg-emerald-50 px-2 py-0.5 text-[11px] font-semibold text-emerald-700 ring-1 ring-emerald-200">
                    Inbox
                  </span>
                </div>
              </div>

              <div className="space-y-6 p-6">
                <div className="rounded-xl border border-zinc-200 bg-white p-5 shadow-sm">
                  <div className="whitespace-pre-wrap text-[14px] leading-6 text-zinc-800">
                    {selected.body_text || selected.snippet}
                  </div>
                  {selected.body_html && (
                    <div
                      className="prose prose-sm mt-4 max-w-none rounded-lg border border-zinc-100 bg-zinc-50 p-4"
                      dangerouslySetInnerHTML={{ __html: selected.body_html }}
                    />
                  )}
                </div>

                <div className="flex flex-wrap gap-2">
                  <button className="rounded-lg bg-zinc-900 px-4 py-2 text-sm font-medium text-white shadow hover:bg-black">
                    Reply
                  </button>
                  <button className="rounded-lg border border-zinc-200 bg-white px-4 py-2 text-sm font-medium text-zinc-700 hover:bg-zinc-50">
                    Forward
                  </button>
                  <button className="rounded-lg border border-zinc-200 bg-white px-4 py-2 text-sm font-medium text-zinc-500 hover:bg-zinc-50">
                    Archive
                  </button>
                  <button className="ml-auto rounded-lg border border-amber-200 bg-amber-50 px-3 py-2 text-xs font-semibold text-amber-800">
                    AI: Create Finance Task
                  </button>
                </div>

                <div className="rounded-xl border border-zinc-200 bg-white p-4">
                  <div className="text-xs font-semibold text-zinc-900">Intelligence (heuristic)</div>
                  <div className="mt-2 flex flex-wrap gap-1.5">
                    <span className="rounded-full bg-zinc-900 px-2.5 py-1 text-xs font-medium text-white">invoice</span>
                    <span className="rounded-full bg-red-50 px-2.5 py-1 text-xs font-semibold text-red-700 ring-1 ring-red-200">
                      High urgency
                    </span>
                    <span className="rounded-full bg-zinc-100 px-2.5 py-1 text-xs text-zinc-700">AED 18,500</span>
                  </div>
                  <div className="mt-3 text-xs leading-relaxed text-zinc-500">
                    Email will trigger <span className="font-semibold text-zinc-700">Aivory Workflow</span> → create task, notify finance, draft reminder after approval.
                  </div>
                </div>
              </div>
            </div>
          )}
        </div>
      </section>
    </div>
  );
}
