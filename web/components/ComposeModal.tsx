"use client";
import { useState, useRef, useEffect } from "react";
import DOMPurify from "dompurify";
const API = process.env.NEXT_PUBLIC_MAIL_API || "http://localhost:8095";
const BOOK_URL = process.env.NEXT_PUBLIC_BOOK_URL || "https://book.aivory.uk/book/aivory-call";
function Ico({ d, size = 14, cls = "" }: { d: string; size?: number; cls?: string }) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.6} vectorEffect="non-scaling-stroke" shapeRendering="geometricPrecision" strokeLinecap="round" strokeLinejoin="round" className={cls} aria-hidden><path d={d} /></svg>;
}
const P = {
  send: "M22 2L11 13 M22 2l-7 20-4-9-9-4 20-7z",
  link: "M10 13a5 5 0 0 1 0-7l1-1a5 5 0 0 1 7 7l-1 1 M14 11a5 5 0 0 1 0 7l-1 1a5 5 0 0 1-7-7l1-1",
  attach: "M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.49",
  calendar: "M8 2v4 M16 2v4 M3 8h18 M5 4h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z",
  // Distinct from `link` — CalNode's "book" button was using the same chain
  // icon as Insert Link, right next to it, and looked like a stray duplicate.
  extLink: "M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6 M15 3h6v6 M10 14 21 3",
  trash: "M3 6h18 M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2 M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6",
  more: "M12 6a1 1 0 1 0 0-2 1 1 0 0 0 0 2z M12 13a1 1 0 1 0 0-2 1 1 0 0 0 0 2z M12 20a1 1 0 1 0 0-2 1 1 0 0 0 0 2z",
  image: "M3 5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5z M8.5 10a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3z M21 15l-5-5L5 21",
  smile: "M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20z M8 14s1.5 2 4 2 4-2 4-2 M9 9h.01 M15 9h.01",
  strike: "M5 12h14 M16 6.5c-.5-1-2-2.5-4.5-2.5S7 5.3 7 7.2c0 4 9 2.6 9 6.8 0 2-2 3.7-4.7 3.7S6.5 16.2 6 15",
};
const FONTS = [
  { label: "Sans-serif", value: "-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,Helvetica,Arial,sans-serif" },
  { label: "Serif", value: "Georgia,'Times New Roman',serif" },
  { label: "Monospace", value: "'SF Mono',Menlo,Consolas,monospace" },
  { label: "Comic", value: "'Comic Sans MS',cursive" },
];
const FONT_SIZES = [
  { label: "Small", value: "12px" },
  { label: "Normal", value: "14px" },
  { label: "Large", value: "18px" },
  { label: "Huge", value: "24px" },
];
const EMOJIS = ["😀","😂","🙂","😉","😍","👍","🙏","🎉","✅","❤️","🔥","👋","😊","🤔","👏","🚀"];

type Props = {
  open: boolean;
  onClose: () => void;
  onSent: () => void;
  defaultFrom: string;
  replyTo?: { to: string; subject: string; body: string; thread_id?: string; sigHtml?: string };
  inline?: boolean;
  undoSendSeconds?: number;
  mailboxId?: string;
};

