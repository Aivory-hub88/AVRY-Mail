"use client";
import { useEffect, useState, useRef } from "react";

const API = process.env.NEXT_PUBLIC_MAIL_API || "http://localhost:8095";

type Msg = { role: "user" | "assistant"; content: string; sources?: any; suggested?: any[] };

export default function AskAIAssistant({
  selected,
  threadId,
  mailboxId,
}: {
  selected?: any;
  threadId?: string;
  mailboxId?: string;
}) {
  const [question, setQuestion] = useState("");
  const [history, setHistory] = useState<Msg[]>([]);
  const [loading, setLoading] = useState(false);
  const [pushed, setPushed] = useState<string | null>(null);
  const listRef = useRef<HTMLDivElement>(null);

  // load chat history on mount / mailbox change
  useEffect(() => {
    const mid = mailboxId || "";
    fetch(`${API}/v1/ai/history?mailbox_id=${encodeURIComponent(mid)}&limit=10`)
      .then((r) => r.json())
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

  async function ask() {
    const q = question.trim();
    if (!q || loading) return;
    const userMsg: Msg = { role: "user", content: q };
    setHistory((h) => [...h, userMsg]);
    setQuestion("");
    setLoading(true);
    setPushed(null);
    try {
      const token = typeof window !== "undefined" ? localStorage.getItem("aivory_mail_token") : null;
      const email = typeof window !== "undefined" ? localStorage.getItem("aivory_mail_email") || "admin@aivory.id" : "admin@aivory.id";
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

      // auto hint push if model suggests
      if (data.auto_push_suggested) {
        // show subtle hint, not auto-push
      }
    } catch (e) {
      setHistory((h) => [...h, { role: "assistant", content: `Error: ${(e as Error).message}. Fallback heuristic akan dipakai.` }]);
    } finally {
      setLoading(false);
    }
  }

  async function pushToMissionControl(lastAnswer: string) {
    try {
      const token = typeof window !== "undefined" ? localStorage.getItem("aivory_mail_token") : null;
      const headers: Record<string, string> = { "content-type": "application/json" };
      if (token) headers["Authorization"] = `Bearer ${token}`;
      const title = selected?.subject ? `Email: ${selected.subject.slice(0, 60)}` : "Ask AI — Mail";
      const body = lastAnswer.slice(0, 500);
      const action_url = threadId ? `https://mail.aivory.uk/?thread_id=${threadId}` : "https://mail.aivory.uk/";
      const r = await fetch(`${API}/v1/ai/push-to-mission-control`, {
        method: "POST",
        headers,
        body: JSON.stringify({
          title,
          body,
          type: "email_assistant",
          action_url,
          metadata: { mailbox_id: mailboxId, message_id: selected?.id, question: history[history.length - 2]?.content },
        }),
      });
      const j = await r.json();
      if (j.success) {
        setPushed(j.data?.id || "ok");
        // also try to notify dashboard via localStorage event (same-origin widget polling will pick up)
        setTimeout(() => setPushed(null), 4000);
      }
    } catch {}
  }

  const lastAssistant = [...history].reverse().find((m) => m.role === "assistant");

  return (
    <div className="flex h-full flex-col rounded-xl border border-[#e8e0c8] bg-[#fefcf6] shadow-sm">
      <div className="flex items-center justify-between border-b border-[#e8e0c8] bg-[#f0ece0] px-4 py-3">
        <div className="flex items-center gap-2">
          <span className="flex h-7 w-7 items-center justify-center rounded-full bg-[#005a5e] text-sm text-white">✦</span>
          <div>
            <div className="text-sm font-semibold text-[#202124]">Ask AI Assistant</div>
          </div>
        </div>
        <span className="rounded-full bg-white px-2 py-0.5 text-[11px] font-medium text-[#005a5e] ring-1 ring-[#e8e0c8]">Sub-agent</span>
      </div>

      {/* context pill */}
      {selected?.subject && (
        <div className="border-b border-[#f0ece0] bg-[#f8f6ef] px-4 py-2 text-xs">
          <span className="font-semibold text-[#005a5e]">Context:</span> {selected.from} — {selected.subject.slice(0, 80)}
        </div>
      )}

      <div ref={listRef} className="flex-1 overflow-y-auto p-3 space-y-3">
        {history.length === 0 && (
          <div className="rounded-lg bg-[#f8f6ef] p-3 text-xs leading-relaxed text-zinc-600">
            <div className="font-semibold text-[#202124]">Coba tanya:</div>
            <ul className="mt-1 list-disc pl-4 space-y-1">
              <li>
                <button onClick={() => setQuestion("Ringkas inbox hari ini")} className="underline decoration-[#e8e0c8] hover:text-[#005a5e]">
                  Ringkas inbox hari ini
                </button>
              </li>
              <li>
                <button onClick={() => setQuestion("Buatkan draft balasan untuk email ini")} className="underline decoration-[#e8e0c8] hover:text-[#005a5e]">
                  Buatkan draft balasan untuk email ini
                </button>
              </li>
              <li>
                <button onClick={() => setQuestion("Cari invoice overdue")} className="underline decoration-[#e8e0c8] hover:text-[#005a5e]">
                  Cari invoice overdue
                </button>
              </li>
            </ul>
            <div className="mt-2 text-[11px] text-zinc-400">Jawaban memakai heuristic + AI gateway (OpenRouter/deepseek) dengan budget 2k thread memory.</div>
          </div>
        )}
        {history.map((m, i) => (
          <div key={i} className={`flex ${m.role === "user" ? "justify-end" : "justify-start"}`}>
            <div
              className={`max-w-[85%] rounded-2xl px-3 py-2 text-sm leading-relaxed ${
                m.role === "user" ? "bg-[#005a5e] text-white" : "bg-white border border-[#e8e0c8] text-zinc-800"
              }`}
            >
              <div className="whitespace-pre-wrap break-words">{m.content}</div>
              {m.suggested && m.suggested.length > 0 && (
                <div className="mt-2 flex flex-wrap gap-1.5">
                  {m.suggested.slice(0, 3).map((a: any, idx: number) => (
                    <span key={idx} className="rounded-full bg-[#f0ece0] px-2 py-0.5 text-[11px] text-[#005a5e]">
                      {a.label || a.action || a}
                    </span>
                  ))}
                </div>
              )}
            </div>
          </div>
        ))}
        {loading && (
          <div className="flex justify-start">
            <div className="rounded-2xl border border-[#e8e0c8] bg-white px-3 py-2 text-sm text-zinc-500">Thinking… zeroclaw vanilla</div>
          </div>
        )}
      </div>

      <div className="border-t border-[#e8e0c8] bg-white p-3">
        {lastAssistant && (
          <div className="mb-2 flex gap-2">
            <button
              onClick={() => pushToMissionControl(lastAssistant.content)}
              className="flex-1 rounded-full bg-[#005a5e] px-3 py-2 text-xs font-semibold text-white hover:bg-[#00454a] active:scale-[0.98]"
            >
              ↗ Push to Mission Control
            </button>
            <span className="self-center text-[11px] text-zinc-400">→ dashboard.aivory.id</span>
          </div>
        )}
        {pushed && <div className="mb-2 rounded-lg bg-emerald-50 px-3 py-2 text-xs text-emerald-800">Pushed to Mission Control ✓ ({pushed})</div>}
        <div className="flex gap-2">
          <input
            value={question}
            onChange={(e) => setQuestion(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                ask();
              }
            }}
            placeholder={selected ? "Tanya tentang email ini…" : "Tanya AI tentang inbox…"}
            className="flex-1 rounded-full border border-[#e8e0c8] bg-[#f8f6ef] px-4 py-2.5 text-sm placeholder:text-zinc-400 focus:bg-white focus:border-[#005a5e] focus:outline-none"
          />
          <button
            onClick={ask}
            disabled={loading || !question.trim()}
            className="rounded-full bg-zinc-900 px-4 py-2 text-sm font-semibold text-white hover:bg-black disabled:opacity-40"
          >
            Ask
          </button>
        </div>
        <div className="mt-1 text-center text-[11px] text-zinc-400">Enter to send · Shift+Enter for newline · Mission Control polls /v1/notifications</div>
      </div>
    </div>
  );
}
