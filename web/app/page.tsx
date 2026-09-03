"use client";
import { useEffect, useState } from "react";
import ComposeModal from "../components/ComposeModal";
import AskAIAssistant from "../components/AskAIAssistant";

const API = process.env.NEXT_PUBLIC_MAIL_API || "http://localhost:8095";
const BOOK_URL = process.env.NEXT_PUBLIC_BOOK_URL || "https://book.aivory.uk/book/aivory-call";
const MAIL_MX_HOST = process.env.NEXT_PUBLIC_MAIL_MX_HOST || "mail.aivory.uk";

// Outline icons — no emoticon (hybrid rule)
function Ico({ d, size = 16, cls = "" }: { d: string; size?: number; cls?: string }) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.65} strokeLinecap="round" strokeLinejoin="round" className={cls} aria-hidden><path d={d} /></svg>;
}
function Chip({ ok, label }: { ok: boolean; label: string }) {
  return <span className={`inline-flex items-center gap-1 rounded-lg px-2 py-0.5 text-[10px] font-semibold ring-1 ${ok ? "bg-emerald-50 text-emerald-700 ring-emerald-200" : "bg-amber-50 text-amber-700 ring-amber-200"}`}>{ok ? <Ico d={P.check} size={10} /> : <Ico d={P.alert} size={10} />}{label}</span>;
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
  const [defaultFrom, setDefaultFrom] = useState("");
  const [shareUrl, setShareUrl] = useState("");
  const [signatures, setSignatures] = useState<any[]>([]);
  const [activeSig, setActiveSig] = useState<any>(null);
  const [showSigModal, setShowSigModal] = useState(false);
  const [sigHtml, setSigHtml] = useState("");
  const [calStatus, setCalStatus] = useState<any>(null);
  const [healthInfo, setHealthInfo] = useState<any>(null);
  const [msgLabels, setMsgLabels] = useState<any[]>([]);
  const [allLabels, setAllLabels] = useState<any[]>([]);
  const [crawl, setCrawl] = useState<any>(null);
  const [domains, setDomains] = useState<any[]>([]);
  const [folderCounts, setFolderCounts] = useState<Record<string,number>>({});
  const [customFolders, setCustomFolders] = useState<any[]>([]);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [tabs, setTabs] = useState<{id:string,label:string}[]>([{id:"mail",label:"Mail"}]);
  const [activeTab, setActiveTab] = useState("mail");
  const [showSnooze, setShowSnooze] = useState(false);
  const [showAvatar, setShowAvatar] = useState(false);
  const [intel, setIntel] = useState<any>(null);
  const [intelLoading, setIntelLoading] = useState(false);
  function openEmbeddedTab(id:string,label:string){
    setTabs(prev=> prev.find(t=>t.id===id) ? prev : [...prev, {id,label}]);
    setActiveTab(id);
  }
  const [general, setGeneral] = useState<any>({ undo_send_seconds: "10", density: "comfortable", conversation_view: "false", page_size: "20" });
  const [appearance, setAppearance] = useState<any>({ theme: "light", reading_pane: "right" });
  const [threads, setThreads] = useState<any[]>([]);
  const [selectedThread, setSelectedThread] = useState<any>(null);

  useEffect(() => {
    fetch(`${API}/v1/settings?category=general`).then(r=>r.json()).then(j=> { if (j.data) setGeneral(j.data); }).catch(()=>{});
    fetch(`${API}/v1/settings?category=appearance`).then(r=>r.json()).then(j=> { if (j.data) setAppearance(j.data); }).catch(()=>{});
    // poll appearance for live update when changed in settings tab
    const iv = setInterval(()=> fetch(`${API}/v1/settings?category=appearance`).then(r=>r.json()).then(j=> { if (j.data) setAppearance(j.data); }).catch(()=>{}), 3000);
    return ()=> clearInterval(iv);
  }, []);

  const conversationView = general.conversation_view === "true";
  const density = general.density || "comfortable";
  const rowPad = density === "compact" ? "py-1.5" : density === "cozy" ? "py-2" : "py-3";

  useEffect(() => {
    setSelectedThread(null);
    // Conversation view only for Inbox; other folders show messages directly (Gmail/Zoho parity)
    if (conversationView && activeFolder==="Inbox" && !search) {
      fetch(`${API}/v1/threads`).then(r=>r.json()).then(j=> setThreads(j.data || [])).catch(()=>{});
      // also fetch Inbox messages for counts fallback
      fetch(`${API}/v1/messages?folder=Inbox&per_page=1`).then(r=>r.json()).then(j=> {
        if (Array.isArray(j.data)) setFolderCounts(prev=> ({...prev, Inbox: j.data.length || 0}));
      }).catch(()=>{});
      return;
    }
    const q = search ? `&search=${encodeURIComponent(search)}` : "";
    const perPage = general.page_size || "20";
    // Drafts: also via messages folder=Drafts (backend stores drafts as messages)
    fetch(`${API}/v1/messages?folder=${encodeURIComponent(activeFolder)}&per_page=${perPage}${q}`)
      .then((r) => r.json())
      .then((j) => setMsgs(j.data || []))
      .catch(() => {});
  }, [activeFolder, selected, search, conversationView, general.page_size]);

  async function openThread(id: string) {
    const r = await fetch(`${API}/v1/threads/${id}`);
    const j = await r.json();
    setSelectedThread(j.data);
    setSelected(null);
  }

  useEffect(() => {
    fetch(`${API}/v1/mailboxes`).then(r=>r.json()).then(j=>{
      const list = j.data || [];
      setMailboxes(list);
      if (list[0]?.address) setDefaultFrom(list[0].address);
    }).catch(()=>{});
    fetch(`${API}/v1/domains`).then(r=>r.json()).then(j=> setDomains(j.data || [])).catch(()=>{});
    fetch(`${API}/v1/calendar/status`).then(r=>r.json()).then(j=> setCalStatus(j.data || j)).catch(()=>{});
    fetch(`${API}/health`).then(r=>r.json()).then(j=> setHealthInfo(j)).catch(()=>{});
    // folder counts — real API, not hard-coded (via /v1/stats by_folder)
    fetch(`${API}/v1/stats`).then(r=>r.json()).then(j=>{
      const by = (j as any).by_folder || (j as any).data?.by_folder;
      if (by && typeof by === 'object') setFolderCounts(by);
    }).catch(()=>{});
    fetch(`${API}/v1/folders`).then(r=>r.json()).then(j=> setCustomFolders(j.data || [])).catch(()=>{});
    fetch(`${API}/v1/labels`).then(r=>r.json()).then(j=> setAllLabels(j.data || [])).catch(()=>{});
    // notifications: request permission if enabled
    fetch(`${API}/v1/settings?category=notifications`).then(r=>r.json()).then(j=>{
      if (j.data?.new_mail_banner==="true" && "Notification" in window && Notification.permission==="default") Notification.requestPermission().catch(()=>{});
    }).catch(()=>{});
    // auth guard — redirect to login if no token
    const token = typeof window !== "undefined" ? localStorage.getItem("aivory_mail_token") : null;
    if (!token) {
      const isLogin = typeof window !== "undefined" && window.location.pathname === "/login";
      if (!isLogin) window.location.href = "/login";
    }
  }, []);
  useEffect(()=>{
    if (!selected?.id) { setMsgLabels([]); return; }
    fetch(`${API}/v1/messages/${selected.id}/labels`).then(r=>r.json()).then(j=> setMsgLabels(j.data || [])).catch(()=> setMsgLabels([]));
  }, [selected?.id]);
  // Shortcuts: c compose, e archive, r reply, / search, x select, s star, # delete
  useEffect(()=>{
    function onKey(e: KeyboardEvent){
      const target = e.target as HTMLElement;
      if (target && (target.tagName==="INPUT" || target.tagName==="TEXTAREA" || target.isContentEditable)) return;
      fetch(`${API}/v1/settings?category=shortcuts`).then(r=>r.json()).then(j=>{
        if (j.data?.enabled==="false") return;
        if (e.key==="c" && !e.metaKey && !e.ctrlKey) { e.preventDefault(); openCompose(); }
        if (e.key==="e" && selected) { e.preventDefault(); bulkMove("Archive"); }
        if (e.key==="r" && selected) { e.preventDefault(); openCompose(selected); }
        if (e.key==="/") { e.preventDefault(); (document.querySelector('input[placeholder*="Search"]') as HTMLInputElement)?.focus(); }
        if (e.key==="x" && selected) { e.preventDefault(); toggleSelect(selected.id); }
        if (e.key==="s" && selected) { e.preventDefault(); toggleStar(selected.id); }
        if (e.key==="#" && selected) { e.preventDefault(); bulkDelete(); }
      }).catch(()=>{});
    }
    window.addEventListener("keydown", onKey);
    return ()=> window.removeEventListener("keydown", onKey);
  }, [selected, msgs, selectedIds]);
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
  function toggleSelect(id:string){ setSelectedIds(prev=>{ const n=new Set(prev); if(n.has(id)) n.delete(id); else n.add(id); return n; }); }
  function toggleSelectAll(){
    if (conversationView && activeFolder==="Inbox") {
      if (selectedIds.size===threads.length) setSelectedIds(new Set());
      else setSelectedIds(new Set(threads.map((t:any)=> t.id)));
    } else {
      if (selectedIds.size===msgs.length && msgs.length>0) setSelectedIds(new Set());
      else setSelectedIds(new Set(msgs.map(m=> m.id)));
    }
  }
  async function refreshCounts(){ try{ const r=await fetch(`${API}/v1/stats`); const j=await r.json(); const by=(j as any).by_folder || (j as any).data?.by_folder; if(by) setFolderCounts(by);}catch{} }
  async function bulkMarkRead(isRead:boolean){
    const isThreadView = conversationView && activeFolder==="Inbox" && !search && threads.length>0;
    if (isThreadView) {
      const tids = Array.from(selectedIds).length? Array.from(selectedIds) : threads.map((t:any)=> t.id);
      if (!tids.length) return;
      for (const tid of tids) {
        try {
          const r = await fetch(`${API}/v1/threads/${tid}`);
          const j = await r.json();
          const tmsgs = j.data?.messages || [];
          await Promise.all(tmsgs.map((m:any)=> fetch(`${API}/v1/messages/${m.id}/read`,{method:"PUT", headers:{"content-type":"application/json"}, body: JSON.stringify({is_read:isRead})})));
        } catch {}
      }
      setThreads(prev=> prev.map((t:any)=> tids.includes(t.id) ? {...t, has_unread: !isRead} as any : t));
      setSelectedIds(new Set());
      refreshCounts();
      return;
    }
    const ids = Array.from(selectedIds);
    const targets = ids.length? ids : msgs.map(m=> m.id);
    if (!targets.length) return;
    await Promise.all(targets.map(id=> fetch(`${API}/v1/messages/${id}/read`,{method:"PUT", headers:{"content-type":"application/json"}, body: JSON.stringify({is_read:isRead})})));
    setMsgs(prev=> prev.map(m=> targets.includes(m.id) ? {...m, is_read:isRead} as any : m));
    setSelectedIds(new Set());
    refreshCounts();
  }
  async function bulkDelete(){
    const isThreadView = conversationView && activeFolder==="Inbox" && !search && threads.length>0;
    if (isThreadView) {
      const tids = Array.from(selectedIds).length? Array.from(selectedIds) : threads.map((t:any)=> t.id);
      if (!tids.length) return;
      if (!confirm(`Delete ${tids.length} conversation(s)?`)) return;
      for (const tid of tids) {
        try {
          const r = await fetch(`${API}/v1/threads/${tid}`);
          const j = await r.json();
          const tmsgs = j.data?.messages || [];
          await Promise.all(tmsgs.map((m:any)=> fetch(`${API}/v1/messages/${m.id}`,{method:"DELETE"})));
        } catch {}
      }
      setThreads(prev=> prev.filter((t:any)=> !tids.includes(t.id)));
      setSelectedIds(new Set());
      setSelectedThread(null);
      refreshCounts();
      return;
    }
    const ids = Array.from(selectedIds);
    const targets = ids.length? ids : msgs.map(m=> m.id);
    if (!targets.length) return;
    if (!confirm(`Delete ${targets.length} message(s)?`)) return;
    await Promise.all(targets.map(id=> fetch(`${API}/v1/messages/${id}`,{method:"DELETE"})));
    setMsgs(prev=> prev.filter(m=> !targets.includes(m.id)));
    setSelectedIds(new Set());
    setSelected(null);
    refreshCounts();
  }
  async function bulkMove(folder:string){
    const isThreadView = conversationView && activeFolder==="Inbox" && !search && threads.length>0;
    if (isThreadView) {
      const tids = Array.from(selectedIds).length? Array.from(selectedIds) : (selectedThread? [selectedThread.id] : []);
      if (!tids.length) return;
      for (const tid of tids) {
        try {
          const r = await fetch(`${API}/v1/threads/${tid}`);
          const j = await r.json();
          const tmsgs = j.data?.messages || [];
          await Promise.all(tmsgs.map((m:any)=> fetch(`${API}/v1/messages/${m.id}/move`,{method:"POST", headers:{"content-type":"application/json"}, body: JSON.stringify({folder})})));
        } catch {}
      }
      setThreads(prev=> prev.filter((t:any)=> !tids.includes(t.id)));
      setSelectedIds(new Set());
      setSelectedThread(null);
      refreshCounts();
      return;
    }
    const ids = Array.from(selectedIds);
    const targets = ids.length? ids : (selected? [selected.id] : []);
    if (!targets.length) return;
    await Promise.all(targets.map(id=> fetch(`${API}/v1/messages/${id}/move`,{method:"POST", headers:{"content-type":"application/json"}, body: JSON.stringify({folder})})));
    setMsgs(prev=> prev.filter(m=> !targets.includes(m.id)));
    setSelectedIds(new Set());
    if (targets.includes(selected?.id)) setSelected(null);
    refreshCounts();
  }
  async function attachLabel(labelId:string){ if(!selected) return; await fetch(`${API}/v1/messages/${selected.id}/labels`,{method:"POST", headers:{"content-type":"application/json"}, body: JSON.stringify({label_id:labelId})}); const r=await fetch(`${API}/v1/messages/${selected.id}/labels`); const j=await r.json(); setMsgLabels(j.data||[]); }
  async function detachLabel(labelId:string){ if(!selected) return; await fetch(`${API}/v1/messages/${selected.id}/labels/${labelId}`,{method:"DELETE"}); setMsgLabels(prev=> prev.filter((l:any)=> l.id!==labelId)); }
  function doLogout(){ localStorage.removeItem("aivory_mail_token"); localStorage.removeItem("aivory_mail_email"); document.cookie = "aivory_mail_token=; path=/; max-age=0"; window.location.href="/login"; }
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

  const isDark = appearance.theme==="dark";
  const isBottomPane = appearance.reading_pane==="bottom";
  const isNoSplit = appearance.reading_pane==="no-split";
  return (
    <div className={`flex h-screen ${isDark ? "bg-zinc-900 text-zinc-100" : "bg-[#f8f6ef] text-[#202124]"}`}>
      {/* Sidebar — Mailflare light, blue-accented with Aivory_mail_logo2.svg */}
      <aside className={`flex w-[280px] shrink-0 flex-col border-r ${isDark ? "border-zinc-700 bg-zinc-800" : "border-[#e8e0c8] bg-[#fefcf6]"}`}>
        <div className="border-b border-[#f0ece0] px-3 py-5">
          <img src="/aivory-mail-logo3.svg" alt="Aivory Mail" className="w-full max-w-[227px] h-auto object-contain object-left" />
        </div>

        <div className="px-3 pt-3">
          <button onClick={()=>openCompose()} className="flex w-full items-center justify-center gap-2 rounded-lg bg-[#005a5e] px-4 py-3 text-sm font-semibold text-white shadow hover:bg-[#00454a]"><Ico d={P.compose} size={14} cls="text-white" /> Compose</button>
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
          ].map((f) => {
            const count = folderCounts[f.label];
            const displayCount = f.label===activeFolder ? (msgs.length || count || 0) : (count || 0);
            return (
            <button
              key={f.label}
              onClick={() => setActiveFolder(f.label)}
              className={`flex items-center gap-2 rounded-lg border px-3 py-2.5 text-left text-sm font-medium transition cursor-pointer ${
                f.label === activeFolder
                  ? "border-[#005a5e] bg-[#005a5e] text-white shadow-sm"
                  : "border-[#e8e0c8] bg-[#fefcf6] text-zinc-700 hover:bg-[#f5efe6] hover:border-[#005a5e]/30"
              }`}
            >
              <Ico d={f.icon} size={15} cls={f.label === activeFolder ? "text-white" : "text-zinc-500"} />
              <span className="flex-1">{f.label}</span>
              {displayCount > 0 && (
                <span className={`rounded-lg px-2 py-0.5 text-xs font-semibold ${activeFolder === f.label ? "bg-[#fefcf6] text-[#005a5e]" : "bg-[#f0ece0] text-[#005a5e]"}`}>{displayCount}</span>
              )}
            </button>
          )})}
          {customFolders.length > 0 && (
            <>
              <div className="mt-2 px-2 text-[10px] font-semibold tracking-widest text-zinc-400 uppercase">Folders</div>
              {customFolders.map((cf:any)=> (
                <button key={cf.id} onClick={()=> setActiveFolder(cf.name)} className={`flex items-center gap-2 rounded-lg border px-3 py-2 text-left text-xs font-medium transition cursor-pointer ${cf.name===activeFolder ? "border-[#005a5e] bg-[#005a5e] text-white" : "border-[#e8e0c8] bg-[#fefcf6] text-zinc-700 hover:bg-[#f5efe6]"}`}>
                  <span className="h-2 w-2 rounded-full" style={{background: cf.color || "#006355"}} />
                  <span className="flex-1 truncate">{cf.name}</span>
                </button>
              ))}
            </>
          )}
        </nav>
        {/* Hybrid — Manage section — Zoho-like: open as tab in second+third panel */}
        <div className="px-3">
          <div className="my-2 h-px bg-[#f0ece0]" />
          <div className="px-2 pb-1 text-[10px] font-semibold tracking-widest text-zinc-400 uppercase">Manage</div>
          <div className="flex flex-col gap-1.5">
            <button onClick={()=>openEmbeddedTab("settings-mail","Settings")} className="flex items-center justify-between rounded-lg border border-[#005a5e] bg-[#005a5e] px-3 py-2.5 text-left text-sm font-medium text-white shadow-sm">
              <span className="flex items-center gap-2"><Ico d={P.settings} size={14} cls="text-white" /> Settings</span>
              <span className="rounded-lg bg-[#fefcf6] px-1.5 py-0.5 text-[10px] font-bold text-[#005a5e]">10</span>
            </button>
            <button onClick={()=>openEmbeddedTab("api-mcp","API & MCP")} className="flex items-center justify-between rounded-lg border border-[#e8e0c8] bg-[#fefcf6] px-3 py-2.5 text-left text-sm font-medium text-zinc-700 hover:bg-[#f5efe6]">
              <span className="flex items-center gap-2"><Ico d={P.key} size={14} cls="text-zinc-500" /> API & MCP</span>
              <span className="text-[11px] text-zinc-400">→</span>
            </button>
            <button onClick={()=>openEmbeddedTab("calendar","Calendar")} className="flex items-center justify-between rounded-lg border border-[#e8e0c8] bg-[#fefcf6] px-3 py-2.5 text-left text-sm font-medium text-zinc-700 hover:bg-[#f5efe6]">
              <span className="flex items-center gap-2"><Ico d={P.calendar} size={14} cls="text-zinc-500" /> Calendar</span>
              <span className="text-[11px] text-zinc-400">↗</span>
            </button>
            <button onClick={()=>openEmbeddedTab("domains","Domains")} className="flex items-center justify-between rounded-lg border border-[#e8e0c8] bg-[#fefcf6] px-3 py-2.5 text-left text-sm font-medium text-zinc-700 hover:bg-[#f5efe6]">
              <span className="flex items-center gap-2"><Ico d={P.globe} size={14} cls="text-zinc-500" /> Domains</span>
              <span className="rounded-lg bg-[#f0ece0] px-2 py-0.5 text-[11px] font-semibold text-[#005a5e]">{domains[0]?.domain || (mailboxes[0]?.address?.split("@")[1] || "no domain")}</span>
            </button>
          </div>
        </div>

        <div className="mx-3 mb-4 rounded-2xl border border-[#e8e0c8] bg-[#f0ece0] p-3">
          <div className="text-xs font-semibold text-[#202124]">AI Triage</div>
          <div className="mt-1 text-[11px] leading-relaxed text-zinc-500">
            Email → Intelligence → Workflow → Action
          </div>
          <div className="mt-2 h-1.5 overflow-hidden rounded-lg bg-[#fefcf6]">
            <div className="h-full w-2/3 rounded-lg bg-[#005a5e]" />
          </div>
          <div className="mt-1.5 text-[10px] text-zinc-400">Heuristic + Cerveau gateway</div>
        </div>

        <div className="px-3 py-2 space-y-1">
          <button onClick={()=> setShowSigModal(true)} className="flex w-full items-center justify-center gap-1.5 rounded-lg border border-[#e8e0c8] bg-[#fefcf6] px-3 py-1.5 text-xs font-medium hover:bg-[#f5efe6]"><Ico d={P.sig} size={12} cls="text-zinc-500" /> Signature {activeSig ? `• ${activeSig.name}` : ""}</button>
          <a href={BOOK_URL} target="_blank" className="flex items-center justify-between rounded-lg border border-[#e8e0c8] bg-[#fefcf6] px-3 py-1.5 text-xs hover:bg-[#f5efe6]">
            <span className="flex items-center gap-1.5"><Ico d={P.calendar} size={12} cls="text-zinc-500" /> Aivory Calendar • {BOOK_URL.replace(/^https?:\/\//,"")}</span>
            <span className="text-[11px] text-zinc-400">↗</span>
          </a>
          <a href="/settings/mail" className="flex items-center gap-2 rounded-lg border border-zinc-200 bg-white px-3 py-1.5 text-xs text-zinc-700 transition-colors duration-150 hover:border-zinc-300 hover:bg-zinc-50 active:scale-[0.98]">
            <svg className="h-3.5 w-3.5 shrink-0 text-zinc-400" fill="none" stroke="currentColor" strokeWidth="1.5" viewBox="0 0 24 24"><path strokeLinecap="round" strokeLinejoin="round" d="M9.594 3.94c.09-.542.56-.94 1.11-.94h2.593c.55 0 1.02.398 1.11.94l.213 1.281c.063.374.313.686.645.87.074.04.147.083.22.127.324.196.72.257 1.075.124l1.217-.456a1.125 1.125 0 0 1 1.37.49l1.296 2.247a1.125 1.125 0 0 1-.26 1.431l-1.003.827c-.293.24-.438.613-.431.992a6.759 6.759 0 0 1 0 .255c-.007.378.138.75.43.99l1.005.828c.424.35.534.954.26 1.43l-1.298 2.247a1.125 1.125 0 0 1-1.369.491l-1.217-.456c-.355-.133-.75-.072-1.076.124a6.57 6.57 0 0 1-.22.128c-.331.183-.581.495-.644.869l-.213 1.281c-.09.543-.56.94-1.11.94h-2.594c-.55 0-1.02-.397-1.11-.94l-.213-1.281c-.062-.374-.312-.686-.644-.87a6.52 6.52 0 0 1-.22-.127c-.325-.196-.72-.257-1.076-.124l-1.217.456a1.125 1.125 0 0 1-1.369-.49l-1.297-2.247a1.125 1.125 0 0 1 .26-1.431l1.004-.827c.292-.24.437-.613.43-.992a6.932 6.932 0 0 1 0-.255c.007-.378-.138-.75-.43-.99l-1.004-.828a1.125 1.125 0 0 1-.26-1.43l1.297-2.247a1.125 1.125 0 0 1 1.37-.491l1.216.456c.356.133.751.072 1.076-.124.072-.044.146-.087.22-.128.332-.183.582-.495.644-.869l.214-1.28Z"/><path strokeLinecap="round" strokeLinejoin="round" d="M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z"/></svg>
            Settings
          </a>
          <a href="/domains" className="flex items-center gap-2 rounded-lg border border-zinc-200 bg-white px-3 py-1.5 text-xs text-zinc-700 transition-colors duration-150 hover:border-zinc-300 hover:bg-zinc-50 active:scale-[0.98]">
            <svg className="h-3.5 w-3.5 shrink-0 text-zinc-400" fill="none" stroke="currentColor" strokeWidth="1.5" viewBox="0 0 24 24"><path strokeLinecap="round" strokeLinejoin="round" d="M12 21a9.004 9.004 0 0 0 8.716-6.747M12 21a9.004 9.004 0 0 1-8.716-6.747M12 21c2.485 0 4.5-4.03 4.5-9S14.485 3 12 3m0 18c-2.485 0-4.5-4.03-4.5-9S9.515 3 12 3m0 0a8.997 8.997 0 0 1 7.843 4.582M12 3a8.997 8.997 0 0 0-7.843 4.582m15.686 0A11.953 11.953 0 0 1 12 10.5c-2.998 0-5.74-1.1-7.843-2.918m15.686 0A8.959 8.959 0 0 1 21 12c0 .778-.099 1.533-.284 2.253"/></svg>
            Domains
          </a>
        </div>
        <div className="border-t border-[#f0ece0] px-3 py-3">
          <div className="text-[11px] text-zinc-400">MAIL_MODE: {healthInfo?.mode || "vps"} · storage: {healthInfo?.storage || "local"} · {healthInfo?.db ? `db:${healthInfo.db}` : "db:—"}</div>
          <a
            href={`${API}/health`}
            target="_blank"
            className="text-[11px] font-medium text-[#005a5e] underline decoration-[#e8e0c8] underline-offset-2 hover:text-[#00454a]"
          >
            API health ↗ {healthInfo?.status ? `· ${healthInfo.status}` : ""}
          </a>
          <div className="mt-2 flex gap-1">
            <a href="/admin" className="flex flex-1 items-center justify-center rounded-lg border border-[#005a5e] bg-[#005a5e] px-3 py-1.5 text-center text-xs font-medium text-white hover:bg-[#00454a]">Admin</a>
            <button onClick={doLogout} className="rounded-lg border border-zinc-200 bg-white px-3 py-1.5 text-xs font-medium text-zinc-600 hover:bg-zinc-50">Logout</button>
          </div>
        </div>
      </aside>

      {/* Content — Mailflare spaced: #f8f6ef bg, main rounded-tl-3xl white — Zoho tab model */}
      <div className={`flex min-w-0 flex-1 flex-col ${isDark ? "bg-zinc-900" : "bg-[#f8f6ef]"}`}>
        <div className="flex h-9 shrink-0 items-center gap-2 border-b border-zinc-700 bg-zinc-800 px-3 text-xs text-zinc-300">
          <span className="flex items-center gap-1.5 rounded bg-[#fefcf6] px-2 py-1 text-xs font-semibold text-zinc-900"><Ico d={P.mail} size={12} /> Mail</span>
          <span className="text-zinc-500">·</span>
          <span className="hidden items-center gap-1 sm:flex"><Ico d={P.search} size={12} cls="text-zinc-500" /> Search</span>
          <input value={search} onChange={e=>setSearch(e.target.value)} placeholder="Search ( / )" className="ml-2 hidden w-48 rounded-lg bg-[#fefcf6] px-3 py-1 text-xs text-zinc-700 placeholder:text-zinc-400 focus:outline-none sm:block" />
          <div className="ml-auto flex items-center gap-1">
            <button onClick={()=>openEmbeddedTab("settings-mail","Settings")} className="flex items-center gap-1.5 rounded-lg bg-[#fefcf6]/10 px-2.5 py-1 text-[11px] font-medium text-white hover:bg-[#fefcf6]/15 border border-white/10"><Ico d={P.settings} size={11} /> Settings</button>
            <button onClick={()=>openEmbeddedTab("api-mcp","API & MCP")} className="hidden sm:flex items-center gap-1 rounded-lg bg-[#fefcf6]/10 px-2 py-1 text-[11px] text-zinc-300 hover:bg-[#fefcf6]/15 border border-white/10"><Ico d={P.key} size={11} /> API</button>
            <span className="mx-1 h-4 w-px bg-[#fefcf6]/10" />
            <button onClick={()=>openEmbeddedTab("calendar","Calendar")} className="flex items-center gap-1 rounded-lg bg-[#fefcf6] px-3 py-1 text-xs font-semibold text-zinc-900 hover:bg-zinc-100"><Ico d={P.calendar} size={11} /> Calendar</button>
            {composeOpen && <span className="ml-2 rounded bg-amber-400 px-2 py-1 text-xs font-semibold text-zinc-900">Composing…</span>}
            <div className="relative ml-2">
              <button onClick={()=> setShowAvatar(!showAvatar)} className="relative flex h-7 w-7 items-center justify-center rounded-lg bg-gradient-to-br from-[#005a5e] to-[#0a3d3f] text-white ring-2 ring-white/20 hover:ring-white/30">
                <span className="text-xs font-bold">{typeof window !== "undefined" ? (localStorage.getItem("aivory_mail_email")?.charAt(0).toUpperCase() || "A") : "A"}</span>
                <span className="absolute -bottom-0.5 -right-0.5 h-3 w-3 rounded-full bg-emerald-500 ring-2 ring-zinc-800" />
              </button>
              {showAvatar && (
                <>
                  <div className="fixed inset-0 z-40" onClick={()=> setShowAvatar(false)} />
                  <div className="absolute right-0 top-full z-50 mt-2 w-80 overflow-hidden rounded-2xl border border-[#e8e0c8] bg-white shadow-xl">
                    <div className="flex flex-col items-center border-b border-[#f0ece0] bg-[#f8f6ef] p-4">
                      <div className="relative">
                        <div className="flex h-20 w-20 items-center justify-center rounded-lg bg-gradient-to-br from-[#e8e0c8] to-[#d5c4a1] text-2xl font-bold text-[#005a5e] ring-4 ring-white shadow">
                          {typeof window !== "undefined" ? (localStorage.getItem("aivory_mail_email")?.charAt(0).toUpperCase() || "A") : "A"}
                        </div>
                        <span className="absolute bottom-1 right-1 h-4 w-4 rounded-full bg-emerald-500 ring-2 ring-white" />
                      </div>
                      <div className="mt-3 text-sm font-bold text-[#202124]">{typeof window !== "undefined" ? ((localStorage.getItem("aivory_mail_email")?.split("@")[0] || "admin").charAt(0).toUpperCase() + (localStorage.getItem("aivory_mail_email")?.split("@")[0] || "admin").slice(1)) : "Admin"}</div>
                      <div className="flex items-center gap-1 text-xs text-zinc-500">{typeof window !== "undefined" ? localStorage.getItem("aivory_mail_email") || "admin@aivory.id" : "admin@aivory.id"} <span className="cursor-pointer text-[10px]">⎘</span></div>
                      <div className="mt-1 text-xs text-zinc-400">User ID: {typeof window !== "undefined" ? String((localStorage.getItem("aivory_mail_email") || "admin@aivory.id").split("").reduce((a,c)=>a+c.charCodeAt(0),0) * 123456 % 1000000000).padStart(9,"0") : "926495579"} <span className="ml-1">ⓘ</span></div>
                      <button onClick={()=> { setShowAvatar(false); window.location.href="/settings/mail"; }} className="mt-2 text-xs font-medium text-[#005a5e] hover:underline">My Account</button>
                    </div>
                    <div className="flex gap-2 p-3">
                      <div className="flex items-center gap-1 rounded-lg border border-[#e8e0c8] bg-white px-2 py-1.5">
                        <span className="h-2 w-2 rounded-full bg-emerald-500" /> <span className="text-xs">▾</span>
                      </div>
                      <select onChange={(e)=> { localStorage.setItem("aivory_presence", e.target.value); }} defaultValue={typeof window !== "undefined" ? localStorage.getItem("aivory_presence") || "Available" : "Available"} className="w-full appearance-none rounded-lg border border-[#e8e0c8] bg-[#f8f6ef] px-3 py-1.5 text-sm">
                        <option>Available</option>
                        <option>Busy</option>
                        <option>Offline</option>
                      </select>
                    </div>
                    <div className="border-y border-[#f0ece0]">
                      <a href="/admin" onClick={()=> setShowAvatar(false)} className="flex items-center gap-3 px-4 py-3 text-sm hover:bg-[#f8f6ef]">
                        <span className="flex h-8 w-8 items-center justify-center rounded-lg bg-[#f0ece0] text-[#005a5e]">⚙</span>
                        <span className="font-medium">Admin Console</span>
                      </a>
                    </div>
                    <div className="p-4">
                      <div className="flex items-center justify-between">
                        <span className="text-sm font-semibold">Quiet Mode</span>
                        <span className="text-zinc-400">⚙</span>
                      </div>
                      <div className="mt-2 rounded-xl border border-[#e8e0c8] bg-[#f8f6ef] p-3">
                        <div className="text-xs font-medium">Pause notifications</div>
                        <select className="mt-1 w-full rounded-lg border border-[#e8e0c8] bg-white px-2 py-1.5 text-sm">
                          <option>Never</option>
                          <option>1 hour</option>
                          <option>8 hours</option>
                          <option>Until tomorrow</option>
                        </select>
                        <div className="mt-2 text-xs text-zinc-500">Quiet mode will automatically deactivate after the specified time.</div>
                      </div>
                    </div>
                    <div className="border-t border-[#f0ece0] p-4">
                      <div className="text-sm font-semibold">Subscription</div>
                      <div className="mt-1 flex items-center justify-between">
                        <span className="text-xs text-zinc-600">You are in Mail Free plan</span>
                        <button onClick={()=> { setShowAvatar(false); window.location.href="/admin"; }} className="rounded-lg border border-[#005a5e] px-3 py-1 text-xs font-medium text-[#005a5e] hover:bg-[#f8f6ef]">Upgrade</button>
                      </div>
                    </div>
                    <div className="border-t border-[#f0ece0] p-3">
                      <button onClick={()=> { setShowAvatar(false); doLogout(); }} className="flex w-full items-center justify-center gap-2 rounded-lg border border-red-100 bg-[#fefcf6] py-2.5 text-sm font-semibold text-red-600 hover:bg-red-50">
                        <span>⏻</span> SIGN OUT
                      </button>
                    </div>
                  </div>
                </>
              )}
            </div>
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
              {activeTab==="domains" && <iframe src="/domains" className="h-full w-full border-0" title="Domains" />}
            </div>
          </div>
        ) : (
        <section className={`flex min-w-0 flex-1 overflow-hidden rounded-tl-3xl shadow-sm ${isDark ? "bg-zinc-800" : "bg-[#fefcf6]"} ${isBottomPane ? "flex-col" : isNoSplit ? "flex-col" : ""}`}>
        {/* Message list — Mailflare hover #f2f6fc, active blue-50 */}
        <div className={`flex shrink-0 flex-col border-r ${isDark ? "border-zinc-700 bg-zinc-800" : "border-[#e8e0c8] bg-[#fefcf6]"} ${isBottomPane ? "w-full h-[380px] border-b border-r-0" : isNoSplit ? "w-full" : "w-[400px]"}`}>
          <div className="sticky top-0 z-10 border-b border-[#e8e0c8] bg-[#fefcf6]">
            <div className="px-3 py-2">
              <input value={search} onChange={e=>setSearch(e.target.value)} placeholder="Search messages..." className="w-full rounded-lg border border-[#e8e0c8] bg-[#f8f6ef] px-3 py-1.5 text-sm placeholder:text-zinc-400 focus:bg-[#fefcf6] focus:border-[#005a5e] focus:outline-none" />
            </div>
            <div className="flex items-center justify-between px-4 py-2 gap-2">
              <label className="flex items-center gap-2 cursor-pointer">
                <input type="checkbox" checked={conversationView && activeFolder==="Inbox" ? (threads.length>0 && selectedIds.size===threads.length) : (msgs.length>0 && selectedIds.size===msgs.length)} onChange={toggleSelectAll} className="rounded border-zinc-300 text-[#005a5e] focus:ring-[#005a5e]" />
                <span className="text-sm font-semibold text-[#202124]">
                  {conversationView && activeFolder==="Inbox" ? `${activeFolder} — ${threads.length}` : `${activeFolder} — ${msgs.length}`} {conversationView && activeFolder==="Inbox" ? "conversations" : ""}
                </span>
              </label>
              {selectedIds.size>0 ? (
                <div className="flex items-center gap-1">
                  <span className="text-xs font-medium text-[#005a5e]">{selectedIds.size} selected</span>
                  <button onClick={()=> bulkMarkRead(true)} className="rounded-lg border border-[#e8e0c8] bg-white px-2 py-1 text-[11px] hover:bg-[#f8f6ef]" title="Mark all as read">Read</button>
                  <button onClick={()=> bulkMarkRead(false)} className="rounded-lg border border-[#e8e0c8] bg-white px-2 py-1 text-[11px] hover:bg-[#f8f6ef]" title="Mark all as unread">Unread</button>
                  <button onClick={()=> bulkMove("Spam")} className="rounded-lg border border-amber-200 bg-amber-50 px-2 py-1 text-[11px] text-amber-700 hover:bg-amber-100" title="Mark as spam">Spam</button>
                  <button onClick={()=> bulkMove("Archive")} className="rounded-lg border border-[#e8e0c8] bg-white px-2 py-1 text-[11px] hover:bg-[#f8f6ef]" title="Archive">Archive</button>
                  <button onClick={bulkDelete} className="rounded-lg border border-red-200 bg-red-50 px-2 py-1 text-[11px] text-red-600 hover:bg-red-100" title="Delete">Delete</button>
                </div>
              ) : (
                <span className="rounded-lg bg-[#005a5e] px-2 py-0.5 text-[11px] font-semibold text-white">
                  {conversationView && activeFolder==="Inbox" ? `${threads.filter((t:any)=>t.has_unread).length} new` : `${msgs.filter((m) => !m.is_read).length} new`}
                </span>
              )}
            </div>
            {selectedIds.size===0 && msgs.length>0 && (
              <div className="flex items-center gap-1 px-4 pb-2">
                <button onClick={()=> bulkMarkRead(true)} className="text-[11px] text-zinc-500 hover:text-[#005a5e]">Mark all as read</button>
                <span className="text-zinc-300">·</span>
                <button onClick={()=> bulkMarkRead(false)} className="text-[11px] text-zinc-500 hover:text-[#005a5e]">Mark all as unread</button>
                <span className="text-zinc-300">·</span>
                <button onClick={bulkDelete} className="text-[11px] text-red-600 hover:text-red-700">Delete all</button>
                <button onClick={()=> bulkMove("Spam")} className="ml-auto text-[11px] text-amber-600 hover:text-amber-700">Mark as spam</button>
              </div>
            )}
          </div>

          <div className="flex-1 overflow-y-auto">
            {conversationView ? (
              <>
                {threads.length === 0 && (
                  <div className="p-8 text-center">
                    <div className="mx-auto flex h-10 w-10 items-center justify-center rounded-lg bg-[#f0ece0] text-[#005a5e]"><Ico d={P.mail} size={16} cls="text-[#005a5e]" /></div>
                    <p className="mt-3 text-sm font-medium text-[#202124]">No {activeFolder} conversations</p>
                    <p className="mt-1 text-xs text-zinc-500">{activeFolder==="Inbox" ? "Conversations appear when you have messages" : `No messages in ${activeFolder}`}</p>
                  </div>
                )}
                {threads.map((t) => (
                  <button
                    key={t.id}
                    onClick={() => openThread(t.id)}
                    className={`flex w-full flex-col gap-1 border-b border-[#f0ece0] px-4 ${rowPad} text-left transition hover:bg-[#f5efe6] hover:shadow-sm ${
                      selectedThread?.id === t.id ? "bg-[#f0ece0] border-l-2 border-l-[#005a5e]" : selectedIds.has(t.id) ? "bg-[#f0ece0]/60 border-l-2 border-l-[#005a5e]/50" : "bg-[#fefcf6] border-l-2 border-l-transparent"
                    }`}
                  >
                    <div className="flex items-center gap-2">
                      <input type="checkbox" checked={selectedIds.has(t.id)} onChange={(e)=> {e.stopPropagation(); toggleSelect(t.id);}} onClick={(e)=> e.stopPropagation()} className="h-3.5 w-3.5 rounded border-zinc-300 text-[#005a5e] focus:ring-[#005a5e]" />
                      <span className={`truncate text-[13px] ${t.has_unread ? "font-semibold text-zinc-900" : "font-normal text-zinc-700"}`}>
                        {t.subject || "(no subject)"}
                      </span>
                      {t.has_unread && <span className="h-2 w-2 shrink-0 rounded-lg bg-blue-500" />}
                      <span className="ml-auto shrink-0 rounded-lg bg-zinc-100 px-1.5 py-0.5 text-[10px] font-medium text-zinc-500">{t.message_count}</span>
                      <span className="shrink-0 text-[11px] text-zinc-400">
                        {new Date(t.last_message_at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
                      </span>
                    </div>
                  </button>
                ))}
              </>
            ) : (
              <>
                {msgs.length === 0 && (
                  <div className="p-8 text-center">
                    <div className="mx-auto flex h-10 w-10 items-center justify-center rounded-lg bg-[#f0ece0] text-[#005a5e]"><Ico d={P.mail} size={16} cls="text-[#005a5e]" /></div>
                    <p className="mt-3 text-sm font-medium text-[#202124]">No {activeFolder} messages</p>
                    <p className="mt-1 text-xs text-zinc-500">{activeFolder==="Inbox" ? "Send a test email to your mailbox" : activeFolder==="Sent" ? "Sent messages will appear here" : activeFolder==="Drafts" ? "Drafts saved via Compose → Save draft" : activeFolder==="Snoozed" ? "Snoozed messages reappear at snooze time" : `No messages in ${activeFolder}`}</p>
                  </div>
                )}
                {msgs.map((m) => (
                  <button
                    key={m.id}
                    onClick={() => open(m.id)}
                    className={`flex w-full flex-col gap-1 border-b border-[#f0ece0] px-4 ${rowPad} text-left transition hover:bg-[#f5efe6] hover:shadow-sm ${
                      selected?.id === m.id ? "bg-[#f0ece0] border-l-2 border-l-[#005a5e]" : selectedIds.has(m.id) ? "bg-[#f0ece0]/60 border-l-2 border-l-[#005a5e]/50" : "bg-[#fefcf6] border-l-2 border-l-transparent"
                    }`}
                  >
                    <div className="flex items-center gap-2">
                      <input type="checkbox" checked={selectedIds.has(m.id)} onChange={(e)=> {e.stopPropagation(); toggleSelect(m.id);}} onClick={(e)=> e.stopPropagation()} className="h-3.5 w-3.5 rounded border-zinc-300 text-[#005a5e] focus:ring-[#005a5e]" />
                      <span
                        className={`truncate text-[13px] ${m.is_read ? "font-normal text-zinc-700" : "font-semibold text-zinc-900"}`}
                      >
                        {m.from}
                      </span>
                      {!m.is_read && <span className="h-2 w-2 shrink-0 rounded-lg bg-blue-500" />}
                      <span className="ml-auto shrink-0 text-[11px] text-zinc-400">
                        {new Date(m.created_at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
                      </span>
                    </div>
                    <div className="truncate text-[13px] font-medium text-zinc-900">
                      {m.subject || "(no subject)"}
                    </div>
                    {density !== "compact" && <div className="line-clamp-2 text-xs leading-relaxed text-zinc-500">{m.snippet}</div>}
                  </button>
                ))}
              </>
            )}
          </div>
        </div>

        {/* Detail — Mailflare card style */}
        <div className={`flex min-w-0 flex-1 flex-col ${isDark ? "bg-zinc-900" : "bg-[#f8f6ef]"} ${isNoSplit ? (selected || (conversationView && selectedThread) || composeOpen ? "flex" : "hidden") : "flex"} ${isNoSplit && (selected || (conversationView && selectedThread) || composeOpen) ? "fixed inset-0 z-20 md:static" : ""}`}>
          {composeOpen ? (
            <div className="flex min-w-0 flex-1 flex-col bg-[#fefcf6] rounded-tl-3xl">
              <ComposeModal open={true} onClose={()=> { setComposeOpen(false); setReplyInfo(null); }} onSent={()=> { setComposeOpen(false); setReplyInfo(null); setSelected(null); }} defaultFrom={defaultFrom} mailboxId={mailboxes.find((m:any)=> m.address===defaultFrom)?.id} replyTo={replyInfo} inline undoSendSeconds={parseInt(general.undo_send_seconds || "10", 10)} />
            </div>
          ) : conversationView && selectedThread ? (
            <div className="flex flex-1 flex-col overflow-y-auto bg-[#f8f6ef]">
              <div className="border-b border-[#e8e0c8] bg-[#fefcf6] px-6 py-5">
                <h2 className="text-lg font-bold leading-tight text-[#202124]">{selectedThread.subject || "(no subject)"}</h2>
                <div className="mt-1 text-xs text-zinc-500">{selectedThread.messages?.length || 0} messages in this conversation</div>
              </div>
              <div className="space-y-3 p-6">
                {(selectedThread.messages || []).map((m: any) => (
                  <div key={m.id} className="rounded-xl border border-[#e8e0c8] bg-[#fefcf6] p-4 shadow-sm">
                    <div className="flex items-center justify-between text-xs text-zinc-500">
                      <span className="font-medium text-zinc-700">{m.from}</span>
                      <span>{new Date(m.created_at).toLocaleString()}</span>
                    </div>
                    <div className="mt-2 whitespace-pre-wrap text-[14px] leading-6 text-zinc-800">{m.body_text || m.snippet}</div>
                  </div>
                ))}
              </div>
              <div className="px-6 pb-6">
                <button
                  onClick={() => {
                    const last = (selectedThread.messages || [])[(selectedThread.messages || []).length - 1];
                    const threadId = selectedThread.id;
                    setSelectedThread(null);
                    if (last) openCompose({ ...last, thread_id: threadId });
                  }}
                  className="rounded-lg bg-[#005a5e] px-4 py-2 text-sm font-medium text-white shadow hover:bg-[#00454a] transition-transform duration-150 active:scale-[0.97]"
                >
                  ↩ Reply
                </button>
              </div>
            </div>
          ) : !selected ? (
            <div className="flex flex-1 flex-col overflow-y-auto bg-[#f8f6ef] p-6 space-y-6">
              <div className="flex flex-col items-center justify-center p-10 text-center">
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
              <div className="min-h-[380px]">
                <AskAIAssistant
                  selected={null}
                  threadId={undefined}
                  mailboxId={mailboxes.find((m: any) => m.address === defaultFrom)?.id || mailboxes[0]?.id}
                />
              </div>
            </div>
          ) : (
            <div className="flex flex-1 flex-col overflow-y-auto bg-[#f8f6ef]">
              <div className="border-b border-[#e8e0c8] bg-[#fefcf6] px-6 py-5">
                <h2 className="text-lg font-bold leading-tight text-[#202124]">{selected.subject}</h2>
                <div className="mt-2 flex flex-wrap items-center gap-2 text-xs text-zinc-500">
                  <span className="rounded-lg border border-[#e8e0c8] bg-[#fefcf6] px-2.5 py-1 font-medium text-zinc-700">
                    From {selected.from}
                  </span>
                  <span>{new Date(selected.created_at).toLocaleString()}</span>
                  <span className="rounded-lg bg-emerald-50 px-2 py-0.5 text-[11px] font-semibold text-emerald-700 ring-1 ring-emerald-200">
                    {selected.folder || "Inbox"}
                  </span>
                </div>
                <div className="flex flex-wrap items-center gap-1.5 border-b border-[#f0ece0] bg-[#fefcf6] px-6 py-3">
                  {msgLabels.map((l:any)=> <span key={l.id} className="inline-flex items-center gap-1 rounded-lg px-2 py-1 text-xs font-medium text-white" style={{background:l.color}}>{l.name} <button onClick={()=> detachLabel(l.id)} className="ml-1 rounded-lg bg-black/10 px-1 text-[10px] leading-none hover:bg-black/20">×</button></span>)}
                  <select onChange={(e)=> { if(e.target.value) { attachLabel(e.target.value); e.target.value=""; }}} className="rounded-lg border border-[#e8e0c8] bg-white px-3 py-1 text-xs" defaultValue="">
                    <option value="">+ Label</option>
                    {allLabels.filter((l:any)=> !msgLabels.some((m:any)=> m.id===l.id)).map((l:any)=> <option key={l.id} value={l.id}>{l.name}</option>)}
                  </select>
                  {allLabels.length===0 && <span className="text-xs text-zinc-400">No labels — create in Settings → Filters & Labels</span>}
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
                  <button onClick={()=>openCompose(selected)} className="inline-flex items-center gap-1.5 rounded-lg bg-[#005a5e] px-4 py-2 text-sm font-medium text-white shadow hover:bg-[#00454a]"><Ico d={P.reply} size={14} cls="text-white" /> Reply</button>
                  <button onClick={()=>{ setReplyInfo({ to: "", subject: `Fwd: ${selected.subject||""}`, body: selected.body_text || "" }); setComposeOpen(true);}} className="inline-flex items-center gap-1.5 rounded-lg border border-[#e8e0c8] bg-[#fefcf6] px-4 py-2 text-sm font-medium text-zinc-700 hover:bg-[#f5efe6]"><Ico d={P.forward} size={14} cls="text-zinc-500" /> Forward</button>
                  <button onClick={()=>fetch(`${API}/v1/messages/${selected.id}/move`,{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({folder:"Archive"})}).then(()=> setSelected(null))} className="inline-flex items-center gap-1.5 rounded-lg border border-[#e8e0c8] bg-[#fefcf6] px-4 py-2 text-sm font-medium text-zinc-500 hover:bg-[#f5efe6]"><Ico d={P.archive} size={14} cls="text-zinc-400" /> Archive</button>
                  <div className="relative">
                    <button onClick={()=> setShowSnooze(!showSnooze)} className="inline-flex items-center gap-1.5 rounded-lg border border-[#e8e0c8] bg-[#fefcf6] px-3 py-2 text-xs font-medium hover:bg-[#f5efe6]"><Ico d={P.snoozed} size={12} cls="text-zinc-500" /> {selected.snoozed_until ? "Snoozed" : "Snooze"}</button>
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
                  <button onClick={()=>toggleStar(selected.id)} className={`inline-flex items-center gap-1 rounded-lg border px-3 py-2 text-xs font-semibold ${selected.is_starred ? "border-amber-300 bg-amber-50 text-amber-800" : "border-[#e8e0c8] bg-[#fefcf6] text-zinc-600 hover:bg-[#f5efe6]"}`}><Ico d={P.star} size={12} cls={selected.is_starred ? "text-amber-500" : "text-zinc-400"} /> {selected.is_starred ? "Starred" : "Star"}</button>
                  <button onClick={()=>doShare(selected.id)} className="inline-flex items-center gap-1.5 rounded-lg border border-[#e8e0c8] bg-[#fefcf6] px-3 py-2 text-xs font-medium hover:bg-[#f5efe6]"><Ico d={P.link} size={12} cls="text-zinc-500" /> Share link</button>
                  <button onClick={()=>doBlock(selected.from)} className="inline-flex items-center gap-1 rounded-lg border border-red-200 bg-white px-3 py-2 text-xs font-medium text-red-600 hover:bg-red-50"><Ico d={P.block} size={12} cls="text-red-500" /> Block</button>
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
                      <span className={`rounded-lg px-2 py-0.5 text-[11px] font-semibold ${crawl.needs_follow_up ? "bg-amber-100 text-amber-800 ring-1 ring-amber-200" : "bg-zinc-100 text-zinc-600"}`}>{crawl.needs_follow_up ? "Needs follow-up" : `${crawl.days_since_last}d since last`}</span>
                    </div>
                    <div className="mt-2 space-y-1">
                      {crawl.timeline?.slice(-5).map((t:any)=> (
                        <div key={t.idx} className="flex gap-2 text-xs"><span className={`mt-1 h-2 w-2 shrink-0 rounded-lg ${t.is_outbound ? "bg-zinc-900" : "bg-blue-500"}`} /><span className="truncate"><span className="font-medium">{String(t.from?.from || t.from || "")}</span> — {String(t.snippet||"").slice(0,60)}</span><span className="ml-auto shrink-0 text-[11px] text-zinc-400">{String(t.at||"").slice(11,16)}</span></div>
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
                        <button onClick={()=> { const url = `${API}/calendar`; const bookUrl = BOOK_URL; const full = `${BOOK_URL} (or app calendar)`; navigator.clipboard?.writeText(full); setReplyInfo({to: selected.from, subject: `Re: ${selected.subject||""}`, body: `Hi,\n\nHere is my calendar to pick a time: ${bookUrl}\n\nBest`, thread_id: selected.thread_id}); setComposeOpen(true); }} className="inline-flex items-center gap-1 rounded border border-zinc-200 bg-[#fefcf6] px-2.5 py-1 text-xs hover:bg-zinc-50"><Ico d={P.calendar} size={12} cls="text-zinc-500" /> Insert calendar link</button>
                        <a href="/calendar" target="_blank" className="rounded border border-zinc-200 bg-[#fefcf6] px-2.5 py-1 text-xs hover:bg-zinc-50">Open calendar ↗</a>
                      </div>
                      <span className="text-[11px] text-zinc-400 self-center">via Aivory Calendar</span>
                    </div>
                  </div>
                )}
                <div className="rounded-xl border border-[#e8e0c8] bg-[#fefcf6] p-4">
                  <div className="flex items-center justify-between">
                    <span className="text-xs font-semibold text-[#202124]">Intelligence {intelLoading ? "• analyzing…" : intel?.ai ? "• heuristic + AI" : intel ? "• heuristic" : ""}</span>
                    {intel?.urgency && <span className={`rounded-lg px-2 py-0.5 text-[11px] font-semibold ring-1 ${intel.urgency==="High" ? "bg-red-50 text-red-700 ring-red-200" : intel.urgency==="Medium" ? "bg-amber-50 text-amber-700 ring-amber-200" : "bg-zinc-100 text-zinc-600"}`}>{intel.urgency}</span>}
                  </div>
                  {intelLoading ? (
                    <div className="mt-2 text-xs text-zinc-400">Analyzing with heuristic{intel?.ai ? " + AI gateway" : ""}…</div>
                  ) : intel ? (
                    <>
                      <div className="mt-2 flex flex-wrap gap-1.5">
                        {intel.intent && <span className="rounded-lg bg-[#005a5e] px-2.5 py-1 text-xs font-medium text-white">{intel.intent}</span>}
                        {intel.entities?.map((e:any, i:number)=> <span key={i} className="rounded-lg bg-zinc-100 px-2.5 py-1 text-xs text-zinc-700">{e.value || e.kind || e}</span>)}
                        {intel.ai?.entities?.map((e:any,i:number)=> <span key={"ai"+i} className="rounded-lg bg-[#f0ece0] px-2.5 py-1 text-xs text-[#005a5e] ring-1 ring-[#e8e0c8]">{e.value}</span>)}
                      </div>
                      {intel.summary && <div className="mt-2 text-xs leading-relaxed text-zinc-600">{intel.summary}</div>}
                      {intel.ai?.summary && <div className="mt-1 text-xs leading-relaxed text-zinc-500">AI: {intel.ai.summary}</div>}
                      {intel.suggested_actions?.length > 0 && (
                        <div className="mt-3 flex flex-wrap gap-1.5">
                          {intel.suggested_actions.map((a:any,i:number)=> (
                            <button key={i} onClick={()=>{
                              if (a.action==="create_task" || a.type==="create_task") { fetch(`${API}/v1/agent/actions`,{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({action:"create_task", entity:a})}); }
                              if (a.action==="draft_reply" || a.type==="draft_reply") { fetch(`${API}/v1/intelligence/suggest`,{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({subject: selected.subject, body: selected.body_text})}).then(r=>r.json()).then(j=>{ const draft=j.data?.draft || j.draft; if(draft){ setReplyInfo({to:selected.from, subject:`Re: ${selected.subject}`, body:draft, thread_id:selected.thread_id}); setComposeOpen(true); } }); }
                            }} className="rounded-lg border border-[#e8e0c8] bg-white px-2.5 py-1 text-[11px] font-medium hover:bg-[#f8f6ef]">{a.action || a.type || a}</button>
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
                {/* Ask AI Assistant — zeroclaw vanilla sub-agent */}
                <div className="min-h-[320px]">
                  <AskAIAssistant
                    selected={selected}
                    threadId={selected?.thread_id}
                    mailboxId={mailboxes.find((m: any) => m.address === defaultFrom)?.id || mailboxes[0]?.id}
                  />
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
