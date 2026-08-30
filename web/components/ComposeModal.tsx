"use client";
import { useState, useRef } from "react";
const API = process.env.NEXT_PUBLIC_MAIL_API || "http://localhost:8095";

type Props = {
  open: boolean;
  onClose: () => void;
  onSent: () => void;
  defaultFrom: string;
  replyTo?: { to: string; subject: string; body: string; thread_id?: string };
};

export default function ComposeModal({ open, onClose, onSent, defaultFrom, replyTo }: Props) {
  const [from, setFrom] = useState(defaultFrom || "hello@demo.aivory.test");
  const [to, setTo] = useState(replyTo?.to || "");
  const [cc, setCc] = useState("");
  const [bcc, setBcc] = useState("");
  const [showCcBcc, setShowCcBcc] = useState(false);
  const [subject, setSubject] = useState(replyTo?.subject || "");
  const [body, setBody] = useState(replyTo?.body || "");
  const [isHtml, setIsHtml] = useState(false);
  const [files, setFiles] = useState<{ name: string; type: string; b64: string; size: number }[]>([]);
  const [sending, setSending] = useState(false);
  const [err, setErr] = useState("");
  const fileRef = useRef<HTMLInputElement>(null);

  async function handleFiles(list: FileList | null) {
    if (!list) return;
    const next: typeof files = [];
    for (const f of Array.from(list)) {
      if (f.size > 10 * 1024 * 1024) { setErr(`${f.name} exceeds 10MB`); continue; }
      const b64 = await new Promise<string>((res, rej) => {
        const r = new FileReader();
        r.onload = () => res((r.result as string).split(",")[1]);
        r.onerror = rej;
        r.readAsDataURL(f);
      });
      next.push({ name: f.name, type: f.type || "application/octet-stream", b64, size: f.size });
    }
    const total = [...files, ...next].reduce((a, b) => a + b.size, 0);
    if (total > 20 * 1024 * 1024) { setErr("Combined attachments exceed 20MB"); return; }
    if (files.length + next.length > 10) { setErr("Max 10 attachments"); return; }
    setFiles([...files, ...next]);
    setErr("");
  }

  function insertLink() {
    const url = prompt("Enter URL (https://...)");
    if (!url) return;
    const text = prompt("Link text (optional)", url) || url;
    const link = isHtml ? `<a href="${url}">${text}</a>` : `${text} (${url})`;
    setBody((b) => b + (b ? "\n" : "") + link);
    setIsHtml(true);
  }

  async function send() {
    setErr("");
    if (!to.trim()) { setErr("To required"); return; }
    if (!subject.trim()) { setErr("Subject required"); return; }
    if (!body.trim()) { setErr("Body required"); return; }
    setSending(true);
    try {
      const attachments = files.map((f) => ({ filename: f.name, content_type: f.type, content_base64: f.b64 }));
      const payload: any = {
        from: from.trim(),
        to: to.split(",").map((s) => s.trim()).filter(Boolean),
        subject: subject.trim(),
        text: isHtml ? undefined : body,
        html: isHtml ? body : undefined,
        attachments: attachments.length ? attachments : undefined,
      };
      if (cc.trim()) payload.cc = cc.split(",").map((s) => s.trim()).filter(Boolean);
      if (bcc.trim()) payload.bcc = bcc.split(",").map((s) => s.trim()).filter(Boolean);
      if (replyTo?.thread_id) payload.thread_id = replyTo.thread_id;

      const r = await fetch(`${API}/v1/send`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(payload),
      });
      const j = await r.json();
      if (!j.success) throw new Error(j.error || "Send failed");
      onSent();
      onClose();
      setTo(""); setSubject(replyTo?.subject || ""); setBody(""); setFiles([]); setCc(""); setBcc("");
    } catch (e: any) {
      setErr(e.message || String(e));
    } finally {
      setSending(false);
    }
  }

  if (!open) return null;
  return (
    <div className="fixed inset-0 z-50 flex items-end justify-center bg-black/30 p-4 sm:items-center">
      <div className="flex max-h-[92vh] w-full max-w-[640px] flex-col overflow-hidden rounded-xl border border-zinc-200 bg-white shadow-xl">
        <div className="flex items-center justify-between border-b border-zinc-200 bg-zinc-50 px-4 py-3">
          <span className="text-sm font-semibold">{replyTo ? "Reply" : "New message"}</span>
          <button onClick={onClose} className="rounded p-1 text-zinc-500 hover:bg-zinc-200">
            ✕
          </button>
        </div>

        <div className="flex-1 space-y-3 overflow-y-auto p-4">
          <div>
            <label className="text-xs font-medium text-zinc-600">From</label>
            <input value={from} onChange={(e) => setFrom(e.target.value)} className="mt-1 w-full rounded-lg border border-zinc-200 px-3 py-2 text-sm focus:border-zinc-900 focus:outline-none focus:ring-1 focus:ring-zinc-900" />
          </div>

          <div>
            <label className="text-xs font-medium text-zinc-600">To</label>
            <div className="mt-1 flex gap-2">
              <input value={to} onChange={(e) => setTo(e.target.value)} placeholder="bob@example.com, alice@example.com" className="flex-1 rounded-lg border border-zinc-200 px-3 py-2 text-sm focus:border-zinc-900 focus:outline-none" />
              <button onClick={() => setShowCcBcc(!showCcBcc)} className="whitespace-nowrap rounded-lg border border-zinc-200 bg-white px-3 text-xs font-medium hover:bg-zinc-50">
                {showCcBcc ? "Hide Cc/Bcc" : "Cc/Bcc"}
              </button>
            </div>
          </div>

          {showCcBcc && (
            <>
              <div>
                <label className="text-xs font-medium text-zinc-600">Cc</label>
                <input value={cc} onChange={(e) => setCc(e.target.value)} placeholder="cc@example.com" className="mt-1 w-full rounded-lg border border-zinc-200 px-3 py-2 text-sm" />
              </div>
              <div>
                <label className="text-xs font-medium text-zinc-600">Bcc</label>
                <input value={bcc} onChange={(e) => setBcc(e.target.value)} placeholder="bcc@example.com" className="mt-1 w-full rounded-lg border border-zinc-200 px-3 py-2 text-sm" />
              </div>
            </>
          )}

          <div>
            <label className="text-xs font-medium text-zinc-600">Subject</label>
            <input value={subject} onChange={(e) => setSubject(e.target.value)} className="mt-1 w-full rounded-lg border border-zinc-200 px-3 py-2 text-sm focus:border-zinc-900 focus:outline-none" />
          </div>

          <div>
            <div className="flex items-center justify-between">
              <label className="text-xs font-medium text-zinc-600">Body</label>
              <div className="flex gap-1">
                <button onClick={insertLink} className="rounded border border-zinc-200 bg-white px-2 py-1 text-xs hover:bg-zinc-50" title="Insert link">
                  🔗 Link
                </button>
                <button onClick={() => setIsHtml(!isHtml)} className={`rounded border px-2 py-1 text-xs ${isHtml ? "border-zinc-900 bg-zinc-900 text-white" : "border-zinc-200 bg-white"}`}>
                  {isHtml ? "HTML" : "Text"}
                </button>
              </div>
            </div>
            <textarea value={body} onChange={(e) => setBody(e.target.value)} rows={8} placeholder={isHtml ? "<p>Hello <a href='https://...'>link</a></p>" : "Write your message... Tip: use 🔗 Link to insert URLs"} className="mt-1 w-full rounded-lg border border-zinc-200 px-3 py-2 text-sm leading-relaxed focus:border-zinc-900 focus:outline-none" />
            <div className="mt-1 text-[11px] text-zinc-400">{isHtml ? "HTML mode — links will render as clickable." : "Plain text — links inserted as text (url). Toggle HTML to embed."}</div>
          </div>

          <div>
            <div className="flex items-center gap-2">
              <button onClick={() => fileRef.current?.click()} className="rounded-lg border border-zinc-200 bg-white px-3 py-1.5 text-xs font-medium hover:bg-zinc-50">
                📎 Attach files
              </button>
              <span className="text-xs text-zinc-500">{files.length ? `${files.length} file(s) — ${(files.reduce((a,b)=>a+b.size,0)/1024).toFixed(1)} KB` : "Max 10 files, 10MB each, 20MB total"}</span>
            </div>
            <input ref={fileRef} type="file" multiple hidden onChange={(e) => handleFiles(e.target.files)} />
            {files.length > 0 && (
              <div className="mt-2 space-y-1">
                {files.map((f, i) => (
                  <div key={i} className="flex items-center justify-between rounded-lg border border-zinc-200 bg-zinc-50 px-3 py-1.5 text-xs">
                    <span className="truncate">{f.name} · {(f.size/1024).toFixed(1)} KB</span>
                    <button onClick={() => setFiles(files.filter((_, j) => j !== i))} className="ml-2 rounded px-1.5 py-0.5 text-zinc-500 hover:bg-white">
                      ✕
                    </button>
                  </div>
                ))}
              </div>
            )}
          </div>

          {err && <div className="rounded-lg bg-red-50 px-3 py-2 text-xs font-medium text-red-700 ring-1 ring-red-200">{err}</div>}
        </div>

        <div className="flex items-center justify-between border-t border-zinc-200 bg-zinc-50 px-4 py-3">
          <div className="text-[11px] text-zinc-500">From hello@demo.aivory.test → SMTP (or Cloudflare if configured)</div>
          <div className="flex gap-2">
            <button onClick={onClose} className="rounded-lg border border-zinc-200 bg-white px-4 py-2 text-sm font-medium hover:bg-zinc-50">
              Cancel
            </button>
            <button onClick={send} disabled={sending} className="rounded-lg bg-zinc-900 px-5 py-2 text-sm font-medium text-white hover:bg-black disabled:opacity-50">
              {sending ? "Sending..." : "Send"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
