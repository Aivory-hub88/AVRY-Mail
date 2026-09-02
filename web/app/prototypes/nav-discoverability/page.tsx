"use client";
import { useEffect, useRef, useState } from "react";

// --- Outline icons (no emoticon) — lucide-style stroke 1.6, 16px, 3D subtle via strokeLinecap/join ---
function Ico({ d, size = 16, cls = "" }: { d: string; size?: number; cls?: string }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.65} strokeLinecap="round" strokeLinejoin="round" className={cls} aria-hidden>
      <path d={d} />
    </svg>
  );
}
const P = {
  compose: "M12 20h9 M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4 12.5-12.5z",
  settings: "M3 6h18 M3 12h18 M3 18h18 M7 6a2 2 0 1 0 0 4 2 2 0 0 0 0-4z M14 12a2 2 0 1 0 0 4 2 2 0 0 0 0-4z M9 18a2 2 0 1 0 0 4 2 2 0 0 0 0-4z",
  key: "M9 8V6a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2 M5 11h14v4a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2v-4z M12 15v7 M12 22c0 1.2-1 2-2 1.2 M12 22c0 1.2 1 2 2 1.2",
  calendar: "M8 2v4 M16 2v4 M3 8h18 M5 4h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z",
  globe: "M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20z M2 12h20 M12 2a15 15 0 0 1 0 20 M12 2a15 15 0 0 0 0 20",
  inbox: "M22 12h-6l-2 3h-4l-2-3H2 M2 7a2 2 0 0 1 2-2h5l2 2h9a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2z",
  send: "M22 2L11 13 M22 2l-7 20-4-9-9-4 20-7z",
  drafts: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z M14 2v6h6 M10 13H8 M16 17H8 M13 17H8",
  spam: "M12 2L2 7l10 5 10-5-10-5z M2 17l10 5 10-5 M2 12l10 5 10-5",
  trash: "M3 6h18 M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2 M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6 M10 11v6 M14 11v6",
  sig: "M12 20h9 M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4 12.5-12.5z M15 12a3 3 0 0 0 0 6",
  mail: "M4 4h16a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z M22 6l-10 7L2 6",
  search: "M21 21l-4.35-4.35 M11 19a8 8 0 1 1 0-16 8 8 0 0 1 0 16z",
};

// --- Mock data same as production inbox ---
const MOCK_MSGS = [
  { id: "1", from: "customer@acme.com", subject: "Invoice #4821 overdue — AED 18,500", snippet: "Hi team, Invoice #4821 for AED 18,500 was due 12 days ago...", time: "09:41", unread: true },
  { id: "2", from: "noreply@supabase.co", subject: "Your Supabase project is ready", snippet: "Your project aivory-mail has been provisioned successfully.", time: "Yesterday", unread: false },
];

// ---- Variant components (keep 3-pane structure identical to production page.tsx) ----
function TopBar({ variant }: { variant: "sidebar" | "topbar" | "hybrid" }) {
  const showTopSettings = variant === "topbar" || variant === "hybrid";
  return (
    <div className="flex h-9 shrink-0 items-center gap-2 border-b border-zinc-700 bg-zinc-800 px-3 text-xs text-zinc-300">
      <span className="flex items-center gap-1.5 rounded bg-[#fefcf6] px-2 py-1 text-xs font-semibold text-zinc-900"><Ico d={P.mail} size={12} /> Mail</span>
      <span className="text-zinc-500">·</span>
      <span className="hidden items-center gap-1 sm:flex"><Ico d={P.search} size={12} cls="text-zinc-500" /> Search</span>
      <input placeholder="Search ( / )" className="ml-2 hidden w-48 rounded-full bg-[#fefcf6] px-3 py-1 text-xs text-zinc-700 placeholder:text-zinc-400 focus:outline-none sm:block" />
      <div className="ml-auto flex items-center gap-1">
        {showTopSettings ? (
          <>
            <a href="/settings/mail" className="flex items-center gap-1.5 rounded-full bg-[#fefcf6]/10 px-2.5 py-1 text-[11px] font-medium text-white hover:bg-[#fefcf6]/15 border border-white/10">
              <Ico d={P.settings} size={12} /> Settings
            </a>
            <a href="/settings" className="hidden sm:flex items-center gap-1 rounded-full bg-[#fefcf6]/10 px-2 py-1 text-[11px] text-zinc-300 hover:bg-[#fefcf6]/15 border border-white/10">
              <Ico d={P.key} size={11} /> API & MCP
            </a>
            <span className="mx-1 h-4 w-px bg-[#fefcf6]/10" />
            <a href="/calendar" className="flex items-center gap-1 rounded-full bg-[#fefcf6] px-3 py-1 text-xs font-semibold text-zinc-900 hover:bg-zinc-100"><Ico d={P.calendar} size={12} /> Calendar</a>
          </>
        ) : (
          <span className="rounded bg-amber-400 px-2 py-1 text-xs font-semibold text-zinc-900">Composing…</span>
        )}
      </div>
      {variant === "sidebar" && <span className="ml-2 hidden rounded bg-amber-400 px-2 py-1 text-xs font-semibold text-zinc-900 sm:inline">Composing…</span>}
    </div>
  );
}

