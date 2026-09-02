"use client";
import { useEffect, useState } from "react";
import ComposeModal from "../components/ComposeModal";

const API = process.env.NEXT_PUBLIC_MAIL_API || "http://localhost:8095";

// Outline icons — no emoticon (hybrid rule)
function Ico({ d, size = 16, cls = "" }: { d: string; size?: number; cls?: string }) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.65} strokeLinecap="round" strokeLinejoin="round" className={cls} aria-hidden><path d={d} /></svg>;
}
function Chip({ ok, label }: { ok: boolean; label: string }) {
  return <span className={`inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-semibold ring-1 ${ok ? "bg-emerald-50 text-emerald-700 ring-emerald-200" : "bg-amber-50 text-amber-700 ring-amber-200"}`}>{ok ? <Ico d={P.check} size={10} /> : <Ico d={P.alert} size={10} />}{label}</span>;
}
const P = {
  compose: "M12 20h9 M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4 12.5-12.5z",
  settings: "M3 6h18 M3 12h18 M3 18h18 M7 6a2 2 0 1 0 0 4 2 2 0 0 0 0-4z M14 12a2 2 0 1 0 0 4 2 2 0 0 0 0-4z M9 18a2 2 0 1 0 0 4 2 2 0 0 0 0-4z",
  key: "M9 8V6a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2 M5 11h14v4a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2v-4z M12 15v7 M12 22c0 1.2-1 2-2 1.2 M12 22c0 1.2 1 2 2 1.2",
  check: "M5 13l4 4L19 7",
  alert: "M12 9v4 M12 17h.01 M10.3 3.3L3.3 18a2 2 0 0 0 1.7 3h13a2 2 0 0 0 1.7-3L13.7 3.3a2 2 0 0 0-3.4 0z",
  copy: "M8 5H6a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2v-1 M8 5a2 2 0 0 0 2 2h2a2 2 0 0 0 2-2M8 5a2 2 0 0 1 2-2h2a2 2 0 0 1 2 2",
  calendar: "M8 2v4 M16 2v4 M3 8h18 M5 4h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z",
  globe: "M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20z M2 12h20 M12 2a15 15 0 0 1 0 20 M12 2a15 15 0 0 0 0 20",
  inbox: "M22 12h-6l-2 3h-4l-2-3H2 M2 7a2 2 0 0 1 2-2h5l2 2h9a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2z",
  send: "M22 2L11 13 M22 2l-7 20-4-9-9-4 20-7z",
  drafts: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z M14 2v6h6 M10 13H8 M16 17H8 M13 17H8",
  spam: "M12 2L2 7l10 5 10-5-10-5z M2 17l10 5 10-5 M2 12l10 5 10-5",
  trash: "M3 6h18 M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2 M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6 M10 11v6 M14 11v6",
  snoozed: "M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20z M12 6v6l4 2",
  sig: "M12 20h9 M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4 12.5-12.5z",
  mail: "M4 4h16a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z M22 6l-10 7L2 6",
  search: "M21 21l-4.35-4.35 M11 19a8 8 0 1 1 0-16 8 8 0 0 1 0 16z",
  reply: "M9 14L4 9l5-5 M4 9h10a4 4 0 0 1 4 4v7",
  forward: "M15 14l5-5-5-5 M20 9H8a4 4 0 0 0-4 4v7",
  archive: "M21 8v13a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V8 M1 3h22v5H1z M10 12h4",
  star: "M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z",
  link: "M10 13a5 5 0 0 1 0-7l1-1a5 5 0 0 1 7 7l-1 1 M14 11a5 5 0 0 1 0 7l-1 1a5 5 0 0 1-7-7l1-1",
  block: "M18 6L6 18 M6 6l12 12 M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20z",
};
type Msg = { id: string; from: string; subject: string; snippet: string; created_at: string; is_read: boolean; is_starred?: boolean };