export default function ComposeModal({ open, onClose, onSent, defaultFrom, replyTo, inline = false, undoSendSeconds = 10, mailboxId }: Props) {
  const [from, setFrom] = useState(defaultFrom || "");
  const [sendAsOptions, setSendAsOptions] = useState<{ email: string; label: string }[]>([]);

  useEffect(() => {
    if (!mailboxId) { setSendAsOptions([]); return; }
    fetch(`${API}/v1/send-as?mailbox_id=${mailboxId}`).then(r=>r.json()).then(j=> {
      const aliases = (j.data || []).map((a: any) => ({ email: a.alias_email, label: a.display_name ? `${a.display_name} <${a.alias_email}>` : a.alias_email }));
      setSendAsOptions(aliases);
    }).catch(()=> setSendAsOptions([]));
  }, [mailboxId]);
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
  const [pending, setPending] = useState<{ secondsLeft: number } | null>(null);
  const pendingTimer = useRef<ReturnType<typeof setInterval> | null>(null);
  const pendingPayload = useRef<any>(null);
  const [showSchedule, setShowSchedule] = useState(false);
  const [showEmoji, setShowEmoji] = useState(false);
  const bodyRef = useRef<HTMLTextAreaElement>(null);
  const imageRef = useRef<HTMLInputElement>(null);

  function wrapSelection(before: string, after: string) {
    const el = bodyRef.current;
    if (!el) { setBody(b => b + before + "text" + after); return; }
    const start = el.selectionStart ?? body.length;
    const end = el.selectionEnd ?? body.length;
    const sel = body.slice(start, end) || "text";
    const next = body.slice(0, start) + before + sel + after + body.slice(end);
    setBody(next);
    setTimeout(()=> { el.focus(); el.setSelectionRange(start + before.length, start + before.length + sel.length); }, 0);
    if (!isHtml && (before.includes("<") )) setIsHtml(true);
  }
  function insertAtCursor(text: string) {
    const el = bodyRef.current;
    if (!el) { setBody(b => b + text); return; }
    const start = el.selectionStart ?? body.length;
    const end = el.selectionEnd ?? body.length;
    const next = body.slice(0, start) + text + body.slice(end);
    setBody(next);
    setTimeout(()=> { el.focus(); el.setSelectionRange(start + text.length, start + text.length); }, 0);
  }
  function applyFont(prop: "font-family" | "font-size", value: string) {
    wrapSelection(`<span style="${prop}:${value}">`, "</span>");
  }
  async function insertImage(list: FileList | null) {
    if (!list || !list[0]) return;
    const f = list[0];
    if (!f.type.startsWith("image/")) { setErr("Please choose an image file"); return; }
    if (f.size > 5 * 1024 * 1024) { setErr("Inline images are limited to 5MB"); return; }
    const dataUrl = await new Promise<string>((res, rej) => {
      const r = new FileReader();
      r.onload = () => res(r.result as string);
      r.onerror = rej;
      r.readAsDataURL(f);
    });
    setIsHtml(true);
    insertAtCursor(`<img src="${dataUrl}" alt="${f.name}" style="max-width:100%">`);
  }
  function scheduleAt(d: Date) {
    const delay = d.getTime() - Date.now();
    if (delay <= 0) { setErr("Schedule time must be in the future"); return; }
    setErr("");
    // prepare payload like send() but without validation delay
    if (!to.trim()) { setErr("To required"); return; }
    if (!subject.trim()) { setErr("Subject required"); return; }
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
    pendingPayload.current = payload;
    const sec = Math.ceil(delay/1000);
    setPending({ secondsLeft: sec });
    if (pendingTimer.current) clearInterval(pendingTimer.current);
    pendingTimer.current = setInterval(() => {
      setPending((p) => {
        if (!p) return p;
        if (p.secondsLeft <= 1) { if (pendingTimer.current) clearInterval(pendingTimer.current); actuallySend(); return null; }
        return { secondsLeft: p.secondsLeft - 1 };
      });
    }, 1000);
  }

  useEffect(() => {
    return () => { if (pendingTimer.current) clearInterval(pendingTimer.current); };
  }, []);

  function cancelPending() {
    if (pendingTimer.current) clearInterval(pendingTimer.current);
    pendingTimer.current = null;
    pendingPayload.current = null;
    setPending(null);
  }

  async function actuallySend() {
    setSending(true);
    try {
      const r = await fetch(`${API}/v1/send`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(pendingPayload.current),
      });
      const j = await r.json();
      if (!j.success) throw new Error(j.error || "Send failed");
      onSent();
      setTo(""); setSubject(replyTo?.subject || ""); setBody(""); setFiles([]); setCc(""); setBcc("");
    } catch (e: any) {
      setErr(e.message || String(e));
    } finally {
      setSending(false);
      pendingPayload.current = null;
      setPending(null);
    }
  }

  useEffect(() => {
    if (replyTo) {
      setTo(replyTo.to || "");
      setSubject(replyTo.subject || "");
      setBody(replyTo.body || "");
    }
    if (defaultFrom) setFrom(defaultFrom);
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

  function send() {
    setErr("");
    if (!to.trim()) { setErr("To required"); return; }
    if (!subject.trim()) { setErr("Subject required"); return; }
    if (!body.trim()) { setErr("Body required"); return; }

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
    pendingPayload.current = payload;

    if (undoSendSeconds <= 0) { actuallySend(); return; }

    setPending({ secondsLeft: undoSendSeconds });
    pendingTimer.current = setInterval(() => {
      setPending((p) => {
        if (!p) return p;
        if (p.secondsLeft <= 1) {
          if (pendingTimer.current) clearInterval(pendingTimer.current);
          actuallySend();
          return null;
        }
        return { secondsLeft: p.secondsLeft - 1 };
      });
    }, 1000);
  }

  if (!open) return null;

  if (pending) {
    const pct = Math.round(((undoSendSeconds - pending.secondsLeft) / undoSendSeconds) * 100);
    const banner = <SendingBanner secondsLeft={pending.secondsLeft} pct={pct} onUndo={cancelPending} />;
    return inline ? (
      <div className="flex h-full items-center justify-center bg-white">{banner}</div>
    ) : (
      <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30 p-4">{banner}</div>
    );
  }

  const inner = (
    <div className={`flex h-full flex-col overflow-hidden bg-white ${inline ? "border border-[#e8e0c8]" : "max-h-[92vh] w-full max-w-[640px] rounded-xl border border-zinc-200 shadow-xl"}`}>
      {/* Header — tidy + Emil: outline icons, no emoticon */}
      <div className="flex items-center justify-between border-b border-[#e8e0c8] bg-[#fefcf6] px-3 py-2">
        <div className="flex items-center gap-2">
          <button onClick={send} disabled={sending} className="inline-flex items-center gap-1.5 rounded-lg border border-[#005a5e] bg-[#fefcf6] px-3 py-1.5 text-sm font-semibold text-[#005a5e] hover:bg-[#005a5e] hover:text-white disabled:opacity-50 active:scale-[0.97] transition-transform">
            <Ico d={P.send} size={12} /> {sending ? "Sending..." : "Send"}
          </button>
          <span className="h-4 w-px bg-[#e8e0c8]" />
          <div className="relative hidden sm:inline-flex">
            <button onClick={()=> setShowSchedule(!showSchedule)} className="rounded-lg px-2 py-1 text-xs text-zinc-600 hover:bg-[#f8f6ef]">Send Later ▾</button>
            {showSchedule && (
              <div className="absolute left-0 top-full z-20 mt-1 w-48 rounded-xl border border-[#e8e0c8] bg-[#fefcf6] p-1 shadow-lg">
                <button onClick={()=>{ setShowSchedule(false); const d=new Date(Date.now()+ 60*60*1000); scheduleAt(d); }} className="w-full rounded-lg px-3 py-1.5 text-left text-xs hover:bg-[#f8f6ef]">In 1 hour</button>
                <button onClick={()=>{ setShowSchedule(false); const d=new Date(); d.setDate(d.getDate()+1); d.setHours(9,0,0,0); scheduleAt(d); }} className="w-full rounded-lg px-3 py-1.5 text-left text-xs hover:bg-[#f8f6ef]">Tomorrow 9am</button>
                <button onClick={()=>{ setShowSchedule(false); const d=new Date(); d.setDate(d.getDate()+(1+7-d.getDay())%7); d.setHours(9,0,0,0); scheduleAt(d); }} className="w-full rounded-lg px-3 py-1.5 text-left text-xs hover:bg-[#f8f6ef]">Monday 9am</button>
                <button onClick={()=>{ const v=prompt("Schedule at (YYYY-MM-DD HH:mm)", new Date(Date.now()+86400000).toISOString().slice(0,16).replace("T"," ")); if(!v) return; const d=new Date(v); if(isNaN(d.getTime())){ setErr("Invalid date"); return;} setShowSchedule(false); scheduleAt(d); }} className="w-full rounded-lg px-3 py-1.5 text-left text-xs hover:bg-[#f8f6ef]">Pick date & time…</button>
              </div>
            )}
          </div>
        </div>
        <div className="flex items-center gap-1">
          <button onClick={onClose} className="hidden sm:inline-flex rounded-lg px-2 py-1 text-xs text-zinc-500 hover:bg-[#f8f6ef] hover:text-zinc-700">Save draft</button>
          <span className="h-4 w-px bg-[#e8e0c8]" />
          <button onClick={() => { setTo(""); setSubject(""); setBody(""); setFiles([]); onClose(); }} className="rounded p-1.5 text-zinc-500 hover:bg-[#f8f6ef]" title="Discard draft"><Ico d={P.trash} size={14} /></button>
          <button onClick={onClose} className="rounded p-1.5 text-zinc-500 hover:bg-zinc-100" title="Close">✕</button>
        </div>
      </div>

      {/* From / To / Cc / Bcc / Subject — Zoho dense rows */}
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto">
        <div className="flex items-center gap-2 border-b border-zinc-100 px-4 py-2.5 text-sm">
          <span className="w-14 shrink-0 text-xs font-medium text-zinc-500">From</span>
          {sendAsOptions.length > 0 ? (
            <select value={from} onChange={(e) => setFrom(e.target.value)} className="ml-auto max-w-[70%] rounded border border-[#e8e0c8] bg-[#fefcf6] px-2 py-1 text-xs text-zinc-900">
              <option value={defaultFrom}>{defaultFrom}</option>
              {sendAsOptions.map((o) => <option key={o.email} value={o.email}>{o.label}</option>)}
            </select>
          ) : (
            <>
              <span className="truncate text-sm text-zinc-900">{from}</span>
              <input value={from} onChange={(e) => setFrom(e.target.value)} className="ml-auto w-64 rounded border border-zinc-200 px-2 py-1 text-xs" placeholder="change from" />
            </>
          )}
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
        <div className="relative flex flex-wrap items-center gap-1 border-b border-[#e8e0c8] bg-[#f8f6ef] px-3 py-1.5">
          <select
            onChange={(e) => { if (e.target.value) applyFont("font-family", e.target.value); e.target.selectedIndex = 0; }}
            title="Font"
            defaultValue=""
            className="rounded border-0 bg-transparent py-1 pl-1 pr-5 text-xs text-zinc-600 hover:bg-[#fefcf6] focus:outline-none"
          >
            <option value="" disabled>Aa</option>
            {FONTS.map((f) => <option key={f.value} value={f.value} style={{ fontFamily: f.value }}>{f.label}</option>)}
          </select>
          <select
            onChange={(e) => { if (e.target.value) applyFont("font-size", e.target.value); e.target.selectedIndex = 0; }}
            title="Font size"
            defaultValue=""
            className="rounded border-0 bg-transparent py-1 pl-1 pr-5 text-xs text-zinc-600 hover:bg-[#fefcf6] focus:outline-none"
          >
            <option value="" disabled>Size</option>
            {FONT_SIZES.map((f) => <option key={f.value} value={f.value}>{f.label}</option>)}
          </select>
          <span className="mx-1 h-4 w-px bg-[#e8e0c8]" />
          <button onClick={() => fileRef.current?.click()} className="rounded p-1.5 text-zinc-600 hover:bg-[#fefcf6] hover:shadow-sm" title="Attach"><Ico d={P.attach} size={14} /></button>
          <button onClick={insertLink} className="rounded p-1.5 text-zinc-600 hover:bg-[#fefcf6]" title="Link"><Ico d={P.link} size={14} /></button>
          <button onClick={() => imageRef.current?.click()} className="rounded p-1.5 text-zinc-600 hover:bg-[#fefcf6]" title="Insert photo"><Ico d={P.image} size={14} /></button>
          <div className="relative">
            <button onClick={() => setShowEmoji(!showEmoji)} className="rounded p-1.5 text-zinc-600 hover:bg-[#fefcf6]" title="Insert emoji"><Ico d={P.smile} size={14} /></button>
            {showEmoji && (
              <div className="absolute left-0 top-full z-20 mt-1 grid w-52 grid-cols-8 gap-0.5 rounded-xl border border-[#e8e0c8] bg-[#fefcf6] p-2 shadow-lg">
                {EMOJIS.map((e) => (
                  <button key={e} onClick={() => { insertAtCursor(e); setShowEmoji(false); }} className="rounded p-1 text-base hover:bg-[#f0ece0]">{e}</button>
                ))}
              </div>
            )}
          </div>
          <a href="/calendar" target="_blank" className="rounded p-1.5 text-[#005a5e] hover:bg-[#fefcf6]" title="Aivory Calendar"><Ico d={P.calendar} size={14} cls="text-[#005a5e]" /></a>
          <a href={BOOK_URL} target="_blank" className="inline-flex items-center gap-1 rounded px-1.5 py-1 text-xs text-zinc-600 hover:bg-[#fefcf6]" title="CalNode booking"><Ico d={P.extLink} size={12} />book</a>
          <span className="mx-1 h-4 w-px bg-[#e8e0c8]" />
          <button onClick={()=> wrapSelection(isHtml ? "<b>" : "**", isHtml ? "</b>" : "**")} className="rounded px-1.5 py-1 text-sm font-bold text-zinc-700 hover:bg-[#fefcf6]">B</button>
          <button onClick={()=> wrapSelection(isHtml ? "<i>" : "*", isHtml ? "</i>" : "*")} className="rounded px-1.5 py-1 text-sm italic text-zinc-700 hover:bg-[#fefcf6]">I</button>
          <button onClick={()=> wrapSelection(isHtml ? "<u>" : "__", isHtml ? "</u>" : "__")} className="rounded px-1.5 py-1 text-sm underline text-zinc-700 hover:bg-[#fefcf6]">U</button>
          <button onClick={()=> wrapSelection(isHtml ? "<s>" : "~~", isHtml ? "</s>" : "~~")} className="rounded p-1.5 text-zinc-700 hover:bg-[#fefcf6]" title="Strikethrough"><Ico d={P.strike} size={14} /></button>
          <button onClick={() => setIsHtml(!isHtml)} className={`ml-1 rounded-lg border px-2 py-1 text-xs ${isHtml ? "border-[#005a5e] bg-[#005a5e] text-white" : "border-[#e8e0c8] bg-[#fefcf6]"}`}>{isHtml ? "HTML" : "Text"}</button>
          <span className="ml-auto text-xs text-zinc-400">Max 10 files · 10MB each</span>
          <input ref={imageRef} type="file" accept="image/*" hidden onChange={(e) => insertImage(e.target.files)} />
        </div>

        <div className="min-h-[180px] flex-1">
          <textarea
            ref={bodyRef}
            value={body}
            onChange={(e) => setBody(e.target.value)}
            placeholder={isHtml ? "<p>Hello <a href='https://...'>aivory.id</a></p>" : "Write your message..."}
            className="h-full w-full resize-none border-0 p-4 text-sm leading-6 placeholder:text-zinc-400 focus:outline-none focus:ring-0"
          />
        </div>

        {(replyTo as any)?.sigHtml && (
          <div className="rounded-lg border border-dashed border-zinc-300 bg-zinc-50 p-3">
            <div className="text-xs font-semibold text-zinc-600">Signature preview</div>
            <div className="prose prose-sm mt-1 max-w-none text-xs" dangerouslySetInnerHTML={{__html: DOMPurify.sanitize((replyTo as any).sigHtml)}} />
            <div className="mt-1 text-xs text-zinc-400">Will be appended automatically (HTML mode).</div>
          </div>
        )}
        {files.length > 0 && (
          <div className="border-t border-zinc-100 bg-zinc-50 p-3">
            <div className="space-y-1">
              {files.map((f, i) => (
                <div key={i} className="flex items-center justify-between rounded-lg border border-[#e8e0c8] bg-[#fefcf6] px-3 py-1.5 text-xs">
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

function SendingBanner({ secondsLeft, pct, onUndo }: { secondsLeft: number; pct: number; onUndo: () => void }) {
  const [mounted, setMounted] = useState(false);
  useEffect(() => { const t = requestAnimationFrame(() => setMounted(true)); return () => cancelAnimationFrame(t); }, []);
  return (
    <div
      className="w-full max-w-sm rounded-xl border border-[#e8e0c8] bg-[#fefcf6] p-5 shadow-lg transition-[transform,opacity] duration-200 ease-[cubic-bezier(0.23,1,0.32,1)]"
      style={{ opacity: mounted ? 1 : 0, transform: mounted ? "translateY(0)" : "translateY(6px) scale(0.98)" }}
    >
      <div className="flex items-center gap-2">
        <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-zinc-900 text-white">
          <svg className="h-4 w-4" fill="none" stroke="currentColor" strokeWidth="1.5" viewBox="0 0 24 24"><path strokeLinecap="round" strokeLinejoin="round" d="M6 12 3.269 3.126A59.77 59.77 0 0 1 21.485 12 59.77 59.77 0 0 1 3.27 20.876L5.999 12Zm0 0h7.5"/></svg>
        </span>
        <div className="min-w-0">
          <div className="text-sm font-semibold text-zinc-900">Sending in {secondsLeft}s</div>
          <div className="text-xs text-zinc-500">You can still undo this.</div>
        </div>
        <button
          onClick={onUndo}
          className="ml-auto shrink-0 rounded-lg border border-[#e8e0c8] bg-[#fefcf6] px-3 py-1.5 text-xs font-semibold text-zinc-900 transition-transform duration-150 hover:bg-zinc-50 active:scale-[0.96]"
        >
          Undo
        </button>
      </div>
      <div className="mt-3 h-1 overflow-hidden rounded-lg bg-zinc-100">
        <div className="h-full rounded-lg bg-zinc-900 transition-[width] duration-1000 ease-linear" style={{ width: `${pct}%` }} />
      </div>
    </div>
  );
}