function Sidebar({ variant }: { variant: "sidebar" | "topbar" | "hybrid" }) {
  const isEnlarged = variant === "sidebar" || variant === "hybrid";
  return (
    <aside className="flex w-[280px] shrink-0 flex-col border-r border-zinc-200 bg-[#fefcf6]">
      <div className="border-b border-zinc-100 px-4 py-5">
        <h1 className="text-xl font-bold tracking-tight">Aivory Mail</h1>
        <p className="mt-1 text-xs leading-relaxed text-zinc-500">Business email, without<br /> the email tax.</p>
      </div>
      <div className="px-3 pt-3">
        <button className="flex w-full items-center justify-center gap-2 rounded-xl bg-zinc-900 px-4 py-3 text-sm font-semibold text-white shadow hover:bg-black"><Ico d={P.compose} size={14} cls="text-white" /> Compose</button>
      </div>

      {/* Primary folders — outline icons, no emoticon */}
      <nav className="flex flex-col gap-1.5 px-3 py-4">
        {[
          { label: "Inbox", icon: P.inbox },
          { label: "Sent", icon: P.send },
          { label: "Drafts", icon: P.drafts },
          { label: "Spam", icon: P.spam },
          { label: "Trash", icon: P.trash },
        ].map((f) => (
          <button
            key={f.label}
            className={`flex items-center gap-2 rounded-lg border px-3 py-2.5 text-left text-sm font-medium transition ${f.label === "Inbox" ? "border-zinc-900 bg-zinc-900 text-white shadow-sm" : "border-zinc-200 bg-[#fefcf6] text-zinc-700 hover:bg-zinc-50 hover:border-zinc-300"}`}
          >
            <Ico d={f.icon} size={15} cls={f.label === "Inbox" ? "text-white" : "text-zinc-500"} />
            <span className="flex-1">{f.label}</span>
            {f.label === "Inbox" && <span className="rounded-full bg-[#fefcf6] px-2 py-0.5 text-xs font-semibold text-zinc-900">2</span>}
          </button>
        ))}
      </nav>

      {/* SETTINGS AREA — the divergence */}
      {isEnlarged ? (
        <div className="px-3">
          <div className="my-2 h-px bg-zinc-100" />
          <div className="px-2 pb-1 text-[10px] font-semibold tracking-widest text-zinc-400 uppercase">Manage</div>
          <div className="flex flex-col gap-1.5">
            <a href="/settings/mail" className="flex items-center justify-between rounded-lg border border-zinc-900 bg-zinc-900 px-3 py-2.5 text-left text-sm font-medium text-white shadow-sm">
              <span className="flex items-center gap-2"><Ico d={P.settings} size={14} cls="text-white" /> Settings</span>
              <span className="rounded-full bg-[#fefcf6] px-1.5 py-0.5 text-[10px] font-bold text-zinc-900">10</span>
            </a>
            <a href="/settings" className="flex items-center justify-between rounded-lg border border-zinc-200 bg-[#fefcf6] px-3 py-2.5 text-left text-sm font-medium text-zinc-700 hover:bg-zinc-50">
              <span className="flex items-center gap-2"><Ico d={P.key} size={14} cls="text-zinc-500" /> API & MCP</span>
              <span className="text-[11px] text-zinc-400">→</span>
            </a>
            <a href="/calendar" className="flex items-center justify-between rounded-lg border border-zinc-200 bg-[#fefcf6] px-3 py-2.5 text-left text-sm font-medium text-zinc-700 hover:bg-zinc-50">
              <span className="flex items-center gap-2"><Ico d={P.calendar} size={14} cls="text-zinc-500" /> Calendar</span>
              <span className="text-[11px] text-zinc-400">↗</span>
            </a>
            <button className="flex items-center justify-between rounded-lg border border-zinc-200 bg-[#fefcf6] px-3 py-2.5 text-left text-sm font-medium text-zinc-700 hover:bg-zinc-50">
              <span className="flex items-center gap-2"><Ico d={P.globe} size={14} cls="text-zinc-500" /> Domains</span>
              <span className="rounded-full bg-zinc-100 px-2 py-0.5 text-[11px] font-semibold text-zinc-600">aivory.uk</span>
            </button>
          </div>
          <button className="mt-3 flex w-full items-center justify-center gap-1.5 rounded-lg border border-zinc-200 bg-[#fefcf6] px-3 py-1.5 text-xs font-medium hover:bg-zinc-50"><Ico d={P.sig} size={12} cls="text-zinc-500" /> Signature • Default</button>
        </div>
      ) : (
        <div className="px-3 py-2 space-y-1">
          <div className="mx-3 mb-2 rounded-xl border border-zinc-100 bg-zinc-50 p-3">
            <div className="text-xs font-semibold text-zinc-900">AI Triage</div>
            <div className="mt-1 text-[11px] leading-relaxed text-zinc-500">Email → Intelligence → Workflow → Action</div>
            <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-zinc-200"><div className="h-full w-2/3 rounded-full bg-zinc-900" /></div>
            <div className="mt-1.5 text-[10px] text-zinc-400">Heuristic + Cerveau gateway</div>
          </div>
          <button className="flex w-full items-center justify-center gap-1.5 rounded-lg border border-zinc-200 bg-[#fefcf6] px-3 py-1.5 text-xs font-medium hover:bg-zinc-50"><Ico d={P.sig} size={12} cls="text-zinc-500" /> Signature</button>
          <a href="/calendar" className="flex items-center justify-between rounded-lg border border-zinc-200 bg-[#fefcf6] px-3 py-1.5 text-xs hover:bg-zinc-50"><span className="flex items-center gap-1.5"><Ico d={P.calendar} size={12} cls="text-zinc-500" /> Aivory Calendar</span><span className="text-[11px] text-zinc-400">↗</span></a>
          <a href="/settings/mail" className="flex items-center justify-between rounded-lg border border-zinc-200 bg-[#fefcf6] px-3 py-1.5 text-xs hover:bg-zinc-50"><span className="flex items-center gap-1.5"><Ico d={P.settings} size={12} cls="text-zinc-500" /> Settings</span><span className="text-[11px] text-zinc-400">10 tabs →</span></a>
          <a href="/settings" className="flex items-center justify-between rounded-lg border border-zinc-200 bg-[#fefcf6] px-3 py-1.5 text-xs hover:bg-zinc-50"><span className="flex items-center gap-1.5"><Ico d={P.key} size={12} cls="text-zinc-500" /> API & MCP</span><span className="text-[11px] text-zinc-400">→</span></a>
          <button className="flex w-full items-center justify-between rounded-lg border border-zinc-200 bg-[#fefcf6] px-3 py-1.5 text-xs hover:bg-zinc-50"><span className="flex items-center gap-1.5"><Ico d={P.globe} size={12} cls="text-zinc-500" /> Domains</span><span className="text-[11px] text-zinc-400">→</span></button>
        </div>
      )}

      <div className="mt-auto border-t border-zinc-100 px-3 py-3">
        <div className="text-[11px] text-zinc-400">MAIL_MODE: vps · storage: local</div>
        <a href="http://localhost:8095/health" target="_blank" className="text-[11px] font-medium text-zinc-600 underline decoration-zinc-300 underline-offset-2 hover:text-zinc-900">API health ↗</a>
      </div>
    </aside>
  );
}