export default function InboxPage() {
  const [msgs, setMsgs] = useState<Msg[]>([]);
  const [selected, setSelected] = useState<any>(null);
  const [activeFolder, setActiveFolder] = useState("Inbox");
  const [composeOpen, setComposeOpen] = useState(false);
  const [replyInfo, setReplyInfo] = useState<any>(null);
  const [search, setSearch] = useState("");
  const [mailboxes, setMailboxes] = useState<any[]>([]);
  const [defaultFrom, setDefaultFrom] = useState("hello@demo.aivory.test");
  const [shareUrl, setShareUrl] = useState("");
  const [signatures, setSignatures] = useState<any[]>([]);
  const [activeSig, setActiveSig] = useState<any>(null);
  const [showSigModal, setShowSigModal] = useState(false);
  const [sigHtml, setSigHtml] = useState("");
  const [calStatus, setCalStatus] = useState<any>(null);
  const [crawl, setCrawl] = useState<any>(null);
  const [tabs, setTabs] = useState<{id:string,label:string}[]>([{id:"mail",label:"Mail"}]);
  const [activeTab, setActiveTab] = useState("mail");
  const [domainHost, setDomainHost] = useState("aivory.uk");
  const [domainMode, setDomainMode] = useState<"auto"|"manual">("auto");
  const [showSnooze, setShowSnooze] = useState(false);
  const [intel, setIntel] = useState<any>(null);
  const [intelLoading, setIntelLoading] = useState(false);
  function openEmbeddedTab(id:string,label:string){
    setTabs(prev=> prev.find(t=>t.id===id) ? prev : [...prev, {id,label}]);
    setActiveTab(id);
  }

  useEffect(() => {
    const q = search ? `&search=${encodeURIComponent(search)}` : "";
    fetch(`${API}/v1/messages?folder=${activeFolder}&per_page=20${q}`)
      .then((r) => r.json())
      .then((j) => setMsgs(j.data || []))
      .catch(() => {});
  }, [activeFolder, selected, search]);

  useEffect(() => {
    fetch(`${API}/v1/mailboxes`).then(r=>r.json()).then(j=>{
      const list = j.data || [];
      setMailboxes(list);
      if (list[0]?.address) setDefaultFrom(list[0].address);
    }).catch(()=>{});
    fetch(`${API}/v1/calendar/status`).then(r=>r.json()).then(j=> setCalStatus(j.data || j)).catch(()=>{});
  }, []);
  useEffect(() => {
    const mb = mailboxes.find((m:any)=> m.address===defaultFrom);
    if (!mb) return;
    fetch(`${API}/v1/signatures?mailbox_id=${mb.id}`).then(r=>r.json()).then(j=>{
      const list = j.data || [];
      setSignatures(list);
      const def = list.find((s:any)=> s.is_default) || list[0];
      setActiveSig(def || null);
    }).catch(()=>{});
  }, [defaultFrom, mailboxes]);

  async function toggleStar(id: string) {
    await fetch(`${API}/v1/messages/${id}/star`, { method: "POST" });
    setMsgs(msgs.map(m=> m.id===id ? {...m, is_starred: !m.is_starred} as any : m));
    if (selected?.id===id) setSelected({...selected, is_starred: !selected.is_starred});
  }
  async function doShare(id: string) {
    const r = await fetch(`${API}/v1/messages/${id}/share`, { method: "POST" });
    const j = await r.json();
    if (j.success) { setShareUrl(j.data.url); navigator.clipboard?.writeText(j.data.url); }
  }
  async function doSnooze(id: string, hours: number) {
    const dt = new Date(Date.now() + hours * 3600 * 1000);
    const r = await fetch(`${API}/v1/messages/${id}/snooze`, { method: "POST", headers: {"content-type":"application/json"}, body: JSON.stringify({snoozed_until: dt.toISOString()}) });
    if (r.ok) { setMsgs(prev=> prev.filter(m=> m.id!==id)); setSelected(null); }
  }
  async function doUnsnooze(id: string) {
    await fetch(`${API}/v1/messages/${id}/snooze`, { method: "DELETE" });
    setSelected(null);
  }
  async function doBlock(email: string) {
    await fetch(`${API}/v1/contacts/block`, { method: "POST", headers: {"content-type":"application/json"}, body: JSON.stringify({email}) });
    if (selected) { await fetch(`${API}/v1/messages/${selected.id}/move`, { method: "POST", headers: {"content-type":"application/json"}, body: JSON.stringify({folder:"Spam"})}); setSelected(null); }
    setMsgs(prev=> prev.filter(m=> m.from!==email));
  }
  function openCompose(reply?: any) {
    const sigText = activeSig?.text?.trim() ? activeSig.text : (activeSig?.html ? activeSig.html.replace(/<br\s*\/?>/gi, "\n").replace(/<[^>]+>/g, "").replace(/\n{3,}/g, "\n\n").trim() : "");
    const sig = sigText ? `\n\n${sigText}` : "";
    const info = reply ? { to: reply.from, subject: reply.subject?.startsWith("Re:") ? reply.subject : `Re: ${reply.subject||""}`, body: (reply.body_text ? `\n\nOn ${reply.created_at}, ${reply.from} wrote:\n${reply.body_text}` : "") + sig, thread_id: reply.thread_id || selected?.thread_id, sigHtml: activeSig?.html } : (activeSig ? { to: "", subject: "", body: sig, thread_id: undefined, sigHtml: activeSig?.html } : null);
    setReplyInfo(info);
    setComposeOpen(true);
    // clear selection highlight when composing new, keep inbox list visible
    // selected stays so user can reference, but detail now shows compose
  }
  function closeTab(id:string) {
    setTabs(prev => prev.filter(t=>t.id!==id));
    if (activeTab===id) setActiveTab("mail");
    if (id.startsWith("compose-")) setComposeOpen(false);
  }
  async function open(id: string) {
    const r = await fetch(`${API}/v1/messages/${id}`);
    const j = await r.json();
    let data = j.data;
    // try fetch attachments meta via listing attachments endpoint fallback: we store via messages detail? add fetch
    // For MVP, parse has_attachments and try to list via separate call if needed
    try {
      const ar = await fetch(`${API}/v1/messages/${id}`);
      const aj = await ar.json();
      data = aj.data;
    } catch {}
    // attachments are stored; backend messages/:id should include attachments array if has_attachments
    // If not, try to fetch via dedicated endpoint (we add fallback: list attachments via DB query exposed as part of message)
    if (data?.has_attachments && !data.attachments) {
      // fetch attachments via hidden endpoint: we repurpose download list via querying? fallback keep empty
    }
    setSelected(data);
    setShareUrl("");
    setIntel(null); setIntelLoading(true);
    fetch(`${API}/v1/intelligence/analyze`, {method:"POST", headers:{"content-type":"application/json"}, body: JSON.stringify({subject: data.subject || "", body: data.body_text || data.snippet || ""})})
      .then(r=>r.json()).then(j=> setIntel(j.data || j)).catch(()=> setIntel(null)).finally(()=> setIntelLoading(false));
    const tid = (data as any)?.thread_id;
    if (tid) {
      fetch(`${API}/v1/threads/${tid}/crawl`).then(r=>r.json()).then(j=> setCrawl(j.data?.crawl || null)).catch(()=> setCrawl(null));
    } else setCrawl(null);
  }

  return (
    <div className="flex h-screen bg-[#f8f6ef] text-[#202124]">
      {/* Sidebar — Mailflare light, blue-accented */}
      <aside className="flex w-[280px] shrink-0 flex-col border-r border-[#e8e0c8] bg-[#fefcf6]">
        <div className="border-b border-[#f0ece0] px-4 py-4">
          <img src="/aivory-mail-logo-black.svg" alt="Aivory Mail" className="ml-4 w-[190px] h-auto max-w-full object-contain object-left" />
        </div>

        <div className="px-3 pt-3">
          <button onClick={()=>openCompose()} className="flex w-full items-center justify-center gap-2 rounded-full bg-[#005a5e] px-4 py-3 text-sm font-semibold text-white shadow hover:bg-[#00454a]"><Ico d={P.compose} size={14} cls="text-white" /> Compose</button>
        </div>
        <nav className="flex flex-col gap-1.5 px-3 py-4">
          {[
            { label: "Inbox", icon: P.inbox },
            { label: "Sent", icon: P.send },
            { label: "Drafts", icon: P.drafts },
            { label: "Snoozed", icon: P.snoozed },
            { label: "Archive", icon: P.archive },
            { label: "Spam", icon: P.spam },
            { label: "Trash", icon: P.trash },
          ].map((f) => (
            <button
              key={f.label}
              onClick={() => setActiveFolder(f.label)}
              className={`flex items-center gap-2 rounded-full border px-3 py-2.5 text-left text-sm font-medium transition ${
                f.label === activeFolder
                  ? "border-[#005a5e] bg-[#005a5e] text-white shadow-sm"
                  : "border-[#e8e0c8] bg-[#fefcf6] text-zinc-700 hover:bg-[#f5efe6] hover:border-[#005a5e]/30"
              }`}
            >
              <Ico d={f.icon} size={15} cls={f.label === activeFolder ? "text-white" : "text-zinc-500"} />
              <span className="flex-1">{f.label}</span>
              {f.label === "Inbox" && msgs.length > 0 && (
                <span className={`rounded-full px-2 py-0.5 text-xs font-semibold ${activeFolder === "Inbox" ? "bg-[#fefcf6] text-[#005a5e]" : "bg-[#005a5e] text-white"}`}>{msgs.length}</span>
              )}
            </button>
          ))}
        </nav>
        {/* Hybrid — Manage section — Zoho-like: open as tab in second+third panel */}
        <div className="px-3">
          <div className="my-2 h-px bg-[#f0ece0]" />
          <div className="px-2 pb-1 text-[10px] font-semibold tracking-widest text-zinc-400 uppercase">Manage</div>
          <div className="flex flex-col gap-1.5">
            <button onClick={()=>openEmbeddedTab("settings-mail","Settings")} className="flex items-center justify-between rounded-full border border-[#005a5e] bg-[#005a5e] px-3 py-2.5 text-left text-sm font-medium text-white shadow-sm">
              <span className="flex items-center gap-2"><Ico d={P.settings} size={14} cls="text-white" /> Settings</span>
              <span className="rounded-full bg-[#fefcf6] px-1.5 py-0.5 text-[10px] font-bold text-[#005a5e]">10</span>
            </button>
            <button onClick={()=>openEmbeddedTab("api-mcp","API & MCP")} className="flex items-center justify-between rounded-full border border-[#e8e0c8] bg-[#fefcf6] px-3 py-2.5 text-left text-sm font-medium text-zinc-700 hover:bg-[#f5efe6]">
              <span className="flex items-center gap-2"><Ico d={P.key} size={14} cls="text-zinc-500" /> API & MCP</span>
              <span className="text-[11px] text-zinc-400">→</span>
            </button>
            <button onClick={()=>openEmbeddedTab("calendar","Calendar")} className="flex items-center justify-between rounded-full border border-[#e8e0c8] bg-[#fefcf6] px-3 py-2.5 text-left text-sm font-medium text-zinc-700 hover:bg-[#f5efe6]">
              <span className="flex items-center gap-2"><Ico d={P.calendar} size={14} cls="text-zinc-500" /> Calendar</span>
              <span className="text-[11px] text-zinc-400">↗</span>
            </button>
            <button onClick={()=>openEmbeddedTab("domains","Domains")} className="flex items-center justify-between rounded-full border border-[#e8e0c8] bg-[#fefcf6] px-3 py-2.5 text-left text-sm font-medium text-zinc-700 hover:bg-[#f5efe6]">
              <span className="flex items-center gap-2"><Ico d={P.globe} size={14} cls="text-zinc-500" /> Domains</span>
              <span className="rounded-full bg-[#f0ece0] px-2 py-0.5 text-[11px] font-semibold text-[#005a5e]">aivory.uk</span>
            </button>
          </div>
        </div>

        <div className="mx-3 mb-4 rounded-2xl border border-[#e8e0c8] bg-[#f0ece0] p-3">
          <div className="text-xs font-semibold text-[#202124]">AI Triage</div>
          <div className="mt-1 text-[11px] leading-relaxed text-zinc-500">
            Email → Intelligence → Workflow → Action
          </div>
          <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-[#fefcf6]">
            <div className="h-full w-2/3 rounded-full bg-[#005a5e]" />
          </div>
          <div className="mt-1.5 text-[10px] text-zinc-400">Heuristic + Cerveau gateway</div>
        </div>

        <div className="px-3 py-2 space-y-1">
          <button onClick={()=> setShowSigModal(true)} className="flex w-full items-center justify-center gap-1.5 rounded-full border border-[#e8e0c8] bg-[#fefcf6] px-3 py-1.5 text-xs font-medium hover:bg-[#f5efe6]"><Ico d={P.sig} size={12} cls="text-zinc-500" /> Signature {activeSig ? `• ${activeSig.name}` : ""}</button>
          <a href="https://book.aivory.uk/book/aivory-call" target="_blank" className="flex items-center justify-between rounded-full border border-[#e8e0c8] bg-[#fefcf6] px-3 py-1.5 text-xs hover:bg-[#f5efe6]">
            <span className="flex items-center gap-1.5"><Ico d={P.calendar} size={12} cls="text-zinc-500" /> Aivory Calendar • book.aivory.uk</span>
            <span className="text-[11px] text-zinc-400">↗</span>
          </a>
        </div>
        <div className="border-t border-[#f0ece0] px-3 py-3">
          <div className="text-[11px] text-zinc-400">MAIL_MODE: vps · storage: local</div>
          <a
            href="http://localhost:8095/health"
            target="_blank"
            className="text-[11px] font-medium text-[#005a5e] underline decoration-[#e8e0c8] underline-offset-2 hover:text-[#00454a]"
          >
            API health ↗
          </a>
        </div>
      </aside>

      {/* Content — Mailflare spaced: #f8f6ef bg, main rounded-tl-3xl white — Zoho tab model */}
      <div className="flex min-w-0 flex-1 flex-col bg-[#f8f6ef]">
        <div className="flex h-9 shrink-0 items-center gap-2 border-b border-zinc-700 bg-zinc-800 px-3 text-xs text-zinc-300">
          <span className="flex items-center gap-1.5 rounded bg-[#fefcf6] px-2 py-1 text-xs font-semibold text-zinc-900"><Ico d={P.mail} size={12} /> Mail</span>
          <span className="text-zinc-500">·</span>
          <span className="hidden items-center gap-1 sm:flex"><Ico d={P.search} size={12} cls="text-zinc-500" /> Search</span>
          <input value={search} onChange={e=>setSearch(e.target.value)} placeholder="Search ( / )" className="ml-2 hidden w-48 rounded-full bg-[#fefcf6] px-3 py-1 text-xs text-zinc-700 placeholder:text-zinc-400 focus:outline-none sm:block" />
          <div className="ml-auto flex items-center gap-1">
            <button onClick={()=>openEmbeddedTab("settings-mail","Settings")} className="flex items-center gap-1.5 rounded-full bg-[#fefcf6]/10 px-2.5 py-1 text-[11px] font-medium text-white hover:bg-[#fefcf6]/15 border border-white/10"><Ico d={P.settings} size={11} /> Settings</button>
            <button onClick={()=>openEmbeddedTab("api-mcp","API & MCP")} className="hidden sm:flex items-center gap-1 rounded-full bg-[#fefcf6]/10 px-2 py-1 text-[11px] text-zinc-300 hover:bg-[#fefcf6]/15 border border-white/10"><Ico d={P.key} size={11} /> API</button>
            <span className="mx-1 h-4 w-px bg-[#fefcf6]/10" />
            <button onClick={()=>openEmbeddedTab("calendar","Calendar")} className="flex items-center gap-1 rounded-full bg-[#fefcf6] px-3 py-1 text-xs font-semibold text-zinc-900 hover:bg-zinc-100"><Ico d={P.calendar} size={11} /> Calendar</button>
            {composeOpen && <span className="ml-2 rounded bg-amber-400 px-2 py-1 text-xs font-semibold text-zinc-900">Composing…</span>}
          </div>
        </div>
        {/* Zoho-style tab bar — tabs live inside second+third panel, not browser tabs */}
        <div className="flex items-center gap-1 border-b border-[#e8e0c8] bg-[#f8f6ef] px-2 pt-2">
          {tabs.map(t=>(
            <button key={t.id} onClick={()=> setActiveTab(t.id)} className={`flex items-center gap-1.5 rounded-t-lg border border-b-0 px-3 py-1.5 text-xs font-medium transition ${activeTab===t.id ? "bg-[#fefcf6] border-[#e8e0c8] text-[#202124] shadow-sm" : "bg-[#f0ece0] border-transparent text-zinc-500 hover:bg-[#fefcf6] hover:border-[#e8e0c8]"}`}>
              {t.id==="mail" && <Ico d={P.mail} size={11} cls={activeTab===t.id ? "text-[#005a5e]" : "text-zinc-400"} />}
              {t.id==="calendar" && <Ico d={P.calendar} size={11} cls={activeTab===t.id ? "text-[#005a5e]" : "text-zinc-400"} />}
              {t.id==="settings-mail" && <Ico d={P.settings} size={11} cls={activeTab===t.id ? "text-[#005a5e]" : "text-zinc-400"} />}
              {t.id==="api-mcp" && <Ico d={P.key} size={11} cls={activeTab===t.id ? "text-[#005a5e]" : "text-zinc-400"} />}
              {t.id==="domains" && <Ico d={P.globe} size={11} cls={activeTab===t.id ? "text-[#005a5e]" : "text-zinc-400"} />}
              <span>{t.label}</span>
              {t.id!=="mail" && <span onClick={(e)=>{e.stopPropagation(); closeTab(t.id);}} className="ml-1 rounded p-0.5 text-[11px] leading-none text-zinc-400 hover:bg-zinc-100 hover:text-zinc-700">×</span>}
            </button>
          ))}
        </div>

        {activeTab !== "mail" ? (
          /* Zoho behavior: Calendar/Settings/MCP occupy second+third panel together — no list/detail split */
          <div className="flex min-w-0 flex-1 overflow-hidden rounded-tl-3xl bg-[#fefcf6] shadow-sm">
            <div className="min-w-0 flex-1 overflow-hidden bg-[#fefcf6]">
              {activeTab==="calendar" && <iframe src="/calendar" className="h-full w-full border-0" title="Calendar" />}
              {activeTab==="settings-mail" && <iframe src="/settings/mail" className="h-full w-full border-0" title="Settings" />}
              {activeTab==="api-mcp" && <iframe src="/settings" className="h-full w-full border-0" title="API & MCP" />}
              {activeTab==="domains" && (
                <div className="h-full w-full overflow-y-auto bg-[#f8f6ef] p-6">
                  <style>{`.animate-row{opacity:0;transform:translateY(6px);animation:rowIn 300ms cubic-bezier(0.23,1,0.32,1) forwards}@keyframes rowIn{to{opacity:1;transform:translateY(0)}}`}</style>
                  <div className="mx-auto w-full max-w-[1140px] space-y-5">
                    <div className="rounded-2xl border border-[#e8e0c8] bg-[#fefcf6] p-7 shadow-sm">
                      <div className="text-sm font-semibold tracking-widest text-zinc-500 uppercase">Add domain — Pilih metode</div>
                      <p className="mt-1.5 text-sm text-zinc-500">Auto = Mailflare (CF), Manual = Zoho (TXT hash) — user pilih.</p>
                      <div className="mt-4 inline-flex rounded-full border border-[#e8e0c8] bg-[#f8f6ef] p-1.5 text-sm">
                        <button onClick={()=>setDomainMode("auto")} className={`rounded-full px-5 py-2 font-semibold transition ${domainMode==="auto" ? "bg-[#005a5e] text-white shadow" : "text-zinc-600 hover:bg-[#fefcf6]"}`}>Auto — Cloudflare</button>
                        <button onClick={()=>setDomainMode("manual")} className={`rounded-full px-5 py-2 font-semibold transition ${domainMode==="manual" ? "bg-[#005a5e] text-white shadow" : "text-zinc-600 hover:bg-[#fefcf6]"}`}>Manual — TXT hash</button>
                      </div>
                      <div className="mt-4 flex gap-3">
                        <input value={domainHost} onChange={e=>setDomainHost(e.target.value)} placeholder="example.com" className="flex-1 rounded-full border border-[#e8e0c8] bg-[#f8f6ef] px-5 py-3 text-[15px] focus:border-[#005a5e] focus:bg-[#fefcf6] focus:outline-none" />
                        <button className="rounded-full bg-[#005a5e] px-7 py-3 text-[15px] font-semibold text-white active:scale-[0.97] transition-transform">{domainMode==="auto" ? "Provision auto" : "Generate hash"}</button>
                      </div>
                      <div className={`mt-4 rounded-xl px-4 py-3 text-sm ${domainMode==="auto" ? "bg-[#f0ece0]" : "border border-amber-200 bg-amber-50 font-mono"}`}>
                        {domainMode==="auto" ? <span>Auto via <code className="rounded bg-[#fefcf6] px-1.5 py-0.5">GET /zones?name=</code> → <code>enableEmailRouting</code> → <code>PUT catch_all worker</code> → no copy.</span> : <span className="text-[13px]">avry-verification={domainHost}-zb{Math.random().toString(36).slice(2,6)}… → TXT @ → Verify</span>}
                      </div>
                    </div>
                    <div className="rounded-2xl border border-[#e8e0c8] bg-[#fefcf6] p-7 shadow-sm">
                      <div className="flex items-center justify-between">
                        <span className="text-[15px] font-semibold flex items-center gap-2"><Ico d={P.globe} size={16} /> {domainHost} — {domainMode==="auto" ? "Auto" : "Manual"}</span>
                        <span className="flex gap-1.5"><Chip ok label={domainMode==="auto" ? "active" : "pending"} /><Chip ok={domainMode==="auto"} label={domainMode==="auto" ? "routing ✓" : "routing —"} /><Chip ok={false} label="DKIM —" /></span>
                      </div>
                      <div className="mt-4 overflow-hidden rounded-xl border border-[#e8e0c8]">
                        <div className="grid grid-cols-[68px_1fr_72px_148px] gap-px bg-[#e8e0c8] text-xs font-semibold"><div className="bg-[#f8f6ef] px-3 py-2.5">Type</div><div className="bg-[#f8f6ef] px-3 py-2.5">Host / Value</div><div className="bg-[#f8f6ef] px-3 py-2.5">Priority</div><div className="bg-[#f8f6ef] px-3 py-2.5">Status</div></div>
                        {[
                          ["MX", "@ → mx.aivory.uk / route.mx.cloudflare.net", "10", domainMode==="auto" ? "verified" : "yet to point"],
                          ["TXT", "@  v=spf1 include:_spf.mx.cloudflare.net ~all", "—", domainMode==="auto" ? "verified" : "unverified"],
                          ["TXT", "aivory._domainkey (DKIM)", "—", "yet to configure"],
                          ["TXT", "_dmarc  v=DMARC1; p=quarantine;", "—", "optional"],
                        ].map((r,i)=>(
                          <div key={i} className="grid grid-cols-[68px_1fr_72px_148px] gap-px bg-[#e8e0c8] text-sm animate-row" style={{animationDelay:`${i*40}ms` as any}}><div className="bg-[#fefcf6] px-3 py-3 font-mono text-[13px]">{r[0]}</div><div className="bg-[#fefcf6] px-3 py-3 text-[13px] whitespace-nowrap">{r[1]}</div><div className="bg-[#fefcf6] px-3 py-3 text-[13px] text-center">{r[2]}</div><div className="bg-[#fefcf6] px-3 py-3"><Chip ok={r[3]==="verified"} label={r[3]} /></div></div>
                        ))}
                      </div>
                      <div className="mt-4 flex gap-2.5">
                        <button className="flex-1 rounded-full bg-[#005a5e] px-6 py-3 text-[15px] font-semibold text-white hover:bg-[#00454a] active:scale-[0.97] transition-transform">{domainMode==="auto" ? "Re-check DNS (CF API)" : "Verify TXT"}</button>
                        <button className="rounded-full border border-[#e8e0c8] bg-[#fefcf6] px-5 py-3 text-sm">Send to DNS admin</button>
                        <button className="rounded-full border border-[#e8e0c8] bg-[#fefcf6] px-5 py-3 text-sm">Toolkit lookup</button>
                      </div>
                    </div>
                  </div>
                </div>
              )}
            </div>
          </div>
        ) : (
        <section className="flex min-w-0 flex-1 overflow-hidden rounded-tl-3xl bg-[#fefcf6] shadow-sm">
        {/* Message list — Mailflare hover #f2f6fc, active blue-50 */}
        <div className="flex w-[400px] shrink-0 flex-col border-r border-[#e8e0c8] bg-[#fefcf6]">
          <div className="sticky top-0 z-10 border-b border-[#e8e0c8] bg-[#fefcf6]">
            <div className="px-3 py-2">
              <input value={search} onChange={e=>setSearch(e.target.value)} placeholder="Search messages..." className="w-full rounded-full border border-[#e8e0c8] bg-[#f8f6ef] px-3 py-1.5 text-sm placeholder:text-zinc-400 focus:bg-[#fefcf6] focus:border-[#005a5e] focus:outline-none" />
            </div>
            <div className="flex items-center justify-between px-4 py-2">
            <span className="text-sm font-semibold text-[#202124]">
              {activeFolder} — {msgs.length}
            </span>
              <span className="rounded-full bg-[#005a5e] px-2 py-0.5 text-[11px] font-semibold text-white">
                {msgs.filter((m) => !m.is_read).length} new
              </span>
            </div>
          </div>

          <div className="flex-1 overflow-y-auto">
            {msgs.length === 0 && (
              <div className="p-8 text-center">
                <div className="mx-auto flex h-10 w-10 items-center justify-center rounded-full bg-[#f0ece0] text-[#005a5e]">
                  <Ico d={P.mail} size={16} cls="text-[#005a5e]" />
                </div>
                <p className="mt-3 text-sm font-medium text-[#202124]">No messages yet</p>
                <p className="mt-1 text-xs text-zinc-500">Send a test email to your mailbox.</p>
              </div>
            )}
            {msgs.map((m) => (
              <button
                key={m.id}
                onClick={() => open(m.id)}
                className={`flex w-full flex-col gap-1 border-b border-[#f0ece0] px-4 py-3 text-left transition hover:bg-[#f5efe6] hover:shadow-sm ${
                  selected?.id === m.id ? "bg-[#f0ece0] border-l-2 border-l-[#005a5e]" : "bg-[#fefcf6] border-l-2 border-l-transparent"
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

        {/* Detail — Mailflare card style */}
        <div className="flex min-w-0 flex-1 flex-col bg-[#f8f6ef]">
          {composeOpen ? (
            <div className="flex min-w-0 flex-1 flex-col bg-[#fefcf6] rounded-tl-3xl">
              <ComposeModal open={true} onClose={()=> { setComposeOpen(false); setReplyInfo(null); }} onSent={()=> { setComposeOpen(false); setReplyInfo(null); setSelected(null); }} defaultFrom={defaultFrom} replyTo={replyInfo} inline />
            </div>
          ) : !selected ? (
            <div className="flex flex-1 flex-col items-center justify-center p-10 text-center">
              <div className="rounded-2xl border border-dashed border-[#e8e0c8] bg-[#fefcf6] px-8 py-10">
                <div className="mx-auto flex h-12 w-12 items-center justify-center rounded-xl bg-[#005a5e] text-white">
                  <Ico d={P.mail} size={20} cls="text-white" />
                </div>
                <p className="mt-4 text-sm font-semibold text-[#202124]">Select a message</p>
                <p className="mt-1 max-w-[260px] text-xs leading-relaxed text-zinc-500">
                  Click a message on the left. Intelligence panel will show intent, urgency, and suggested actions.
                </p>
              </div>
            </div>
          ) : (
            <div className="flex flex-1 flex-col overflow-y-auto bg-[#f8f6ef]">
              <div className="border-b border-[#e8e0c8] bg-[#fefcf6] px-6 py-5">
                <h2 className="text-lg font-bold leading-tight text-[#202124]">{selected.subject}</h2>
                <div className="mt-2 flex flex-wrap items-center gap-2 text-xs text-zinc-500">
                  <span className="rounded-full border border-[#e8e0c8] bg-[#fefcf6] px-2.5 py-1 font-medium text-zinc-700">
                    From {selected.from}
                  </span>
                  <span>{new Date(selected.created_at).toLocaleString()}</span>
                  <span className="rounded-full bg-emerald-50 px-2 py-0.5 text-[11px] font-semibold text-emerald-700 ring-1 ring-emerald-200">
                    Inbox
                  </span>
                </div>
              </div>

              <div className="space-y-6 p-6">
                <div className="rounded-2xl border border-[#e8e0c8] bg-[#fefcf6] p-5 shadow-sm">
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
                  <button onClick={()=>openCompose(selected)} className="inline-flex items-center gap-1.5 rounded-full bg-[#005a5e] px-4 py-2 text-sm font-medium text-white shadow hover:bg-[#00454a]"><Ico d={P.reply} size={14} cls="text-white" /> Reply</button>
                  <button onClick={()=>{ setReplyInfo({ to: "", subject: `Fwd: ${selected.subject||""}`, body: selected.body_text || "" }); setComposeOpen(true);}} className="inline-flex items-center gap-1.5 rounded-full border border-[#e8e0c8] bg-[#fefcf6] px-4 py-2 text-sm font-medium text-zinc-700 hover:bg-[#f5efe6]"><Ico d={P.forward} size={14} cls="text-zinc-500" /> Forward</button>
                  <button onClick={()=>fetch(`${API}/v1/messages/${selected.id}/move`,{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({folder:"Archive"})}).then(()=> setSelected(null))} className="inline-flex items-center gap-1.5 rounded-full border border-[#e8e0c8] bg-[#fefcf6] px-4 py-2 text-sm font-medium text-zinc-500 hover:bg-[#f5efe6]"><Ico d={P.archive} size={14} cls="text-zinc-400" /> Archive</button>
                  <div className="relative">
                    <button onClick={()=> setShowSnooze(!showSnooze)} className="inline-flex items-center gap-1.5 rounded-full border border-[#e8e0c8] bg-[#fefcf6] px-3 py-2 text-xs font-medium hover:bg-[#f5efe6]"><Ico d={P.snoozed} size={12} cls="text-zinc-500" /> {selected.snoozed_until ? "Snoozed" : "Snooze"}</button>
                    {showSnooze && (
                      <div className="absolute left-0 top-full z-20 mt-1 w-44 rounded-xl border border-[#e8e0c8] bg-[#fefcf6] p-1 shadow-lg">
                        <button onClick={()=>{ doSnooze(selected.id, 1); setShowSnooze(false); }} className="w-full rounded-lg px-3 py-1.5 text-left text-xs hover:bg-[#f8f6ef]">1 hour</button>
                        <button onClick={()=>{ doSnooze(selected.id, 4); setShowSnooze(false); }} className="w-full rounded-lg px-3 py-1.5 text-left text-xs hover:bg-[#f8f6ef]">4 hours</button>
                        <button onClick={()=>{ const d=new Date(); d.setDate(d.getDate()+1); d.setHours(9,0,0,0); fetch(`${API}/v1/messages/${selected.id}/snooze`,{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({snoozed_until:d.toISOString()})}).then(()=>{ setMsgs(prev=>prev.filter(m=>m.id!==selected.id)); setSelected(null); setShowSnooze(false); }); }} className="w-full rounded-lg px-3 py-1.5 text-left text-xs hover:bg-[#f8f6ef]">Tomorrow 9am</button>
                        <button onClick={()=>{ doSnooze(selected.id, 24*7); setShowSnooze(false); }} className="w-full rounded-lg px-3 py-1.5 text-left text-xs hover:bg-[#f8f6ef]">Next week</button>
                        {selected.snoozed_until && <button onClick={()=>{ doUnsnooze(selected.id); setShowSnooze(false); }} className="w-full rounded-lg px-3 py-1.5 text-left text-xs text-amber-700 hover:bg-amber-50">Unsnooze</button>}
                      </div>
                    )}
                  </div>
                  <button onClick={()=>toggleStar(selected.id)} className={`inline-flex items-center gap-1 rounded-full border px-3 py-2 text-xs font-semibold ${selected.is_starred ? "border-amber-300 bg-amber-50 text-amber-800" : "border-[#e8e0c8] bg-[#fefcf6] text-zinc-600 hover:bg-[#f5efe6]"}`}><Ico d={P.star} size={12} cls={selected.is_starred ? "text-amber-500" : "text-zinc-400"} /> {selected.is_starred ? "Starred" : "Star"}</button>
                  <button onClick={()=>doShare(selected.id)} className="inline-flex items-center gap-1.5 rounded-full border border-[#e8e0c8] bg-[#fefcf6] px-3 py-2 text-xs font-medium hover:bg-[#f5efe6]"><Ico d={P.link} size={12} cls="text-zinc-500" /> Share link</button>
                  <button onClick={()=>doBlock(selected.from)} className="inline-flex items-center gap-1 rounded-full border border-red-200 bg-white px-3 py-2 text-xs font-medium text-red-600 hover:bg-red-50"><Ico d={P.block} size={12} cls="text-red-500" /> Block</button>
                  <button className="ml-auto rounded-lg border border-amber-200 bg-amber-50 px-3 py-2 text-xs font-semibold text-amber-800">AI: Create Finance Task</button>
                </div>
                {shareUrl && <div className="rounded-lg border border-emerald-200 bg-emerald-50 px-3 py-2 text-xs"><span className="font-semibold">Share link copied:</span> <a href={shareUrl} target="_blank" className="break-all text-emerald-800 underline">{shareUrl}</a></div>}
                {selected.attachments?.length > 0 && (
                  <div className="rounded-xl border border-zinc-200 bg-[#fefcf6] p-4">
                    <div className="text-xs font-semibold">Attachments · {selected.attachments.length}</div>
                    <div className="mt-2 space-y-2">
                      {selected.attachments.map((a:any)=> (
                        <a key={a.id} href={`${API}/v1/messages/${selected.id}/attachments/${a.id}`} target="_blank" className="flex items-center justify-between rounded-lg border border-zinc-200 bg-zinc-50 px-3 py-2 text-xs hover:bg-[#fefcf6]">
                          <span className="truncate font-medium">{a.filename} · {(a.size_bytes/1024).toFixed(1)} KB · {a.content_type}</span>
                          <span className="ml-2 shrink-0 rounded bg-zinc-900 px-2 py-1 text-[11px] font-semibold text-white">Download</span>
                        </a>
                      ))}
                    </div>
                  </div>
                )}

                {crawl && (
                  <div className="rounded-xl border border-zinc-200 bg-[#fefcf6] p-4">
                    <div className="flex items-center justify-between">
                      <span className="text-xs font-semibold">Thread crawl • {crawl.message_count} messages</span>
                      <span className={`rounded-full px-2 py-0.5 text-[11px] font-semibold ${crawl.needs_follow_up ? "bg-amber-100 text-amber-800 ring-1 ring-amber-200" : "bg-zinc-100 text-zinc-600"}`}>{crawl.needs_follow_up ? "Needs follow-up" : `${crawl.days_since_last}d since last`}</span>
                    </div>
                    <div className="mt-2 space-y-1">
                      {crawl.timeline?.slice(-5).map((t:any)=> (
                        <div key={t.idx} className="flex gap-2 text-xs"><span className={`mt-1 h-2 w-2 shrink-0 rounded-full ${t.is_outbound ? "bg-zinc-900" : "bg-blue-500"}`} /><span className="truncate"><span className="font-medium">{String(t.from?.from || t.from || "")}</span> — {String(t.snippet||"").slice(0,60)}</span><span className="ml-auto shrink-0 text-[11px] text-zinc-400">{String(t.at||"").slice(11,16)}</span></div>
                      ))}
                    </div>
                    {crawl.needs_follow_up && crawl.suggested_follow_up && (
                      <div className="mt-3 rounded-lg border border-amber-200 bg-amber-50 p-3">
                        <div className="text-xs font-semibold text-amber-900">Suggested follow-up</div>
                        <div className="mt-1 text-xs text-amber-800">{crawl.suggested_follow_up.reason}</div>
                        <div className="mt-2 text-xs"><span className="font-medium">Subj:</span> {crawl.suggested_follow_up.subject}</div>
                        <button onClick={()=>{ setReplyInfo({to: selected.from, subject: crawl.suggested_follow_up.subject, body: crawl.suggested_follow_up.body, thread_id: selected.thread_id}); setComposeOpen(true); }} className="mt-2 rounded bg-amber-900 px-3 py-1.5 text-xs font-semibold text-white hover:bg-black">Use follow-up draft →</button>
                      </div>
                    )}
                    <div className="mt-2 flex gap-2">
                      <div className="flex gap-1">
                        <button onClick={()=> { const url = `https://mail.aivory.uk/calendar`; const bookUrl = `https://book.aivory.uk/book/aivory-call`; const full = `${url} (or book directly: ${bookUrl})`; navigator.clipboard?.writeText(full); setReplyInfo({to: selected.from, subject: `Re: ${selected.subject||""}`, body: `Hi,\n\nHere is my calendar to pick a time: ${url}\nPrefer CalNode booking: ${bookUrl}\n\nBest`, thread_id: selected.thread_id}); setComposeOpen(true); }} className="inline-flex items-center gap-1 rounded border border-zinc-200 bg-[#fefcf6] px-2.5 py-1 text-xs hover:bg-zinc-50"><Ico d={P.calendar} size={12} cls="text-zinc-500" /> Insert calendar link</button>
                        <a href="/calendar" target="_blank" className="rounded border border-zinc-200 bg-[#fefcf6] px-2.5 py-1 text-xs hover:bg-zinc-50">Open calendar ↗</a>
                      </div>
                      <span className="text-[11px] text-zinc-400 self-center">via Aivory Calendar</span>
                    </div>
                  </div>
                )}
                <div className="rounded-xl border border-[#e8e0c8] bg-[#fefcf6] p-4">
                  <div className="flex items-center justify-between">
                    <span className="text-xs font-semibold text-[#202124]">Intelligence {intelLoading ? "• analyzing…" : intel?.ai ? "• heuristic + AI" : intel ? "• heuristic" : ""}</span>
                    {intel?.urgency && <span className={`rounded-full px-2 py-0.5 text-[11px] font-semibold ring-1 ${intel.urgency==="High" ? "bg-red-50 text-red-700 ring-red-200" : intel.urgency==="Medium" ? "bg-amber-50 text-amber-700 ring-amber-200" : "bg-zinc-100 text-zinc-600"}`}>{intel.urgency}</span>}
                  </div>
                  {intelLoading ? (
                    <div className="mt-2 text-xs text-zinc-400">Analyzing with heuristic{intel?.ai ? " + AI gateway" : ""}…</div>
                  ) : intel ? (
                    <>
                      <div className="mt-2 flex flex-wrap gap-1.5">
                        {intel.intent && <span className="rounded-full bg-[#005a5e] px-2.5 py-1 text-xs font-medium text-white">{intel.intent}</span>}
                        {intel.entities?.map((e:any, i:number)=> <span key={i} className="rounded-full bg-zinc-100 px-2.5 py-1 text-xs text-zinc-700">{e.value || e.kind || e}</span>)}
                        {intel.ai?.entities?.map((e:any,i:number)=> <span key={"ai"+i} className="rounded-full bg-[#f0ece0] px-2.5 py-1 text-xs text-[#005a5e] ring-1 ring-[#e8e0c8]">{e.value}</span>)}
                      </div>
                      {intel.summary && <div className="mt-2 text-xs leading-relaxed text-zinc-600">{intel.summary}</div>}
                      {intel.ai?.summary && <div className="mt-1 text-xs leading-relaxed text-zinc-500">AI: {intel.ai.summary}</div>}
                      {intel.suggested_actions?.length > 0 && (
                        <div className="mt-3 flex flex-wrap gap-1.5">
                          {intel.suggested_actions.map((a:any,i:number)=> (
                            <button key={i} onClick={()=>{
                              if (a.action==="create_task" || a.type==="create_task") { fetch(`${API}/v1/agent/actions`,{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({action:"create_task", entity:a})}); }
                              if (a.action==="draft_reply" || a.type==="draft_reply") { fetch(`${API}/v1/intelligence/suggest`,{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({subject: selected.subject, body: selected.body_text})}).then(r=>r.json()).then(j=>{ const draft=j.data?.draft || j.draft; if(draft){ setReplyInfo({to:selected.from, subject:`Re: ${selected.subject}`, body:draft, thread_id:selected.thread_id}); setComposeOpen(true); } }); }
                            }} className="rounded-full border border-[#e8e0c8] bg-white px-2.5 py-1 text-[11px] font-medium hover:bg-[#f8f6ef]">{a.action || a.type || a}</button>
                          ))}
                        </div>
                      )}
                      <div className="mt-3 text-xs leading-relaxed text-zinc-500">
                        {intel.ai ? "Heuristic + AI gateway merged — workflow will trigger per intent." : "Heuristic only — set AI_GATEWAY_URL to enable LLM merge."}
                      </div>
                    </>
                  ) : (
                    <div className="mt-2 text-xs text-zinc-400">Select a message to analyze.</div>
                  )}
                </div>
              </div>
            </div>
          )}
        </div>
      </section>
      )}
      </div>
      {showSigModal && (
        <div className="fixed inset-0 z-40 flex items-center justify-center bg-black/20 p-4">
          <div className="w-full max-w-md rounded-xl border border-zinc-200 bg-[#fefcf6] p-4 shadow-xl">
            <div className="flex items-center justify-between"><span className="text-sm font-semibold">Signature — {defaultFrom}</span><button onClick={()=> setShowSigModal(false)} className="rounded p-1 hover:bg-zinc-100">✕</button></div>
            <div className="mt-3 space-y-2">
              <textarea value={sigHtml || activeSig?.html || ""} onChange={e=> setSigHtml(e.target.value)} placeholder="<p>Best,<br/>Your Name<br/>Aivory | book.aivory.uk</p>" rows={4} className="w-full rounded border border-zinc-200 px-3 py-2 text-xs" />
              <div className="text-[11px] text-zinc-500">Supports HTML. Auto-appended to new compose if Default.</div>
              <div className="flex gap-2">
                <button onClick={async()=>{ const mb = mailboxes.find((m:any)=> m.address===defaultFrom); if(!mb) return; await fetch(`${API}/v1/signatures`,{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({mailbox_id: mb.id, name:"Default", html: sigHtml, text: sigHtml.replace(/<[^>]+>/g,""), is_default:true})}); const r=await fetch(`${API}/v1/signatures?mailbox_id=${mb.id}`); const j=await r.json(); const list=j.data||[]; setSignatures(list); setActiveSig(list.find((s:any)=>s.is_default)||list[0]); setShowSigModal(false); }} className="rounded bg-zinc-900 px-3 py-1.5 text-xs font-semibold text-white">Save as Default</button>
                <button onClick={()=> setShowSigModal(false)} className="rounded border border-zinc-200 px-3 py-1.5 text-xs">Close</button>
              </div>
              {activeSig && <div className="rounded border border-zinc-100 bg-zinc-50 p-2 text-xs" dangerouslySetInnerHTML={{__html: activeSig.html}} />}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
