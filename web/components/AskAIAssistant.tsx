"use client";
import { useEffect, useState, useRef } from "react";
import { AISparkle } from "./AIAssistantButton";

const API = process.env.NEXT_PUBLIC_MAIL_API || "http://localhost:8095";

type Msg = { role: "user" | "assistant"; content: string; sources?: any; suggested?: any[] };

function Icon({ d, size = 16, cls = "" }: { d: string; size?: number; cls?: string }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round" className={cls} aria-hidden>
      <path d={d} />
    </svg>
  );
}
const I = {
  minimize: "M6 9l6 6 6-6",
  send: "M12 19V5M5 12l7-7 7 7",
  copy: "M9 9h10v10H9zM5 15V5h10",
  check: "M5 12l5 5L20 7",
  edit: "M4 20h4L19 9l-4-4L4 16v4z",
  push: "M7 17L17 7M9 7h8v8",
  reply: "M9 14L4 9l5-5M4 9h10a6 6 0 016 6v4",
};

function renderText(s: string) {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>")
    .replace(/\n/g, "<br />");
}

export default function AskAIAssistant({
  selected,
  threadId,
  mailboxId,
  onMinimize,
  onApplyDraft,
}: {
  selected?: any;
  threadId?: string;
  mailboxId?: string;
  onMinimize?: () => void;
  onApplyDraft?: (text: string) => void;
}) {
  const [question, setQuestion] = useState("");
  const [history, setHistory] = useState<Msg[]>([]);
  const [loading, setLoading] = useState(false);
  const [pushed, setPushed] = useState<number | null>(null);
  const [copied, setCopied] = useState<number | null>(null);
  const [editingIdx, setEditingIdx] = useState<number | null>(null);
  const [editText, setEditText] = useState("");
  const listRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  // load chat history on mount / mailbox change — scoped to the logged-in mailbox
  useEffect(() => {
    const mid = mailboxId || "";
    if (!mid) return;
    const token = typeof window !== "undefined"
      ? (localStorage.getItem("aivory_mail_token") || sessionStorage.getItem("aivory_mail_token"))
      : null;
    const headers: Record<string, string> = {};
    if (token) headers["Authorization"] = `Bearer ${token}`;
    fetch(`${API}/v1/ai/history?mailbox_id=${encodeURIComponent(mid)}&limit=10`, { headers })
      .then((r) => {
        if (!r.ok) throw new Error(String(r.status));
        return r.json();
      })
      .then((j) => {
        const rows: any[] = j.data || [];
        // map to chat history (reverse chronological -> chronological)
        const msgs: Msg[] = [];
        for (const r of rows.slice().reverse()) {
          msgs.push({ role: "user", content: r.question });
          msgs.push({ role: "assistant", content: r.answer });
        }
        if (msgs.length) setHistory(msgs);
      })
      .catch(() => {});
  }, [mailboxId]);

  useEffect(() => {
    listRef.current?.scrollTo({ top: listRef.current.scrollHeight, behavior: "smooth" });
  }, [history, loading]);

  // Composer grows with its content up to ~6 lines.
  useEffect(() => {
    const el = inputRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 144)}px`;
    el.style.overflowY = el.scrollHeight > 144 ? "auto" : "hidden";
  }, [question]);

  const hasContext = !!(selected?.id || threadId);
  const quickActions = [
    { label: "Draft reply", q: "Draft a professional reply to this email, ready to send. Keep it short." },
    { label: "Summarize", q: "Summarize this email in 3 bullets: request, deadline, action needed." },
    { label: "Translate", q: "Translate this email to Indonesian, preserving tone and formatting." },
    { label: "CRM extract", q: "Extract CRM data from this email as JSON: contact name, company, role, email, phone, deal intent, next action." },
  ];
  const starters = hasContext
    ? ["Buatkan draft balasan untuk email ini", "Apa yang diminta di email ini?", "Ada deadline yang perlu gue perhatikan?"]
    : ["Ringkas inbox hari ini", "Cari invoice overdue", "Email mana yang belum gue balas?"];

  async function ask(preset?: string) {
    const q = (preset ?? question).trim();
    if (!q || loading) return;
    const userMsg: Msg = { role: "user", content: q };
    setHistory((h) => [...h, userMsg]);
    setQuestion("");
    setLoading(true);
    setPushed(null);
    try {
      const token = typeof window !== "undefined"
      ? (localStorage.getItem("aivory_mail_token") || sessionStorage.getItem("aivory_mail_token"))
      : null;
      const email = typeof window !== "undefined"
        ? (localStorage.getItem("aivory_mail_email") || sessionStorage.getItem("aivory_mail_email") || "")
        : "";
      const headers: Record<string, string> = { "content-type": "application/json" };
      if (token) headers["Authorization"] = `Bearer ${token}`;
      const r = await fetch(`${API}/v1/ai/ask`, {
        method: "POST",
        headers,
        body: JSON.stringify({
          question: q,
          context: {
            mailbox_id: mailboxId || undefined,
            message_id: selected?.id || undefined,
            thread_id: threadId || selected?.thread_id || undefined,
          },
          user_email: email,
        }),
      });
      const j = await r.json();
      const data = j.data || j;
      const answer: string = data.answer || data.draft || JSON.stringify(data, null, 2);
      const suggested = data.suggested_actions || [];
      setHistory((h) => [...h, { role: "assistant", content: answer, sources: data.sources, suggested }]);
    } catch (e) {
      setHistory((h) => [...h, { role: "assistant", content: `Error: ${(e as Error).message}. Fallback heuristic akan dipakai.` }]);
    } finally {
      setLoading(false);
    }
  }

  async function pushToMissionControl(idx: number, answer: string) {
    try {
      const token = typeof window !== "undefined"
      ? (localStorage.getItem("aivory_mail_token") || sessionStorage.getItem("aivory_mail_token"))
      : null;
      const headers: Record<string, string> = { "content-type": "application/json" };
      if (token) headers["Authorization"] = `Bearer ${token}`;
      const title = selected?.subject ? `Email: ${selected.subject.slice(0, 60)}` : "Ask AI — Mail";
      const body = answer.slice(0, 500);
      const action_url = threadId ? `https://mail.aivory.uk/?thread_id=${threadId}` : "https://mail.aivory.uk/";
      const r = await fetch(`${API}/v1/ai/push-to-mission-control`, {
        method: "POST",
        headers,
        body: JSON.stringify({
          title,
          body,
          type: "email_assistant",
          action_url,
          metadata: { mailbox_id: mailboxId, message_id: selected?.id, question: history[idx - 1]?.content },
        }),
      });
      const j = await r.json();
      if (j.success) {
        setPushed(idx);
        setTimeout(() => setPushed(null), 4000);
      }
    } catch {}
  }

  function copy(idx: number, text: string) {
    navigator.clipboard?.writeText(text).then(() => {
      setCopied(idx);
      setTimeout(() => setCopied(null), 1500);
    }).catch(() => {});
  }

  const ghostBtn = "inline-flex items-center gap-1 rounded-md px-1.5 py-1 text-xs text-zinc-500 transition-colors hover:bg-black/[0.05] hover:text-zinc-800 dark:text-zinc-400 dark:hover:bg-white/10 dark:hover:text-zinc-100";

  return (
    <div className="flex h-full flex-col bg-[#fefcf6] dark:bg-zinc-900">
      {/* header */}
      <div className="flex items-center gap-3 border-b border-black/[0.06] px-4 py-3 dark:border-white/[0.08]">
        <span className="flex h-8 w-8 shrink-0 rounded-full bg-gradient-to-br from-[#d80000] to-[#f7c34d] p-[1.5px] shadow-sm">
          <span className="flex h-full w-full items-center justify-center rounded-full bg-zinc-950"><AISparkle size={15} /></span>
        </span>
        <div className="min-w-0 flex-1">
          <div className="text-sm font-semibold leading-tight text-zinc-900 dark:text-white">Aivory Assistant</div>
          <div className="truncate text-xs text-zinc-500 dark:text-zinc-400">
            {selected?.subject ? selected.subject : threadId ? "This conversation" : "Your inbox"}
          </div>
        </div>
        {onMinimize && (
          <button
            onClick={onMinimize}
            className="flex h-8 w-8 items-center justify-center rounded-full text-zinc-500 transition-colors hover:bg-black/[0.05] hover:text-zinc-900 dark:text-zinc-400 dark:hover:bg-white/10 dark:hover:text-white"
            title="Minimize"
            aria-label="Minimize assistant"
          >
            <Icon d={I.minimize} size={18} />
          </button>
        )}
      </div>

      {/* messages */}
      <div ref={listRef} className="flex-1 space-y-5 overflow-y-auto px-4 py-4">
        {history.length === 0 && !loading && (
          <div className="flex h-full flex-col justify-end gap-3 pb-1">
            <div>
              <div className="text-base font-semibold text-zinc-900 dark:text-white">Ada yang bisa dibantu?</div>
              <div className="mt-0.5 text-xs text-zinc-500 dark:text-zinc-400">
                {hasContext ? "Tanya apa saja tentang email yang sedang dibuka." : "Tanya apa saja tentang inbox lu."}
              </div>
            </div>
            <div className="flex flex-col gap-1.5">
              {starters.map((s) => (
                <button
                  key={s}
                  onClick={() => ask(s)}
                  className="rounded-xl border border-black/[0.07] bg-white/60 px-3 py-2 text-left text-sm text-zinc-700 transition-colors hover:border-[#ff6d00]/40 hover:bg-white dark:border-white/[0.08] dark:bg-white/[0.03] dark:text-zinc-300 dark:hover:bg-white/[0.06]"
                >
                  {s}
                </button>
              ))}
            </div>
          </div>
        )}

        {history.map((m, i) =>
          m.role === "user" ? (
            <div key={i} className="flex justify-end">
              <div className="max-w-[85%] whitespace-pre-wrap break-words rounded-2xl rounded-br-md bg-zinc-900 px-3.5 py-2 text-sm leading-relaxed text-white dark:bg-white/[0.1]">
                {m.content}
              </div>
            </div>
          ) : (
            <div key={i} className="group flex gap-2.5">
              <span className="mt-0.5 flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-[#f7c34d]/10">
                <AISparkle size={14} />
              </span>
              <div className="min-w-0 flex-1">
                {editingIdx === i ? (
                  <div>
                    <textarea
                      value={editText}
                      onChange={(e) => setEditText(e.target.value)}
                      rows={8}
                      autoFocus
                      className="w-full resize-y rounded-xl border border-black/10 bg-white p-3 text-sm leading-relaxed focus:border-[#ff6d00]/60 focus:outline-none focus:ring-2 focus:ring-[#ff6d00]/15 dark:border-white/10 dark:bg-zinc-800 dark:text-zinc-100"
                    />
                    <div className="mt-2 flex gap-1.5">
                      <button
                        onClick={() => { onApplyDraft?.(editText); setEditingIdx(null); }}
                        disabled={!editText.trim()}
                        className="rounded-full bg-[#ff6d00] px-3 py-1 text-xs font-semibold text-white transition-colors hover:bg-[#e65e00] disabled:opacity-40"
                      >
                        Use edited reply
                      </button>
                      <button onClick={() => setEditingIdx(null)} className={ghostBtn}>Cancel</button>
                    </div>
                  </div>
                ) : (
                  <div
                    className="break-words text-sm leading-relaxed text-zinc-800 dark:text-zinc-200"
                    dangerouslySetInnerHTML={{ __html: renderText(m.content) }}
                  />
                )}
                {m.suggested && m.suggested.length > 0 && (
                  <div className="mt-2 flex flex-wrap gap-1.5">
                    {m.suggested.slice(0, 3).map((a: any, idx: number) => (
                      <span key={idx} className="rounded-full bg-black/[0.04] px-2 py-0.5 text-xs text-zinc-600 dark:bg-white/10 dark:text-zinc-300">
                        {a.label || a.action || a}
                      </span>
                    ))}
                  </div>
                )}
                {editingIdx !== i && (
                  <div className="mt-2 flex flex-wrap items-center gap-0.5">
                    {onApplyDraft && (
                      <button
                        onClick={() => onApplyDraft(m.content)}
                        className="mr-1 inline-flex items-center gap-1 rounded-full bg-[#ff6d00] px-2.5 py-1 text-xs font-semibold text-white transition-colors hover:bg-[#e65e00]"
                      >
                        <Icon d={I.reply} size={12} /> Use as reply
                      </button>
                    )}
                    {onApplyDraft && (
                      <button onClick={() => { setEditText(m.content); setEditingIdx(i); }} className={ghostBtn} title="Edit before using">
                        <Icon d={I.edit} size={13} /> Edit
                      </button>
                    )}
                    <button onClick={() => copy(i, m.content)} className={ghostBtn} title={copied === i ? "Copied" : "Copy"} aria-label="Copy">
                      <Icon d={copied === i ? I.check : I.copy} size={13} />
                    </button>
                    <button onClick={() => pushToMissionControl(i, m.content)} className={ghostBtn} title="Push to Mission Control (dashboard.aivory.id)" aria-label="Push to Mission Control">
                      <Icon d={pushed === i ? I.check : I.push} size={13} />{pushed === i && " Sent"}
                    </button>
                  </div>
                )}
              </div>
            </div>
          )
        )}

        {loading && (
          <div className="flex gap-2.5">
            <span className="mt-0.5 flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-[#f7c34d]/10">
              <AISparkle size={14} className="animate-pulse" />
            </span>
            <div className="flex items-center gap-1 py-2" aria-label="Thinking">
              {[0, 150, 300].map((d) => (
                <span key={d} className="h-1.5 w-1.5 animate-bounce rounded-full bg-zinc-400 dark:bg-zinc-500" style={{ animationDelay: `${d}ms` }} />
              ))}
            </div>
          </div>
        )}
      </div>

      {/* composer */}
      <div className="px-3 pb-3 pt-1">
        {hasContext && (
          <div className="-mx-3 mb-2 flex gap-1.5 overflow-x-auto px-3 [scrollbar-width:none]">
            {quickActions.map((a) => (
              <button
                key={a.label}
                onClick={() => ask(a.q)}
                disabled={loading}
                className="shrink-0 rounded-full border border-black/[0.08] px-2.5 py-1 text-xs font-medium text-zinc-600 transition-colors hover:border-[#ff6d00]/40 hover:text-zinc-900 disabled:opacity-40 dark:border-white/10 dark:text-zinc-300 dark:hover:text-white"
              >
                {a.label}
              </button>
            ))}
          </div>
        )}
        <div className="flex items-end gap-2 rounded-2xl border border-black/10 bg-white p-1.5 pl-3.5 shadow-sm transition-[border-color,box-shadow] focus-within:border-[#ff6d00]/50 focus-within:ring-4 focus-within:ring-[#ff6d00]/10 dark:border-white/10 dark:bg-zinc-800">
          <textarea
            ref={inputRef}
            value={question}
            rows={1}
            onChange={(e) => setQuestion(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
                e.preventDefault();
                ask();
              }
            }}
            placeholder={hasContext ? "Tanya tentang email ini…" : "Tanya tentang inbox…"}
            className="max-h-36 flex-1 resize-none bg-transparent py-1.5 text-sm leading-relaxed text-zinc-900 placeholder:text-zinc-400 focus:outline-none dark:text-zinc-100 dark:placeholder:text-zinc-500"
          />
          <button
            onClick={() => ask()}
            disabled={loading || !question.trim()}
            className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-[#ff6d00] text-white transition-[background-color,opacity,transform] hover:bg-[#e65e00] active:scale-95 disabled:bg-zinc-300 disabled:opacity-60 dark:disabled:bg-zinc-700"
            title="Send (Enter)"
            aria-label="Send"
          >
            <Icon d={I.send} size={16} />
          </button>
        </div>
      </div>
    </div>
  );
}