function Chrome({ variant }: { variant: "sidebar" | "topbar" | "hybrid" }) {
  return (
    <div className="flex h-[620px] overflow-hidden rounded-2xl border border-zinc-200 bg-[#fefcf6] shadow-sm">
      <Sidebar variant={variant} />
      <div className="flex min-w-0 flex-1 flex-col bg-zinc-100">
        <TopBar variant={variant} />
        <section className="flex min-w-0 flex-1">
          {/* Message list — identical across variants */}
          <div className="flex w-[340px] shrink-0 flex-col border-r border-zinc-200 bg-[#fefcf6]">
            <div className="border-b border-zinc-200 px-3 py-2">
              <input placeholder="Search messages..." className="w-full rounded-lg border border-zinc-200 bg-zinc-50 px-3 py-1.5 text-sm placeholder:text-zinc-400 focus:bg-[#fefcf6] focus:border-zinc-900 focus:outline-none" disabled />
            </div>
            <div className="flex items-center justify-between px-4 py-2 border-b border-zinc-100">
              <span className="text-sm font-semibold">Inbox — 2</span>
              <span className="rounded-full bg-zinc-900 px-2 py-0.5 text-[11px] font-semibold text-white">1 new</span>
            </div>
            <div className="flex-1 overflow-y-auto">
              {MOCK_MSGS.map((m) => (
                <div key={m.id} className={`flex w-full flex-col gap-1 border-b border-zinc-100 px-4 py-3 ${m.unread ? "bg-[#fefcf6]" : "bg-zinc-50/50"}`}>
                  <div className="flex items-center gap-2">
                    <span className={`truncate text-[13px] ${m.unread ? "font-semibold text-zinc-900" : "font-normal text-zinc-700"}`}>{m.from}</span>
                    {m.unread && <span className="h-2 w-2 shrink-0 rounded-full bg-blue-500" />}
                    <span className="ml-auto shrink-0 text-[11px] text-zinc-400">{m.time}</span>
                  </div>
                  <div className="truncate text-[13px] font-medium text-zinc-900">{m.subject}</div>
                  <div className="line-clamp-2 text-xs leading-relaxed text-zinc-500">{m.snippet}</div>
                </div>
              ))}
            </div>
          </div>
          {/* Detail / Compose — identical */}
          <div className="flex min-w-0 flex-1 flex-col bg-[#fefcf6]">
            <div className="flex items-center gap-2 border-b border-zinc-200 px-3 py-2">
              <span className="rounded border border-blue-600 bg-[#fefcf6] px-2.5 py-1 text-xs font-semibold text-blue-600">✈ Send</span>
              <span className="text-xs text-zinc-400">Send Later</span>
              <span className="ml-auto text-xs text-zinc-400">Save draft ×</span>
            </div>
            <div className="space-y-3 p-4">
              <div className="flex gap-2 text-xs"><span className="w-12 text-zinc-500">From</span><span className="rounded bg-zinc-100 px-2 py-1">hello@demo.aivory.test</span></div>
              <div className="flex gap-2 text-xs"><span className="w-12 text-zinc-500">To</span><span className="text-zinc-400">To</span><span className="ml-auto text-zinc-400">Cc  Bcc</span></div>
              <div className="flex gap-2 text-xs"><span className="w-12 text-zinc-500">Subject</span><span className="text-zinc-400">Subject</span></div>
              <div className="flex gap-1 border-y border-zinc-100 py-2 text-xs text-zinc-500"><span className="rounded border px-1.5 py-0.5">B</span><span className="rounded border px-1.5 py-0.5">I</span><span className="rounded border px-1.5 py-0.5">U</span><span className="rounded bg-zinc-100 px-2 py-0.5">Text</span></div>
              <div className="text-sm text-zinc-400">Write your message...</div>
              <div className="rounded-xl border border-zinc-200 bg-zinc-50 p-3 text-xs text-zinc-500">
                {variant === "sidebar" && "Variant Sidebar: Settings naik ke row utama di sidebar (Manage section, full-width, badge 10). Top bar tetap Composing… — paling konservatif."}
                {variant === "topbar" && "Variant Top Bar: Sidebar tetap kecil (existing), Settings pindah ke header gelap sebagai pill ⚙️ Settings + Calendar. Tidak ubah sidebar layout."}
                {variant === "hybrid" && "Variant Hybrid: Dua-duanya — sidebar enlarged + header pills. Paling discoverable, price: header lebih ramai."}
              </div>
            </div>
          </div>
        </section>
      </div>
    </div>
  );
}

