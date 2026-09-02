"use client";
import { useState, useRef, useEffect } from "react";
const API = process.env.NEXT_PUBLIC_MAIL_API || "http://localhost:8095";
function Ico({ d, size = 14, cls = "" }: { d: string; size?: number; cls?: string }) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.65} strokeLinecap="round" strokeLinejoin="round" className={cls} aria-hidden><path d={d} /></svg>;
}
const P = {
  send: "M22 2L11 13 M22 2l-7 20-4-9-9-4 20-7z",
  link: "M10 13a5 5 0 0 1 0-7l1-1a5 5 0 0 1 7 7l-1 1 M14 11a5 5 0 0 1 0 7l-1 1a5 5 0 0 1-7-7l1-1",
  attach: "M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.49",
  calendar: "M8 2v4 M16 2v4 M3 8h18 M5 4h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z",
};

type Props = {
  open: boolean;
  onClose: () => void;
  onSent: () => void;
  defaultFrom: string;
  replyTo?: { to: string; subject: string; body: string; thread_id?: string; sigHtml?: string };
  inline?: boolean;
};

export default function ComposeModal({ open, onClose, onSent, defaultFrom, replyTo, inline = false }: Props) {
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

  useEffect(() => {
    if (replyTo) {
      setTo(replyTo.to || "");
      setSubject(replyTo.subject || "");
      setBody(replyTo.body || "");
    }
    setFrom(defaultFrom || "hello@demo.aivory.test");
  }, [replyTo, defaultFrom, open]);

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
      const htmlSig = (replyTo as any)?.sigHtml;
      const payload: any = {
        from: from.trim(),
        to: to.split(",").map((s) => s.trim()).filter(Boolean),
        subject: subject.trim(),
        text: isHtml ? undefined : body,
        html: isHtml ? (htmlSig ? `${body}<br/><br/>${htmlSig}` : body) : undefined,
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

  const inner = (
    <div className={`flex flex-col overflow-hidden bg-white ${inline ? "h-full border-0 rounded-none" : "max-h-[92vh] w-full max-w-[640px] rounded-xl border border-zinc-200 shadow-xl"}`}>
      {/* Header — tidy + Emil: outline icons, no emoticon */}
      <div className="flex items-center justify-between border-b border-[#e8e0c8] bg-[#fefcf6] px-3 py-2">
        <div className="flex items-center gap-2">
          <button onClick={send} disabled={sending} className="inline-flex items-center gap-1.5 rounded-full border border-[#005a5e] bg-[#fefcf6] px-3 py-1.5 text-sm font-semibold text-[#005a5e] hover:bg-[#005a5e] hover:text-white disabled:opacity-50 active:scale-[0.97] transition-transform">
            <Ico d={P.send} size={12} /> {sending ? "Sending..." : "Send"}
          </button>
          <span className="h-4 w-px bg-[#e8e0c8]" />
          <button className="hidden sm:inline-flex rounded-full px-2 py-1 text-xs text-zinc-600 hover:bg-[#f8f6ef]">Send Later</button>
          <span className="text-xs text-zinc-400">|</span>
          <button onClick={insertLink} className="rounded p-1.5 text-zinc-600 hover:bg-[#f8f6ef]" title="Insert link"><Ico d={P.link} size={14} /></button>
          <button onClick={() => fileRef.current?.click()} className="rounded p-1.5 text-zinc-600 hover:bg-[#f8f6ef]" title="Attach"><Ico d={P.attach} size={14} /></button>
        </div>
        <div className="flex items-center gap-2">
          <button onClick={onClose} className="hidden sm:inline-flex text-xs text-zinc-500 hover:text-zinc-700">Save draft</button>
          <button onClick={onClose} className="rounded p-1.5 text-zinc-500 hover:bg-zinc-100">✕</button>
        </div>
      </div>

      {/* From / To / Cc / Bcc / Subject — Zoho dense rows */}
      <div className="flex-1 space-y-0 overflow-y-auto">
        <div className="flex items-center gap-2 border-b border-zinc-100 px-4 py-2.5 text-sm">
          <span className="w-14 shrink-0 text-xs font-medium text-zinc-500">From</span>
          <span className="truncate text-sm text-zinc-900">{from}</span>
          <input value={from} onChange={(e) => setFrom(e.target.value)} className="ml-auto w-64 rounded border border-zinc-200 px-2 py-1 text-xs" placeholder="change from" />
        </div>

        <div className="flex items-center gap-2 border-b border-zinc-100 px-4 py-2.5">
          <span className="w-14 shrink-0 text-xs font-medium text-zinc-500">To</span>
          <input value={to} onChange={(e) => setTo(e.target.value)} placeholder="To" className="flex-1 border-0 p-0 text-sm placeholder:text-zinc-400 focus:ring-0 focus:outline-none" />
          <button onClick={() => setShowCcBcc(!showCcBcc)} className="shrink-0 rounded px-2 py-1 text-xs text-zinc-500 hover:bg-zinc-100">Cc</button>
          <span className="text-xs text-zinc-300">|</span>
          <button onClick={() => setShowCcBcc(!showCcBcc)} className="shrink-0 rounded px-2 py-1 text-xs text-zinc-500 hover:bg-zinc-100">Bcc</button>
        </div>

        {showCcBcc && (
          <>
            <div className="flex items-center gap-2 border-b border-zinc-100 px-4 py-2.5">
              <span className="w-14 shrink-0 text-xs font-medium text-zinc-500">Cc</span>
              <input value={cc} onChange={(e) => setCc(e.target.value)} placeholder="Cc" className="flex-1 border-0 p-0 text-sm placeholder:text-zinc-400 focus:outline-none" />
            </div>
            <div className="flex items-center gap-2 border-b border-zinc-100 px-4 py-2.5">
              <span className="w-14 shrink-0 text-xs font-medium text-zinc-500">Bcc</span>
              <input value={bcc} onChange={(e) => setBcc(e.target.value)} placeholder="Bcc" className="flex-1 border-0 p-0 text-sm placeholder:text-zinc-400 focus:outline-none" />
            </div>
          </>
        )}

        <div className="flex items-center gap-2 border-b border-zinc-100 px-4 py-2.5">
          <span className="w-14 shrink-0 text-xs font-medium text-zinc-500">Subject</span>
          <input value={subject} onChange={(e) => setSubject(e.target.value)} placeholder="Subject" className="flex-1 border-0 p-0 text-sm placeholder:text-zinc-400 focus:outline-none" />
        </div>

        {/* Formatting toolbar — tidy outline, no emoticon */}
        <div className="flex flex-wrap items-center gap-1 border-b border-[#e8e0c8] bg-[#f8f6ef] px-3 py-1.5">
          <button onClick={() => fileRef.current?.click()} className="rounded p-1.5 text-zinc-600 hover:bg-[#fefcf6] hover:shadow-sm" title="Attach"><Ico d={P.attach} size={14} /></button>
          <button onClick={insertLink} className="rounded p-1.5 text-zinc-600 hover:bg-[#fefcf6]" title="Link"><Ico d={P.link} size={14} /></button>
          <a href="/calendar" target="_blank" className="rounded p-1.5 text-[#005a5e] hover:bg-[#fefcf6]" title="Aivory Calendar"><Ico d={P.calendar} size={14} cls="text-[#005a5e]" /></a>
          <a href="https://book.aivory.uk/book/aivory-call" target="_blank" className="inline-flex items-center gap-1 rounded px-1.5 py-1 text-xs text-zinc-600 hover:bg-[#fefcf6]" title="CalNode booking"><Ico d={P.link} size={12} />book</a>
          <span className="mx-1 h-4 w-px bg-[#e8e0c8]" />
          <button className="rounded px-1.5 py-1 text-sm font-bold text-zinc-700 hover:bg-[#fefcf6]">B</button>
          <button className="rounded px-1.5 py-1 text-sm italic text-zinc-700 hover:bg-[#fefcf6]">I</button>
          <button className="rounded px-1.5 py-1 text-sm underline text-zinc-700 hover:bg-[#fefcf6]">U</button>
          <button onClick={() => setIsHtml(!isHtml)} className={`ml-1 rounded-full border px-2 py-1 text-xs ${isHtml ? "border-[#005a5e] bg-[#005a5e] text-white" : "border-[#e8e0c8] bg-[#fefcf6]"}`}>{isHtml ? "HTML" : "Text"}</button>
          <span className="ml-auto text-[11px] text-zinc-400">Max 10 files · 10MB each</span>
        </div>

        <div className="min-h-[320px] p-0">
          <textarea
            value={body}
            onChange={(e) => setBody(e.target.value)}
            placeholder={isHtml ? "<p>Hello <a href='https://...'>aivory.id</a></p>" : "Write your message..."}
            className="min-h-[340px] w-full resize-none border-0 p-4 text-sm leading-6 placeholder:text-zinc-400 focus:outline-none focus:ring-0"
          />
        </div>

        {(replyTo as any)?.sigHtml && (
          <div className="rounded-lg border border-dashed border-zinc-300 bg-zinc-50 p-3">
            <div className="text-[11px] font-semibold text-zinc-600">Signature preview</div>
            <div className="prose prose-sm mt-1 max-w-none text-xs" dangerouslySetInnerHTML={{__html: (replyTo as any).sigHtml}} />
            <div className="mt-1 text-[11px] text-zinc-400">Will be appended automatically (HTML mode).</div>
          </div>
        )}
        {files.length > 0 && (
          <div className="border-t border-zinc-100 bg-zinc-50 p-3">
            <div className="space-y-1">
              {files.map((f, i) => (
                <div key={i} className="flex items-center justify-between rounded-lg border border-zinc-200 bg-white px-3 py-1.5 text-xs">
                  <span className="truncate">{f.name} · {(f.size / 1024).toFixed(1)} KB</span>
                  <button onClick={() => setFiles(files.filter((_, j) => j !== i))} className="ml-2 rounded px-1.5 py-0.5 text-zinc-500 hover:bg-zinc-50">✕</button>
                </div>
              ))}
            </div>
          </div>
        )}

        {err && <div className="mx-4 mb-3 rounded-lg bg-red-50 px-3 py-2 text-xs font-medium text-red-700 ring-1 ring-red-200">{err}</div>}
      </div>

      <input ref={fileRef} type="file" multiple hidden onChange={(e) => handleFiles(e.target.files)} />
    </div>
  );

  if (inline) return inner;

  return (
    <div className="fixed inset-0 z-50 flex items-end justify-center bg-black/30 p-4 sm:items-center">
      {inner}
    </div>
  );
}