export default function Page() {
  const variants = [
    { key: "sidebar", label: "Sidebar", el: <Chrome variant="sidebar" /> },
    { key: "topbar", label: "Top Bar", el: <Chrome variant="topbar" /> },
    { key: "hybrid", label: "Hybrid", el: <Chrome variant="hybrid" /> },
  ] as const;
  const [idx, setIdx] = useState(0);
  const pickerRef = useRef<HTMLElement>(null);
  const highlightRef = useRef<HTMLSpanElement>(null);

  // URL param + highlight positioning
  useEffect(() => {
    const v = parseInt(new URLSearchParams(window.location.search).get("v") || "1", 10);
    if (v >= 1 && v <= variants.length) setIdx(v - 1);
    requestAnimationFrame(() => requestAnimationFrame(() => pickerRef.current?.setAttribute("data-ready", "")));
  }, []);
  useEffect(() => {
    const p = pickerRef.current; const h = highlightRef.current;
    if (!p || !h) return;
    const items = [...p.querySelectorAll<HTMLButtonElement>(".proto-picker-item:not(.proto-picker-replay)")];
    const el = items[idx];
    if (!el) return;
    h.style.width = el.offsetWidth + "px";
    h.style.transform = `translateX(${el.offsetLeft}px)`;
    const url = new URL(window.location.href);
    url.searchParams.set("v", String(idx + 1));
    window.history.replaceState(null, "", url);
  }, [idx]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (/^(INPUT|TEXTAREA|SELECT)$/.test((e.target as HTMLElement).tagName) || (e.target as HTMLElement).isContentEditable) return;
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      const n = parseInt(e.key, 10);
      if (n >= 1 && n <= variants.length) setIdx(n - 1);
      else if (e.key === "ArrowRight") setIdx((i) => (i + 1) % variants.length);
      else if (e.key === "ArrowLeft") setIdx((i) => (i - 1 + variants.length) % variants.length);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div className="min-h-dvh bg-[#f8f6ef] p-6 font-[Manrope]">
      <div className="mx-auto max-w-[1160px]">
        <div className="mb-4">
          <h1 className="text-xl font-bold tracking-tight text-zinc-900">Nav Discoverability — keep 3-pane structure</h1>
          <p className="mt-1 text-sm text-zinc-500">Inbox — 0 conversations layout tetap. Hanya posisi Settings/Domains yang berbeda. Pilih via pill di bawah (1-3 / ←→).</p>
        </div>
        <div key={idx}>{variants[idx].el}</div>
        <div className="mt-6 grid grid-cols-1 gap-3 sm:grid-cols-3">
          <div className="rounded-xl border border-zinc-200 bg-[#fefcf6] p-3">
            <div className="text-xs font-bold">Sidebar</div>
            <div className="mt-1 text-xs text-zinc-600">Axis: sidebar hierarchy. Manage divider + Settings sebagai primary row (zinc-900, badge 10). Cocok kalau user mostly mouse di sidebar.</div>
            <div className="mt-2 text-[11px] text-zinc-400">Cost: sidebar lebih tinggi, push AI Triage ke bawah.</div>
          </div>
          <div className="rounded-xl border border-zinc-200 bg-[#fefcf6] p-3">
            <div className="text-xs font-bold">Top Bar</div>
            <div className="mt-1 text-xs text-zinc-600">Axis: header action. Sidebar untouched, Settings jadi pill di kanan header gelap (dekat Calendar). Global access dari mana pun.</div>
            <div className="mt-2 text-[11px] text-zinc-400">Cost: header lebih ramai, but muted glass.</div>
          </div>
          <div className="rounded-xl border border-zinc-200 bg-[#fefcf6] p-3 ring-1 ring-zinc-900">
            <div className="text-xs font-bold">Hybrid — both</div>
            <div className="mt-1 text-xs text-zinc-600">Axis: redundancy. Sidebar + header. Paling susah miss, konsisten dengan mailflare header actions.</div>
            <div className="mt-2 text-[11px] text-zinc-400">Cost: duplikat label, but lowest miss-rate.</div>
          </div>
        </div>
        <div className="mt-4 text-xs text-zinc-500">Routes: <code className="rounded bg-zinc-900 px-1.5 py-0.5 text-white">/settings/mail</code> 10 tabs · <code className="rounded bg-zinc-900 px-1.5 py-0.5 text-white">/settings</code> API & MCP · <code className="rounded bg-zinc-900 px-1.5 py-0.5 text-white">/calendar</code></div>
      </div>

      {/* Picker — verbatim spec */}
      <nav ref={pickerRef} className="proto-picker" aria-label="Prototype variants">
        <span ref={highlightRef} className="proto-picker-highlight" aria-hidden="true" />
        {variants.map((v, i) => (
          <button key={v.key} className="proto-picker-item" data-active={i === idx ? "" : undefined} aria-current={i === idx ? "true" : undefined} onClick={() => setIdx(i)}>
            {v.label}
          </button>
        ))}
      </nav>
      <style>{`
        .proto-picker{position:fixed;bottom:24px;left:50%;transform:translateX(-50%);z-index:2147483647;display:flex;align-items:center;gap:2px;padding:4px;border-radius:999px;background:rgba(10,10,10,.82);-webkit-backdrop-filter:blur(12px) saturate(1.4);backdrop-filter:blur(12px) saturate(1.4);box-shadow:0 0 0 1px rgba(255,255,255,.08) inset,0 8px 24px rgba(0,0,0,.24),0 2px 6px rgba(0,0,0,.12);font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;font-size:13px;line-height:1;-webkit-font-smoothing:antialiased;user-select:none;-webkit-user-select:none}
        .proto-picker-highlight{position:absolute;top:4px;left:0;height:28px;border-radius:999px;background:rgba(255,255,255,.12);will-change:transform}
        .proto-picker[data-ready] .proto-picker-highlight{transition:transform 250ms cubic-bezier(0.23,1,0.32,1),width 250ms cubic-bezier(0.23,1,0.32,1)}
        @media(prefers-reduced-motion:reduce){.proto-picker[data-ready] .proto-picker-highlight{transition:none}}
        .proto-picker-item{position:relative;display:flex;align-items:center;height:28px;padding:0 12px;border:0;border-radius:999px;background:transparent;color:rgba(255,255,255,.55);font:inherit;cursor:pointer;transition:color 150ms ease-out}
        .proto-picker-item:hover{color:rgba(255,255,255,.85)} .proto-picker-item:active{transform:scale(.97)}
        .proto-picker-item:focus-visible{outline:2px solid rgba(255,255,255,.4);outline-offset:2px} .proto-picker-item[data-active]{color:#fff}
        .proto-picker-divider{width:1px;height:16px;margin:0 4px;background:rgba(255,255,255,.12)} .proto-picker-replay{padding:0 10px;font-size:14px}
      `}</style>
    </div>
  );
}
