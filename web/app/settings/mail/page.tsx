"use client";
import { useEffect, useRef, useState } from "react";
import { useThemeSync } from "../../../components/themeSync";
import SignatureEditor from "../../../components/SignatureEditor";
const API = process.env.NEXT_PUBLIC_MAIL_API || "http://localhost:8095";

// The user-scoped mailbox endpoint and the admin-only webhook registry both
// require bearer authentication.
function authFetch(path: string, opts: RequestInit = {}) {
  const token = typeof window !== "undefined"
    ? (localStorage.getItem("aivory_mail_token") || sessionStorage.getItem("aivory_mail_token"))
    : null;
  const headers = new Headers(opts.headers);
  if (token) headers.set("Authorization", `Bearer ${token}`);
  return fetch(`${API}${path}`, { ...opts, headers });
}
const FALLBACK_TIMEZONES = ["Asia/Jakarta","UTC","Asia/Singapore","Asia/Tokyo","Asia/Dubai","Asia/Kolkata","Europe/London","Europe/Berlin","America/New_York","America/Los_Angeles","Australia/Sydney"];
// Intl.supportedValuesOf('timeZone') is baseline-supported in current
// Chrome/Safari/Firefox — gives the full IANA list without us maintaining
// one by hand. Falls back to a short curated list on older engines.
const TIMEZONE_OPTIONS: string[] = (() => {
  try {
    // @ts-ignore — not in older TS lib.d.ts targets
    const list = Intl.supportedValuesOf?.("timeZone");
    return Array.isArray(list) && list.length > 0 ? list : FALLBACK_TIMEZONES;
  } catch { return FALLBACK_TIMEZONES; }
})();
const TABS = [
  {id:"profile", label:"Profile"},
  {id:"general", label:"General"},
  {id:"integrations", label:"Integrations • Account • IMAP"},
  {id:"inbox", label:"Inbox"},
  {id:"signatures", label:"Signatures"},
  {id:"compose", label:"Compose"},
  {id:"filters", label:"Filters & Labels"},
  {id:"contacts", label:"Contacts"},
  {id:"webhooks", label:"Webhooks"},
  {id:"agent", label:"Agent Tasks"},
  {id:"vacation", label:"Vacation responder"},
  {id:"forwarding", label:"Forwarding & Send As"},
  {id:"appearance", label:"Appearance"},
  {id:"notifications", label:"Notifications"},
  {id:"shortcuts", label:"Shortcuts"},
  {id:"storage", label:"Storage & Offline"},
];
export default function MailSettingsPage() {
  useThemeSync();
  // Deep-link support: the inbox avatar dropdown opens
  // /settings/mail?tab=profile so Edit avatar lands on Profile directly.
  const [tab, setTab] = useState(() => {
    try {
      const q = new URLSearchParams(window.location.search).get("tab");
      return TABS.some(x => x.id === q) ? (q as string) : "general";
    } catch { return "general"; }
  });
  const [settings, setSettings] = useState<any>({});
  const [labels, setLabels] = useState<any[]>([]);
  const [filters, setFilters] = useState<any[]>([]);
  const [vac, setVac] = useState<any>({enabled:false, subject:"Out of office", body:""});
  const [newLabel, setNewLabel] = useState("");
  const [newFilter, setNewFilter] = useState("");
  const [newFilterSubject, setNewFilterSubject] = useState("");
  const [newFilterAction, setNewFilterAction] = useState("move:Spam");
  const [newFilterForward, setNewFilterForward] = useState("");
  const [newFilterPriority, setNewFilterPriority] = useState("0");
  const [contacts, setContacts] = useState<any[]>([]);
  const [csvInput, setCsvInput] = useState("");
  const [importResult, setImportResult] = useState("");
  const [webhooks, setWebhooks] = useState<any[]>([]);
  const [newWebhookUrl, setNewWebhookUrl] = useState("");
  const [newWebhookEvents, setNewWebhookEvents] = useState("email.received");
  const [webhookDeliveries, setWebhookDeliveries] = useState<Record<string, any[]>>({});
  const [agentTasks, setAgentTasks] = useState<any[]>([]);
  const [agentFilterState, setAgentFilterState] = useState("");
  const [mailboxes, setMailboxes] = useState<any[]>([]);
  const [mailboxId, setMailboxId] = useState("");
  const mailboxIdRef = useRef("");
  const [aliases, setAliases] = useState<any[]>([]);
  const [newAlias, setNewAlias] = useState("");
  const [newAliasName, setNewAliasName] = useState("");
  const [signatures, setSignatures] = useState<any[]>([]);
  const [newSigDefault, setNewSigDefault] = useState(false);
  const [editingSigId, setEditingSigId] = useState<string | null>(null);
  // Profile — display name + avatar (own mailbox, see /v1/me/profile + /v1/me/avatar)
  const [profile, setProfile] = useState<any>(null);
  const [profileLoading, setProfileLoading] = useState(false);
  const [displayName, setDisplayName] = useState("");
  const [profileMsg, setProfileMsg] = useState("");
  const [avatarPreview, setAvatarPreview] = useState<string | null>(null);
  const [avatarBusy, setAvatarBusy] = useState(false);
  const [avatarStamp, setAvatarStamp] = useState(() => Date.now());
  function mailToken(): string | null {
    if (typeof window === "undefined") return null;
    return localStorage.getItem("aivory_mail_token") || sessionStorage.getItem("aivory_mail_token");
  }
  function avatarSrc(mbId: string) {
    if (!mbId) return "";
    const token = mailToken();
    return `${API}/v1/me/avatar?mailbox_id=${encodeURIComponent(mbId)}${token ? `&token=${encodeURIComponent(token)}` : ""}&v=${avatarStamp}`;
  }
  // Integrations · Email Account (embedded, no jump)
  const [integration, setIntegration] = useState<any>(null);
  const [integLoading, setIntegLoading] = useState(false);
  const [integHost, setIntegHost] = useState("mail.aivory.uk");
  const [integPort, setIntegPort] = useState("993");
  const [integUser, setIntegUser] = useState("");
  const [integPw, setIntegPw] = useState("");
  const [integShowPw, setIntegShowPw] = useState(false);
  const [integTesting, setIntegTesting] = useState(false);
  const [integTestOk, setIntegTestOk] = useState<boolean | null>(null);
  const [integTestMsg, setIntegTestMsg] = useState("");
  const [integSaving, setIntegSaving] = useState(false);
  const [integMsg, setIntegMsg] = useState("");
  const [integShowForm, setIntegShowForm] = useState(false);
  async function loadSettings(cat:string, forMailboxId = mailboxId){
    const q = forMailboxId ? `&mailbox_id=${encodeURIComponent(forMailboxId)}` : "";
    const r=await authFetch(`/v1/settings?category=${cat}${q}`);
    if (!r.ok || mailboxIdRef.current !== forMailboxId) return;
    const j=await r.json();
    if (mailboxIdRef.current === forMailboxId) setSettings((s:any)=> ({...s, [cat]: j.data}));
  }
  async function save(cat:string, key:string, value:string){
    const body:any = {category:cat, key, value};
    if (mailboxId) body.mailbox_id = mailboxId;
    await authFetch(`/v1/settings`, {method:"POST", headers:{"content-type":"application/json"}, body: JSON.stringify(body)});
    loadSettings(cat, mailboxId);
  }
  async function loadLabels(forMailboxId = mailboxId){
    const q=forMailboxId ? `?mailbox_id=${encodeURIComponent(forMailboxId)}` : "";
    const r=await authFetch(`/v1/labels${q}`);
    if (mailboxIdRef.current !== forMailboxId) return;
    if(!r.ok){ setLabels([]); return; }
    const j=await r.json();
    if (mailboxIdRef.current === forMailboxId) setLabels(j.data||[]);
  }
  async function loadFilters(forMailboxId = mailboxId){
    const q=forMailboxId ? `?mailbox_id=${encodeURIComponent(forMailboxId)}` : "";
    const r=await authFetch(`/v1/filters${q}`);
    if (mailboxIdRef.current !== forMailboxId) return;
    if(!r.ok){ setFilters([]); return; }
    const j=await r.json();
    if (mailboxIdRef.current === forMailboxId) setFilters(j.data||[]);
  }
  async function loadContacts(forMailboxId = mailboxId){
    // Contacts are mailbox-scoped; never issue an unscoped request while the
    // mailbox selector is still resolving.
    if (!forMailboxId) { setContacts([]); return; }
    const q=`?mailbox_id=${encodeURIComponent(forMailboxId)}`;
    const r=await authFetch(`/v1/contacts${q}`);
    if (mailboxIdRef.current !== forMailboxId) return;
    if(!r.ok){ setContacts([]); return; }
    const j=await r.json();
    if (mailboxIdRef.current === forMailboxId) setContacts(j.data||[]);
  }
  async function loadWebhooks(){ const r=await authFetch("/v1/webhooks"); if(!r.ok){ setWebhooks([]); return; } const j=await r.json(); setWebhooks(j.data||[]); }
  async function loadAgentTasks(){ const url = agentFilterState ? `/v1/agent/tasks?state=${encodeURIComponent(agentFilterState)}` : "/v1/agent/tasks"; const r=await authFetch(url); if(!r.ok){ setAgentTasks([]); return; } const j=await r.json(); setAgentTasks(j.data||[]); }
  async function loadVac(mbId:string){ if(!mbId) return; const r=await authFetch(`/v1/vacation?mailbox_id=${mbId}`); const j=await r.json(); if (mailboxIdRef.current === mbId) setVac(j.data||{enabled:false}); }
  async function saveVac(next:any){
    if(!mailboxId) return;
    const body = {mailbox_id: mailboxId, enabled: next.enabled, subject: next.subject, body: next.body};
    await authFetch(`/v1/vacation`, {method:"POST", headers:{"content-type":"application/json"}, body: JSON.stringify(body)});
    setVac(next);
  }
  async function loadAliases(mbId:string){ if(!mbId) return; const r=await authFetch(`/v1/send-as?mailbox_id=${mbId}`); const j=await r.json(); if (mailboxIdRef.current === mbId) setAliases(j.data||[]); }
  async function addAlias(){
    if(!mailboxId || !newAlias.trim()) return;
    await authFetch(`/v1/send-as`, {method:"POST", headers:{"content-type":"application/json"}, body: JSON.stringify({mailbox_id: mailboxId, alias_email: newAlias.trim(), display_name: newAliasName.trim()})});
    setNewAlias(""); setNewAliasName(""); loadAliases(mailboxId);
  }
  async function removeAlias(id:string){ await authFetch(`/v1/send-as/${id}`, {method:"DELETE"}); loadAliases(mailboxId); }
  async function loadSigs(mbId:string){ if(!mbId) return; const r=await authFetch(`/v1/signatures?mailbox_id=${mbId}`); const j=await r.json(); if (mailboxIdRef.current === mbId) setSignatures(j.data||[]); }
  async function loadProfile(mbId:string){
    if(!mbId) return;
    setProfileLoading(true); setProfileMsg("");
    try{
      const r = await authFetch(`/v1/me/profile?mailbox_id=${encodeURIComponent(mbId)}`);
      const j = await r.json();
      if(j.success && j.data && mailboxIdRef.current === mbId){
        setProfile(j.data);
        setDisplayName(j.data.display_name || "");
        setAvatarPreview(null);
      }
    } catch {}
    setProfileLoading(false);
  }
  async function saveDisplayName(){
    if(!mailboxId) return;
    const name = displayName.trim();
    if(name.length > 120){ setProfileMsg("Display name maksimal 120 karakter"); return; }
    setProfileMsg("");
    try{
      const r = await authFetch(`/v1/me/profile`, {method:"PUT", headers:{"content-type":"application/json"}, body: JSON.stringify({mailbox_id: mailboxId, display_name: name})});
      const j = await r.json();
      if(!r.ok || j.success === false){ setProfileMsg("Gagal menyimpan display name"); return; }
      setProfileMsg("Display name tersimpan");
      await loadProfile(mailboxId);
    } catch { setProfileMsg("Gagal menyimpan display name"); }
  }
  async function uploadAvatarFile(file: File){
    if(!mailboxId) return;
    const okTypes = ["image/png","image/jpeg","image/gif","image/webp"];
    if(!okTypes.includes(file.type)){ setProfileMsg("Format harus PNG/JPG/GIF/WebP"); return; }
    if(file.size > 2*1024*1024){ setProfileMsg("Maksimal 2MB"); return; }
    try{
      const reader = new FileReader();
      reader.onload = () => setAvatarPreview(String(reader.result));
      reader.readAsDataURL(file);
    } catch {}
    setAvatarBusy(true); setProfileMsg("");
    try{
      const fd = new FormData();
      fd.append("avatar", file);
      const token = mailToken();
      const headers: Record<string,string> = {};
      if(token) headers["Authorization"] = `Bearer ${token}`;
      const r = await fetch(`${API}/v1/me/avatar?mailbox_id=${encodeURIComponent(mailboxId)}`, {method:"POST", headers, body: fd});
      const j = await r.json().catch(()=>null);
      if(!r.ok || !j?.success){ setProfileMsg("Upload gagal (maks 2MB, PNG/JPG/GIF/WebP)"); return; }
      setProfileMsg("Avatar diperbarui");
      setAvatarStamp(Date.now());
      await loadProfile(mailboxId);
    } catch { setProfileMsg("Upload gagal"); }
    setAvatarBusy(false);
  }
  async function removeAvatar(){
    if(!mailboxId) return;
    if(!confirm("Hapus avatar? Kembali ke inisial.")) return;
    setAvatarBusy(true);
    try{
      await authFetch(`/v1/me/avatar?mailbox_id=${encodeURIComponent(mailboxId)}`, {method:"DELETE"});
      setAvatarPreview(null);
      setAvatarStamp(Date.now());
      setProfileMsg("Avatar dihapus");
      await loadProfile(mailboxId);
    } catch { setProfileMsg("Gagal menghapus avatar"); }
    setAvatarBusy(false);
  }
  async function loadIntegration(){
    setIntegLoading(true);
    try{
      const r = await authFetch("/v1/integrations/email");
      const j = await r.json();
      if(j.success && j.data){
        setIntegration(j.data);
        const d = j.data;
        if(!d.connected){
          setIntegHost(d.host || "mail.aivory.uk");
          setIntegPort(String(d.port || 993));
          setIntegUser(d.username || d.address || "");
          setIntegShowForm(true);
        } else {
          setIntegShowForm(false);
        }
      }
    } catch {}
    setIntegLoading(false);
  }
  async function testIntegration(){
    setIntegTesting(true); setIntegTestOk(null); setIntegTestMsg("");
    try{
      const r = await authFetch("/v1/integrations/email/test",{method:"POST",headers:{"content-type":"application/json"}, body: JSON.stringify({host: integHost.trim(), port: parseInt(integPort||"993",10), username: integUser.trim(), password: integPw})});
      const j = await r.json();
      if(j.success){ setIntegTestOk(true); setIntegTestMsg(j.message || "Connection test passed"); }
      else { setIntegTestOk(false); setIntegTestMsg(j.error || "Test failed"); }
    }catch(e:any){ setIntegTestOk(false); setIntegTestMsg(e?.message || "Test failed"); }
    setIntegTesting(false);
  }
  async function saveIntegration(){
    if(integTestOk !== true){ setIntegMsg("Test connection dulu sebelum Save."); return; }
    if(integPw.length < 8){ setIntegMsg("Password minimal 8 karakter"); return; }
    setIntegSaving(true); setIntegMsg("");
    try{
      const r = await authFetch("/v1/integrations/email",{method:"POST",headers:{"content-type":"application/json"}, body: JSON.stringify({host: integHost.trim(), port: parseInt(integPort||"993",10), username: integUser.trim(), password: integPw})});
      const j = await r.json();
      if(!j.success) setIntegMsg(j.error || "Failed to save");
      else { setIntegMsg("Connected"); setIntegPw(""); setIntegTestOk(null); setIntegTestMsg(""); setIntegShowForm(false); await loadIntegration(); }
    }catch(e:any){ setIntegMsg(e?.message || "Failed to save"); }
    setIntegSaving(false);
  }
  async function disconnectIntegration(){
    if(!confirm("Disconnect email account? Password IMAP akan dihapus — webmail tetap jalan, mail client akan logout.")) return;
    try{ await authFetch("/v1/integrations/email",{method:"DELETE"}); setIntegShowForm(true); setIntegPw(""); setIntegTestOk(null); await loadIntegration(); setIntegMsg("Disconnected"); }catch{}
  }
  useEffect(()=>{
    authFetch("/v1/me/mailboxes").then(r=>r.json()).then(j=>{
      const list = j.data || [];
      setMailboxes(list);
      if (list.length === 0) return;
      // Honor an explicit ?mailbox_id= (e.g. inbox "Edit avatar" deep-link).
      try {
        const q = new URLSearchParams(window.location.search).get("mailbox_id");
        if (q && list.some((m:any)=> m.id === q)) {
          mailboxIdRef.current = q;
          setMailboxId(q);
          return;
        }
      } catch {}
      // Admins see every mailbox in the list — default to the LOGGED-IN
      // user's own mailbox (from /v1/auth/me), not list[0], so the Profile
      // tab never opens on someone else's mailbox (e.g. career@...).
      authFetch("/v1/auth/me").then(r=>r.json()).then(me=>{
        const own = me.data?.mailbox_id;
        const pick = (own && list.some((m:any)=> m.id === own)) ? own : list[0]?.id;
        if (pick) { mailboxIdRef.current = pick; setMailboxId(pick); }
      }).catch(()=>{
        const first = list[0]?.id;
        if (first) { mailboxIdRef.current = first; setMailboxId(first); }
      });
    }).catch(()=>{});
  },[]);
  useEffect(()=>{
    mailboxIdRef.current = mailboxId;
    if (!mailboxId) {
      setSettings({});
      setLabels([]);
      setFilters([]);
      setContacts([]);
      setVac({enabled:false, subject:"Out of office", body:""});
      setAliases([]);
      setSignatures([]);
      setProfile(null);
      return;
    }
    TABS.forEach(t=> loadSettings(t.id, mailboxId));
    loadLabels(mailboxId); loadFilters(mailboxId); loadContacts(mailboxId);
    loadVac(mailboxId); loadAliases(mailboxId); loadSigs(mailboxId); loadProfile(mailboxId);
    loadWebhooks(); loadAgentTasks(); loadIntegration();
  },[mailboxId]);
  function switchMailbox(id:string){
    mailboxIdRef.current = id;
    setMailboxId(id);
    setVac({enabled:false, subject:"Out of office", body:""});
    setAliases([]);
    setSignatures([]);
    setSettings({});
    setLabels([]);
    setFilters([]);
    setContacts([]);
    loadVac(id); loadAliases(id); loadSigs(id); loadProfile(id);
  }
  return (
    <div className="min-h-screen bg-[#f8f6ef] dark:bg-zinc-900 font-[Manrope]">
      <div className="mx-auto max-w-5xl p-6">
        <div className="flex items-center justify-between">
          <div className="text-sm text-zinc-500 dark:text-zinc-400"><a href="/settings" target="_top" className="underline">Settings</a> / <span className="font-semibold text-[#202124] dark:text-white">Mail</span></div>
          <a href="/settings" target="_top" className="rounded-lg border border-[#e8e0c8] dark:border-zinc-700 bg-[#fefcf6] dark:bg-zinc-800 px-3 py-1 text-xs">← API & MCP</a>
        </div>
        <h1 className="mt-2 text-3xl font-bold font-[Manrope]">Mail user settings</h1>

        {mailboxes.length >= 1 && (tab === "profile" || tab === "vacation" || tab === "forwarding" || tab === "signatures" || tab === "filters" || tab === "contacts") && (
          <div className="mt-3 flex items-center gap-2 text-xs">
            <span className="text-zinc-500 dark:text-zinc-400">Mailbox</span>
            <select value={mailboxId} onChange={(e)=> switchMailbox(e.target.value)} className="rounded border border-zinc-200 px-2 py-1 dark:bg-zinc-900 dark:text-zinc-100 dark:border-zinc-600">
              {mailboxes.map((m:any)=> <option key={m.id} value={m.id}>{m.address}</option>)}
            </select>
          </div>
        )}
        <div className="mt-6 flex gap-6">
          <nav className="hidden w-48 shrink-0 flex-col gap-1 lg:flex">
            {TABS.map(t=> (
              <button key={t.id} onClick={()=> setTab(t.id)} className={`rounded-lg px-3 py-2 text-left text-sm ${tab===t.id ? "bg-[#ff6d00] text-white" : "hover:bg-[#fefcf6] dark:hover:bg-white/10 border border-transparent hover:border-[#e8e0c8] dark:border-zinc-700"}`}>{t.label}</button>
            ))}
          </nav>
          <div className="flex-1 space-y-4">
            <div className="flex gap-2 lg:hidden overflow-x-auto pb-2">
              {TABS.map(t=> <button key={t.id} onClick={()=> setTab(t.id)} className={`whitespace-nowrap rounded-lg px-3 py-1.5 text-xs ${tab===t.id ? "bg-[#ff6d00] text-white" : "bg-[#fefcf6] dark:bg-zinc-800 border"}`}>{t.label}</button>)}
            </div>
            {tab==="profile" && (
              <div className="rounded-2xl border border-[#e8e0c8] dark:border-zinc-700 bg-[#fefcf6] dark:bg-zinc-800 p-5">
                <h3 className="font-semibold">Profile</h3>
                <p className="mt-1 text-xs text-zinc-500 dark:text-zinc-400">Nama tampilan + foto profil untuk mailbox ini. Avatar muncul di header inbox dan daftar mailbox.</p>
                {profileMsg && <div className="mt-3 rounded-xl bg-amber-50 px-4 py-2 text-sm text-amber-800 ring-1 ring-amber-200">{profileMsg} <button onClick={()=> setProfileMsg("")} className="ml-2 text-xs underline">×</button></div>}
                {profileLoading ? <div className="mt-4 p-8 text-center text-sm text-zinc-400">Loading…</div> : (
                  <div className="mt-4 flex flex-col sm:flex-row gap-5">
                    <div className="flex flex-col items-center gap-2">
                      <div className="relative">
                        {avatarPreview || profile?.has_avatar ? (
                          // eslint-disable-next-line @next/next/no-img-element
                          <img src={avatarPreview || avatarSrc(mailboxId)} alt="Avatar" className="h-24 w-24 rounded-full object-cover ring-4 ring-white shadow" />
                        ) : (
                          <div className="flex h-24 w-24 items-center justify-center rounded-full bg-gradient-to-br from-zinc-200 to-zinc-300 text-3xl font-bold text-zinc-500 ring-4 ring-white shadow">
                            {(displayName || profile?.address || profile?.email || "A").charAt(0).toUpperCase()}
                          </div>
                        )}
                        {avatarBusy && <div className="absolute inset-0 flex items-center justify-center rounded-full bg-black/40 text-xs font-semibold text-white">…</div>}
                      </div>
                      <div className="flex gap-2">
                        <label className={`cursor-pointer rounded-lg bg-zinc-900 px-3 py-1.5 text-xs font-medium text-white hover:bg-zinc-800 ${avatarBusy ? "opacity-50 pointer-events-none" : ""}`}>
                          {profile?.has_avatar ? "Ganti" : "Upload"}
                          <input type="file" accept="image/png,image/jpeg,image/gif,image/webp" className="hidden" onChange={e=> { const f = e.target.files?.[0]; if(f) uploadAvatarFile(f); e.target.value=""; }} />
                        </label>
                        {profile?.has_avatar && <button onClick={removeAvatar} disabled={avatarBusy} className="rounded-lg border border-red-200 px-3 py-1.5 text-xs font-medium text-red-600 hover:bg-red-50 disabled:opacity-50">Hapus</button>}
                      </div>
                      <span className="text-[11px] text-zinc-400">PNG/JPG/GIF/WebP · maks 2MB</span>
                    </div>
                    <div className="flex-1 grid gap-4 content-start">
                      <label className="flex flex-col gap-1 text-sm"><span className="text-xs font-medium text-zinc-600 dark:text-zinc-400">Email</span>
                        <input value={profile?.address || profile?.email || mailboxes.find((m:any)=>m.id===mailboxId)?.address || ""} readOnly disabled className="rounded border px-3 py-1.5 text-sm bg-zinc-50 text-zinc-500 dark:bg-zinc-900 dark:text-zinc-400 dark:border-zinc-600" />
                      </label>
                      <label className="flex flex-col gap-1 text-sm"><span className="text-xs font-medium text-zinc-600 dark:text-zinc-400">Display name</span>
                        <input value={displayName} onChange={e=> setDisplayName(e.target.value)} placeholder="Nama Tampilan" maxLength={120} className="rounded border px-3 py-1.5 text-sm dark:bg-zinc-900 dark:text-zinc-100 dark:border-zinc-600" />
                        <span className="text-[11px] text-zinc-400">Dipakai di header, compose From, dan signature default.</span>
                      </label>
                      <div><button onClick={saveDisplayName} disabled={!mailboxId} className="rounded-lg bg-[#ff6d00] px-4 py-1.5 text-xs font-semibold text-white hover:bg-[#e65f00] disabled:opacity-50">Simpan profile</button></div>
                    </div>
                  </div>
                )}
              </div>
            )}
            {tab==="general" && (
              <div className="rounded-2xl border border-[#e8e0c8] dark:border-zinc-700 bg-[#fefcf6] dark:bg-zinc-800 p-5">
                <h3 className="font-semibold">General</h3>
                <div className="mt-4 grid gap-4">
                  <label className="flex items-center justify-between text-sm"><span>Undo send</span>
                    <select value={settings.general?.undo_send_seconds || "10"} onChange={e=> save("general","undo_send_seconds",e.target.value)} className="rounded border px-3 py-1 text-sm dark:bg-zinc-900 dark:text-zinc-100 dark:border-zinc-600"><option value="5">5s</option><option value="10">10s</option><option value="20">20s</option><option value="30">30s</option></select>
                  </label>
                  <label className="flex items-center justify-between text-sm"><span>Density</span>
                    <select value={settings.general?.density || "comfortable"} onChange={e=> save("general","density",e.target.value)} className="rounded border px-3 py-1 text-sm dark:bg-zinc-900 dark:text-zinc-100 dark:border-zinc-600"><option value="comfortable">Comfortable</option><option value="compact">Compact</option><option value="cozy">Cozy</option></select>
                  </label>
                  <label className="flex items-center justify-between text-sm"><span>Conversation view</span>
                    <input type="checkbox" checked={(settings.general?.conversation_view||"true")==="true"} onChange={e=> save("general","conversation_view",String(e.target.checked))} />
                  </label>
                  <label className="flex items-center justify-between text-sm"><span>Max page size</span>
                    <select value={settings.general?.page_size || "20"} onChange={e=> save("general","page_size",e.target.value)} className="rounded border px-3 py-1 text-sm dark:bg-zinc-900 dark:text-zinc-100 dark:border-zinc-600"><option value="20">20</option><option value="50">50</option><option value="100">100</option></select>
                  </label>
                  <label className="flex items-center justify-between text-sm"><span>Timezone</span>
                    <select value={settings.general?.timezone || "Asia/Jakarta"} onChange={e=> save("general","timezone",e.target.value)} className="rounded border px-3 py-1 text-sm dark:bg-zinc-900 dark:text-zinc-100 dark:border-zinc-600">
                      {TIMEZONE_OPTIONS.map(tz=> <option key={tz} value={tz}>{tz.replace(/_/g," ")}</option>)}
                    </select>
                  </label>
                </div>
              </div>
            )}
            {tab==="inbox" && (
              <div className="rounded-2xl border border-[#e8e0c8] dark:border-zinc-700 bg-[#fefcf6] dark:bg-zinc-800 p-5">
                <h3 className="font-semibold">Inbox</h3>
                <div className="mt-4 grid gap-4">
                  <label className="flex items-center justify-between text-sm"><span>Inbox type</span>
                    <select value={settings.inbox?.inbox_type || "Default"} onChange={e=> save("inbox","inbox_type",e.target.value)} className="rounded border px-3 py-1 text-sm dark:bg-zinc-900 dark:text-zinc-100 dark:border-zinc-600"><option>Default</option><option>Unread first</option><option>Starred</option><option>Priority Inbox</option></select>
                  </label>
                  <label className="flex items-center justify-between text-sm"><span>Categories</span>
                    <input value={settings.inbox?.categories || "Primary,Promotions,Social"} onChange={e=> save("inbox","categories",e.target.value)} className="rounded border px-3 py-1 text-sm dark:bg-zinc-900 dark:text-zinc-100 dark:border-zinc-600" />
                  </label>
                </div>
              </div>
            )}
            {tab==="signatures" && (
              <div className="space-y-4">
                <div className="rounded-2xl border border-[#e8e0c8] dark:border-zinc-700 bg-[#fefcf6] dark:bg-zinc-800 p-5">
                  <h3 className="font-semibold">Signatures</h3>
                  <p className="text-sm text-zinc-500 dark:text-zinc-400">Multi per mailbox — like Zoho/Gmail. {mailboxId ? `For ${mailboxes.find((m:any)=>m.id===mailboxId)?.address || mailboxId}` : "Select a mailbox first."}</p>
                  {mailboxId && (
                    <>
                      <div className="mt-4 space-y-2">
                        {(() => {
                          const list = (signatures as any[]) || [];
                          if (list.length===0) return <div className="text-xs text-zinc-400 dark:text-zinc-500">No signature yet — create one below.</div>;
                          return list.map((s:any)=> (
                            <div key={s.id} className="rounded-xl border border-[#e8e0c8] dark:border-zinc-700 bg-white dark:bg-zinc-800 dark:text-zinc-100 px-3 py-2">
                              <div className="flex items-center justify-between gap-2">
                                <div className="min-w-0">
                                  <div className="text-sm font-medium truncate">{s.name} {s.is_default ? <span className="ml-2 rounded-lg bg-[#ff6d00] px-2 py-0.5 text-xs text-white">Default</span> : null}</div>
                                  {editingSigId!==s.id && <div className="text-xs text-zinc-500 dark:text-zinc-400 truncate max-w-[320px]" dangerouslySetInnerHTML={{__html: s.html?.slice(0,80) || ""}} />}
                                </div>
                                <div className="flex shrink-0 gap-1">
                                  {editingSigId!==s.id && <button onClick={()=> setEditingSigId(s.id)} className="rounded border border-[#e8e0c8] dark:border-zinc-700 px-2 py-1 text-xs hover:bg-[#f8f6ef] dark:hover:bg-white/10">Edit</button>}
                                  {!s.is_default && editingSigId!==s.id && <button onClick={async()=>{ await authFetch(`/v1/signatures/${s.id}`,{method:"PUT", headers:{"content-type":"application/json"}, body: JSON.stringify({is_default:true})}); loadSigs(mailboxId); }} className="rounded border border-[#e8e0c8] dark:border-zinc-700 px-2 py-1 text-xs hover:bg-[#f8f6ef] dark:hover:bg-white/10">Set default</button>}
                                  <button onClick={async()=>{ await authFetch(`/v1/signatures/${s.id}`,{method:"DELETE"}); if (editingSigId===s.id) setEditingSigId(null); loadSigs(mailboxId); }} className="rounded border border-red-200 px-2 py-1 text-xs text-red-600 hover:bg-red-50">Delete</button>
                                </div>
                              </div>
                              {editingSigId===s.id && (
                                <div className="mt-2 border-t border-[#f0ece0] dark:border-zinc-700 pt-2">
                                  <SignatureEditor
                                    key={s.id + (s.html || "").length}
                                    initialHtml={s.html || ""}
                                    saveLabel="Save changes"
                                    onSave={async (html, text) => {
                                      await authFetch(`/v1/signatures/${s.id}`, { method: "PUT", headers: { "content-type": "application/json" }, body: JSON.stringify({ html, text }) });
                                      setEditingSigId(null);
                                      loadSigs(mailboxId);
                                    }}
                                  />
                                  <button onClick={()=> setEditingSigId(null)} className="mt-2 text-xs text-zinc-500 hover:underline dark:text-zinc-400">Cancel</button>
                                </div>
                              )}
                            </div>
                          ));
                        })()}
                      </div>
                      <div className="mt-4 rounded-xl border border-dashed border-[#e8e0c8] dark:border-zinc-700 bg-[#f8f6ef] dark:bg-zinc-900 p-3">
                        <div className="text-xs font-semibold">Add signature</div>
                        <label className="mt-2 flex items-center gap-2 text-xs"><input type="checkbox" checked={newSigDefault} onChange={e=> setNewSigDefault(e.target.checked)} /> Set as default</label>
                        <div className="mt-2">
                          <SignatureEditor
                            key={((signatures as any[]) || []).length}
                            initialHtml=""
                            saveLabel="Save signature"
                            onSave={async (html, text) => {
                              const list = (signatures as any[]) || [];
                              await authFetch(`/v1/signatures`, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ mailbox_id: mailboxId, name: list.length ? `Signature ${list.length + 1}` : "Default", html, text, is_default: list.length === 0 || newSigDefault }) });
                              setNewSigDefault(false);
                              loadSigs(mailboxId);
                            }}
                          />
                        </div>
                      </div>
                    </>
                  )}
                  {!mailboxId && <div className="mt-3 text-xs text-amber-700">Create a mailbox first in Domains / API.</div>}
                </div>
              </div>
            )}
            {tab==="compose" && (
              <div className="rounded-2xl border border-[#e8e0c8] dark:border-zinc-700 bg-[#fefcf6] dark:bg-zinc-800 p-5">
                <h3 className="font-semibold">Compose</h3>
                <div className="mt-4 grid gap-4">
                  <label className="flex items-center justify-between text-sm"><span>Default font</span>
                    <select value={settings.compose?.default_font || "Manrope"} onChange={e=> save("compose","default_font",e.target.value)} className="rounded border px-3 py-1 text-sm dark:bg-zinc-900 dark:text-zinc-100 dark:border-zinc-600"><option>Manrope</option><option>Verdana</option><option>Arial</option></select>
                  </label>
                  <label className="flex items-center justify-between text-sm"><span>Font size</span>
                    <select value={settings.compose?.font_size || "14"} onChange={e=> save("compose","font_size",e.target.value)} className="rounded border px-3 py-1 text-sm dark:bg-zinc-900 dark:text-zinc-100 dark:border-zinc-600"><option value="12">12</option><option value="14">14</option><option value="16">16</option></select>
                  </label>
                  <label className="flex items-center justify-between text-sm"><span>Always show Cc</span><input type="checkbox" checked={(settings.compose?.always_show_cc||"false")==="true"} onChange={e=> save("compose","always_show_cc",String(e.target.checked))} /></label>
                  <label className="flex items-center justify-between text-sm"><span>Always show Bcc</span><input type="checkbox" checked={(settings.compose?.always_show_bcc||"false")==="true"} onChange={e=> save("compose","always_show_bcc",String(e.target.checked))} /></label>
                  <label className="flex items-center justify-between text-sm"><span>Outbox delay (min)</span>
                    <select value={settings.compose?.outbox_delay_minutes || "0"} onChange={e=> save("compose","outbox_delay_minutes",e.target.value)} className="rounded border px-3 py-1 text-sm dark:bg-zinc-900 dark:text-zinc-100 dark:border-zinc-600"><option value="0">0</option><option value="1">1</option><option value="2">2</option><option value="5">5</option></select>
                  </label>
                </div>
              </div>
            )}
            {tab==="filters" && (
              <div className="space-y-4">
                <div className="rounded-2xl border border-[#e8e0c8] dark:border-zinc-700 bg-[#fefcf6] dark:bg-zinc-800 p-5">
                  <h3 className="font-semibold">Filters & Labels — priority + reject/block</h3>
                  <p className="mt-1 text-xs text-zinc-500 dark:text-zinc-400">Priority kecil menang duluan (0 tertinggi). Action: Move / Reject 550 / Block (auto Spam) / Forward copy. Match "contains" case-insensitive.</p>
                  <div className="mt-3 grid grid-cols-1 md:grid-cols-2 gap-2">
                    <input value={newFilter} onChange={e=> setNewFilter(e.target.value)} placeholder="From contains e.g. spam@evil.com" className="rounded border px-3 py-1.5 text-sm dark:bg-zinc-900 dark:text-zinc-100 dark:border-zinc-600" />
                    <input value={newFilterSubject} onChange={e=> setNewFilterSubject(e.target.value)} placeholder="Subject contains (optional)" className="rounded border px-3 py-1.5 text-sm dark:bg-zinc-900 dark:text-zinc-100 dark:border-zinc-600" />
                    <select value={newFilterAction} onChange={e=> setNewFilterAction(e.target.value)} className="rounded border px-3 py-1.5 text-sm dark:bg-zinc-900 dark:text-zinc-100 dark:border-zinc-600">
                      <option value="move:Spam">Move to Spam</option>
                      <option value="move:Trash">Move to Trash</option>
                      <option value="move:Archive">Move to Archive</option>
                      <option value="move:Inbox">Move to Inbox</option>
                      <option value="reject">Reject 550</option>
                      <option value="block">Block + Spam</option>
                      <option value="forward">Forward copy</option>
                    </select>
                    <input type="number" value={newFilterPriority} onChange={e=> setNewFilterPriority(e.target.value)} placeholder="Priority 0" className="rounded border px-3 py-1.5 text-sm dark:bg-zinc-900 dark:text-zinc-100 dark:border-zinc-600" />
                  </div>
                  {newFilterAction==="forward" && (
                    <input value={newFilterForward} onChange={e=> setNewFilterForward(e.target.value)} placeholder="Forward to email" className="mt-2 w-full rounded border px-3 py-1.5 text-sm dark:bg-zinc-900 dark:text-zinc-100 dark:border-zinc-600" />
                  )}
                  <button onClick={async()=>{
                    if (!newFilter.trim() && !newFilterSubject.trim()) return;
                    const criteria:any={};
                    if (newFilter.trim()) criteria.from=newFilter.trim();
                    if (newFilterSubject.trim()) criteria.subject=newFilterSubject.trim();
                    let action:any={};
                    if (newFilterAction.startsWith("move:")) action.move=newFilterAction.split(":")[1];
                    else if (newFilterAction==="reject") action={reject:true, reason:"rejected by filter"};
                    else if (newFilterAction==="block") action={block:true};
                    else if (newFilterAction==="forward") { if(!newFilterForward.trim()) return; action.forward=newFilterForward.trim(); }
                    const prio = parseInt(newFilterPriority||"0",10)||0;
                    await authFetch(`/v1/filters`,{method:"POST",headers:{"content-type":"application/json"}, body: JSON.stringify({name:`filter prio ${prio}: ${JSON.stringify(criteria)} -> ${JSON.stringify(action)}`, criteria, action, priority:prio, mailbox_id: mailboxId})});
                    setNewFilter(""); setNewFilterSubject(""); setNewFilterForward(""); setNewFilterPriority("0"); loadFilters();
                  }} className="mt-3 rounded bg-[#ff6d00] px-4 py-1.5 text-sm font-medium text-white transition-transform duration-150 active:scale-[0.97]">Add filter (prio {newFilterPriority})</button>
                  <div className="mt-4 space-y-2">
                    {filters.map((f:any)=> (
                      <div key={f.id} className="flex items-center justify-between rounded border bg-white dark:bg-zinc-800 dark:text-zinc-100 px-3 py-2 text-sm">
                        <div className="min-w-0">
                          <div className="font-medium truncate">[{f.priority??0}] {f.name}</div>
                          <div className="text-xs text-zinc-400 dark:text-zinc-500 truncate">crit {typeof f.criteria==="string"?f.criteria:JSON.stringify(f.criteria)} → act {typeof f.action==="string"?f.action:JSON.stringify(f.action)}</div>
                        </div>
                        <div className="flex items-center gap-2">
                          <span className={`rounded-lg px-2 py-0.5 text-xs ${f.enabled?"bg-emerald-50 text-emerald-700":"bg-zinc-100 text-zinc-500 dark:text-zinc-400"}`}>{f.enabled?"enabled":"disabled"}</span>
                          <button onClick={async()=>{ await authFetch(`/v1/filters/${f.id}`,{method:"PUT", headers:{"content-type":"application/json"}, body: JSON.stringify({enabled: !f.enabled})}); loadFilters(); }} className="rounded border px-2 py-1 text-xs">{f.enabled?"Disable":"Enable"}</button>
                          <button onClick={async()=>{ await authFetch(`/v1/filters/${f.id}`,{method:"DELETE"}); loadFilters(); }} className="rounded border border-red-200 px-2 py-1 text-xs text-red-600">Delete</button>
                        </div>
                      </div>
                    ))}
                    {filters.length===0 && <div className="text-xs text-zinc-400 dark:text-zinc-500">No filters yet — add one above (from/subject → move/reject/block/forward).</div>}
                  </div>
                </div>
                <div className="rounded-2xl border border-[#e8e0c8] dark:border-zinc-700 bg-[#fefcf6] dark:bg-zinc-800 p-5">
                  <h3 className="font-semibold">Labels</h3>
                  <div className="mt-3 flex gap-2">
                    <input value={newLabel} onChange={e=> setNewLabel(e.target.value)} placeholder="Label name" className="flex-1 rounded border px-3 py-1.5 text-sm dark:bg-zinc-900 dark:text-zinc-100 dark:border-zinc-600" />
                    <button onClick={async()=>{ await authFetch(`/v1/labels`,{method:"POST",headers:{"content-type":"application/json"}, body: JSON.stringify({name:newLabel, color:"#3b82f6", mailbox_id: mailboxId})}); setNewLabel(""); loadLabels();}} className="rounded bg-[#ff6d00] px-4 py-1.5 text-sm text-white">Add label</button>
                  </div>
                  <div className="mt-3 flex flex-wrap gap-2">{labels.map((l:any)=> <span key={l.id} className="rounded-lg px-2.5 py-1 text-xs text-white" style={{background:l.color}}>{l.name}</span>)}{labels.length===0 && <span className="text-xs text-zinc-400 dark:text-zinc-500">No labels</span>}</div>
                </div>
              </div>
            )}
            {tab==="contacts" && (
              <div className="space-y-4">
                <div className="rounded-2xl border border-[#e8e0c8] dark:border-zinc-700 bg-[#fefcf6] dark:bg-zinc-800 p-5">
                  <h3 className="font-semibold">Contacts — import & blocklist</h3>
                  <p className="mt-1 text-xs text-zinc-500 dark:text-zinc-400">{contacts.length} contacts. Auto-aggregated from inbound From. Import CSV: email,display_name per line.</p>
                  <div className="mt-3 space-y-2 max-h-64 overflow-y-auto rounded border bg-white dark:bg-zinc-800 p-2 text-xs">
                    {contacts.slice(0,50).map((c:any)=> (
                      <div key={c.id} className="flex justify-between border-b border-zinc-100 py-1">
                        <span className="font-mono">{c.email}</span>
                        <span className={c.blocked?"text-red-600":"text-zinc-500 dark:text-zinc-400"}>{c.blocked?"blocked":""} {c.display_name}</span>
                      </div>
                    ))}
                    {contacts.length===0 && <div className="text-zinc-400 dark:text-zinc-500">No contacts yet</div>}
                  </div>
                  <div className="mt-3">
                    <textarea value={csvInput} onChange={e=> setCsvInput(e.target.value)} placeholder={"email,display_name\nalice@example.com,Alice\nbob@example.com,Bob"} rows={4} className="w-full rounded border px-3 py-2 text-xs font-mono dark:bg-zinc-900 dark:text-zinc-100 dark:border-zinc-600" />
                    <div className="mt-2 flex gap-2">
                      <button onClick={async()=>{ if(!csvInput.trim()) return; const r=await authFetch(`/v1/contacts/import`,{method:"POST",headers:{"content-type":"application/json"}, body: JSON.stringify({csv: csvInput, mailbox_id: mailboxId})}); const j=await r.json(); setImportResult(j.success?`Imported ${j.data?.imported||0}`: (j.error||"failed")); loadContacts(); }} className="rounded bg-[#ff6d00] px-4 py-1.5 text-xs text-white">Import CSV</button>
                      <button onClick={async()=>{ const r=await authFetch(`/v1/contacts/import`,{method:"POST",headers:{"content-type":"application/json"}, body: JSON.stringify({contacts: [{email:"demo@example.com", display_name:"Demo"}], mailbox_id: mailboxId})}); const j=await r.json(); setImportResult(`Demo: ${JSON.stringify(j.data)}`); loadContacts(); }} className="rounded border px-3 py-1.5 text-xs">Demo import</button>
                      <span className="text-xs text-zinc-500 dark:text-zinc-400 self-center">{importResult}</span>
                    </div>
                  </div>
                </div>
              </div>
            )}
            {tab==="webhooks" && (
              <div className="space-y-4">
                <div className="rounded-2xl border border-[#e8e0c8] dark:border-zinc-700 bg-[#fefcf6] dark:bg-zinc-800 p-5">
                  <h3 className="font-semibold">Webhooks — delivery & retry</h3>
                  <p className="mt-1 text-xs text-zinc-500 dark:text-zinc-400">Fire on email.received to any URL, HMAC secret optional, retry visibility per delivery.</p>
                  <div className="mt-3 flex flex-wrap gap-2">
                    <input value={newWebhookUrl} onChange={e=> setNewWebhookUrl(e.target.value)} placeholder="https://example.com/webhook" className="flex-1 rounded border px-3 py-1.5 text-sm dark:bg-zinc-900 dark:text-zinc-100 dark:border-zinc-600" />
                    <input value={newWebhookEvents} onChange={e=> setNewWebhookEvents(e.target.value)} placeholder="events csv: email.received" className="w-40 rounded border px-3 py-1.5 text-sm dark:bg-zinc-900 dark:text-zinc-100 dark:border-zinc-600" />
                    <button onClick={async()=>{ if(!newWebhookUrl.trim()) return; const evs = newWebhookEvents.split(",").map(s=>s.trim()).filter(Boolean); await authFetch("/v1/webhooks",{method:"POST",headers:{"content-type":"application/json"}, body: JSON.stringify({url:newWebhookUrl.trim(), events:evs})}); setNewWebhookUrl(""); loadWebhooks(); }} className="rounded bg-[#ff6d00] px-4 py-1.5 text-sm text-white">Add webhook</button>
                  </div>
                  <div className="mt-3 space-y-2">
                    {webhooks.map((w:any)=> (
                      <div key={w.id} className="rounded border bg-white dark:bg-zinc-800 p-3 text-sm">
                        <div className="flex justify-between">
                          <span className="font-mono text-xs truncate">{w.url}</span>
                          <button onClick={async()=>{ await authFetch(`/v1/webhooks/${w.id}`,{method:"DELETE"}); loadWebhooks(); }} className="text-xs text-red-600">Delete</button>
                        </div>
                        <div className="text-xs text-zinc-400 dark:text-zinc-500">events: {JSON.stringify(w.events)} • {w.enabled?"enabled":"disabled"}</div>
                        <button onClick={async()=>{
                          const r=await authFetch(`/v1/webhooks/${w.id}/deliveries`); const j=await r.json();
                          setWebhookDeliveries(prev=> ({...prev, [w.id]: j.data||[]}));
                        }} className="mt-1 rounded border px-2 py-1 text-xs">View deliveries ({webhookDeliveries[w.id]?.length||0})</button>
                        {webhookDeliveries[w.id] && (
                          <div className="mt-2 space-y-1 max-h-40 overflow-y-auto">
                            {webhookDeliveries[w.id].slice(0,10).map((d:any)=> (
                              <div key={d.id} className="flex justify-between rounded bg-zinc-50 px-2 py-1 text-xs">
                                <span>{d.event} • {d.status} • {d.attempts} attempts</span>
                                {d.status==="failed" && <button onClick={async()=>{ await authFetch(`/v1/webhooks/${w.id}/retry`,{method:"POST",headers:{"content-type":"application/json"}, body: JSON.stringify({delivery_id:d.id})}); }} className="text-xs text-amber-600">Retry</button>}
                              </div>
                            ))}
                          </div>
                        )}
                      </div>
                    ))}
                    {webhooks.length===0 && <div className="text-xs text-zinc-400 dark:text-zinc-500">No webhooks yet</div>}
                  </div>
                </div>
              </div>
            )}
            {tab==="agent" && (
              <div className="space-y-4">
                <div className="rounded-2xl border border-[#e8e0c8] dark:border-zinc-700 bg-[#fefcf6] dark:bg-zinc-800 p-5">
                  <h3 className="font-semibold">Agent Tasks — inbox by state</h3>
                  <p className="mt-1 text-xs text-zinc-500 dark:text-zinc-400">States: needs_reply / waiting_on_me / waiting_on_them / fyi / auto_handled / needs_approval — human-approved actions.</p>
                  <div className="mt-3 flex gap-2">
                    <select value={agentFilterState} onChange={e=> setAgentFilterState(e.target.value)} className="rounded border px-3 py-1.5 text-sm dark:bg-zinc-900 dark:text-zinc-100 dark:border-zinc-600">
                      <option value="">All states</option>
                      <option value="needs_reply">needs_reply</option>
                      <option value="waiting_on_me">waiting_on_me</option>
                      <option value="waiting_on_them">waiting_on_them</option>
                      <option value="fyi">fyi</option>
                      <option value="auto_handled">auto_handled</option>
                      <option value="needs_approval">needs_approval</option>
                    </select>
                    <button onClick={loadAgentTasks} className="rounded border px-3 py-1.5 text-sm dark:bg-zinc-900 dark:text-zinc-100 dark:border-zinc-600">Filter</button>
                    <button onClick={async()=>{ await authFetch(`/v1/agent/tasks`,{method:"POST",headers:{"content-type":"application/json"}, body: JSON.stringify({type:"triage", state:"needs_reply", title:"Demo task "+Date.now(), body:"Follow up demo"})}); loadAgentTasks(); }} className="rounded bg-[#ff6d00] px-4 py-1.5 text-sm text-white">Create demo task</button>
                  </div>
                  <div className="mt-3 space-y-2 max-h-80 overflow-y-auto">
                    {agentTasks.map((t:any)=> (
                      <div key={t.id} className="rounded border bg-white dark:bg-zinc-800 p-3 text-sm">
                        <div className="flex justify-between">
                          <span className="font-medium">{t.title}</span>
                          <span className={`rounded-lg px-2 py-0.5 text-xs ${t.state==="needs_reply"?"bg-amber-50 text-amber-700": t.state==="needs_approval"?"bg-red-50 text-red-700":"bg-zinc-100 text-zinc-600 dark:text-zinc-400"}`}>{t.state}</span>
                        </div>
                        <div className="text-xs text-zinc-500 dark:text-zinc-400 truncate">{t.body}</div>
                        <div className="mt-1 flex gap-1">
                          <select defaultValue={t.state} onChange={async(e)=>{ await authFetch(`/v1/agent/tasks/${t.id}`,{method:"PUT", headers:{"content-type":"application/json"}, body: JSON.stringify({state: e.target.value})}); loadAgentTasks(); }} className="rounded border px-2 py-1 text-xs">
                            <option value="needs_reply">needs_reply</option>
                            <option value="waiting_on_me">waiting_on_me</option>
                            <option value="waiting_on_them">waiting_on_them</option>
                            <option value="fyi">fyi</option>
                            <option value="auto_handled">auto_handled</option>
                            <option value="needs_approval">needs_approval</option>
                            <option value="done">done</option>
                          </select>
                          <span className="text-xs text-zinc-400 dark:text-zinc-500">{t.type} • {new Date(t.created_at).toLocaleString()}</span>
                        </div>
                      </div>
                    ))}
                    {agentTasks.length===0 && <div className="text-xs text-zinc-400 dark:text-zinc-500">No agent tasks — create demo or trigger via AI intelligence.</div>}
                  </div>
                </div>
              </div>
            )}
            {tab==="vacation" && (
              <div className="rounded-2xl border border-[#e8e0c8] dark:border-zinc-700 bg-[#fefcf6] dark:bg-zinc-800 p-5">
                <h3 className="font-semibold">Vacation responder</h3>
                <p className="mt-1 text-xs text-zinc-500 dark:text-zinc-400">{mailboxId ? `For ${mailboxes.find((m:any)=>m.id===mailboxId)?.address || mailboxId}` : "No mailbox yet — create one first."} — auto-replies once per sender per day while enabled.</p>
                <div className="mt-4 grid gap-4">
                  <label className="flex items-center justify-between text-sm"><span>Enabled</span>
                    <input type="checkbox" checked={!!vac.enabled} onChange={e=> saveVac({...vac, enabled: e.target.checked})} disabled={!mailboxId} />
                  </label>
                  <label className="flex flex-col gap-1 text-sm"><span className="text-zinc-500 dark:text-zinc-400">Subject</span>
                    <input value={vac.subject||""} onChange={e=> setVac({...vac, subject: e.target.value})} onBlur={()=> saveVac(vac)} disabled={!mailboxId} className="rounded border px-3 py-1.5 text-sm dark:bg-zinc-900 dark:text-zinc-100 dark:border-zinc-600 disabled:bg-zinc-50" />
                  </label>
                  <label className="flex flex-col gap-1 text-sm"><span className="text-zinc-500 dark:text-zinc-400">Message</span>
                    <textarea value={vac.body||""} onChange={e=> setVac({...vac, body: e.target.value})} onBlur={()=> saveVac(vac)} disabled={!mailboxId} rows={4} className="rounded border px-3 py-1.5 text-sm dark:bg-zinc-900 dark:text-zinc-100 dark:border-zinc-600 disabled:bg-zinc-50" />
                  </label>
                </div>
              </div>
            )}
            {tab==="forwarding" && (
              <div className="space-y-4">
                <div className="rounded-2xl border border-[#e8e0c8] dark:border-zinc-700 bg-[#fefcf6] dark:bg-zinc-800 p-5">
                  <h3 className="font-semibold">Forwarding & POP/IMAP</h3>
                  <div className="mt-4 grid gap-4">
                    <label className="flex items-center justify-between text-sm"><span>Forward to</span>
                      <input value={settings.forwarding?.forward_to || ""} onChange={e=> save("forwarding","forward_to",e.target.value)} placeholder="forward@aivory.uk" className="rounded border px-3 py-1 text-sm dark:bg-zinc-900 dark:text-zinc-100 dark:border-zinc-600" />
                    </label>
                    <label className="flex items-center justify-between text-sm"><span>Keep copy</span><input type="checkbox" checked={(settings.forwarding?.keep_copy||"true")==="true"} onChange={e=> save("forwarding","keep_copy",String(e.target.checked))} /></label>
                    <label className="flex items-center justify-between text-sm"><span>POP enabled</span><input type="checkbox" checked={(settings.forwarding?.pop_enabled||"false")==="true"} onChange={e=> save("forwarding","pop_enabled",String(e.target.checked))} /></label>
                    <label className="flex items-center justify-between text-sm"><span>IMAP enabled</span><input type="checkbox" checked={(settings.forwarding?.imap_enabled||"true")==="true"} onChange={e=> save("forwarding","imap_enabled",String(e.target.checked))} /></label>
                  </div>
                </div>
                <div className="rounded-2xl border border-[#e8e0c8] dark:border-zinc-700 bg-[#fefcf6] dark:bg-zinc-800 p-5">
                  <h3 className="font-semibold">Send As</h3>
                  <p className="mt-1 text-xs text-zinc-500 dark:text-zinc-400">{mailboxId ? `Aliases for ${mailboxes.find((m:any)=>m.id===mailboxId)?.address || mailboxId}` : "No mailbox yet — create one first."} Appears in the compose From dropdown. Sending still requires the alias's domain to be verified.</p>
                  <div className="mt-3 flex flex-wrap gap-2">
                    <input value={newAliasName} onChange={e=> setNewAliasName(e.target.value)} placeholder="Display name (optional)" className="w-40 rounded border px-3 py-1.5 text-sm dark:bg-zinc-900 dark:text-zinc-100 dark:border-zinc-600" disabled={!mailboxId} />
                    <input value={newAlias} onChange={e=> setNewAlias(e.target.value)} placeholder="alias@yourdomain.com" className="flex-1 rounded border px-3 py-1.5 text-sm dark:bg-zinc-900 dark:text-zinc-100 dark:border-zinc-600" disabled={!mailboxId} />
                    <button onClick={addAlias} disabled={!mailboxId} className="rounded bg-zinc-900 px-4 py-1.5 text-sm font-medium text-white transition-transform duration-150 active:scale-[0.97] disabled:opacity-50">Add alias</button>
                  </div>
                  <div className="mt-3 space-y-2">
                    {aliases.map((a:any)=> (
                      <div key={a.id} className="flex items-center justify-between rounded border px-3 py-1.5 text-sm dark:bg-zinc-900 dark:text-zinc-100 dark:border-zinc-600">
                        <span>{a.display_name ? `${a.display_name} <${a.alias_email}>` : a.alias_email}{a.is_default && <span className="ml-2 rounded-lg bg-zinc-100 px-2 py-0.5 text-xs text-zinc-500 dark:text-zinc-400">Default</span>}</span>
                        <button onClick={()=> removeAlias(a.id)} className="text-xs text-zinc-400 dark:text-zinc-500 hover:text-red-600">Remove</button>
                      </div>
                    ))}
                    {aliases.length===0 && <div className="text-xs text-zinc-400 dark:text-zinc-500">No aliases yet</div>}
                  </div>
                </div>
              </div>
            )}
            {tab==="appearance" && (
              <div className="rounded-2xl border border-[#e8e0c8] dark:border-zinc-700 bg-[#fefcf6] dark:bg-zinc-800 p-5">
                <h3 className="font-semibold">Appearance</h3>
                <div className="mt-4 grid gap-4">
                  <label className="flex items-center justify-between text-sm"><span>Theme</span>
                    <select value={settings.appearance?.theme || "dark"} onChange={e=> save("appearance","theme",e.target.value)} className="rounded border px-3 py-1 text-sm dark:bg-zinc-900 dark:text-zinc-100 dark:border-zinc-600"><option value="light">Light</option><option value="dark">Dark</option></select>
                  </label>
                  <label className="flex items-center justify-between text-sm"><span>Reading pane</span>
                    <select value={settings.appearance?.reading_pane || "right"} onChange={e=> save("appearance","reading_pane",e.target.value)} className="rounded border px-3 py-1 text-sm dark:bg-zinc-900 dark:text-zinc-100 dark:border-zinc-600"><option value="right">Right</option><option value="bottom">Bottom</option><option value="no-split">No split</option></select>
                  </label>
                </div>
              </div>
            )}
            {tab==="notifications" && (
              <div className="rounded-2xl border border-[#e8e0c8] dark:border-zinc-700 bg-[#fefcf6] dark:bg-zinc-800 p-5">
                <h3 className="font-semibold">Notifications</h3>
                <div className="mt-4 grid gap-4">
                  <label className="flex items-center justify-between text-sm"><span>Desktop sound</span><input type="checkbox" checked={(settings.notifications?.desktop_sound||"true")==="true"} onChange={e=> save("notifications","desktop_sound",String(e.target.checked))} /></label>
                  <label className="flex items-center justify-between text-sm"><span>New mail banner</span><input type="checkbox" checked={(settings.notifications?.new_mail_banner||"true")==="true"} onChange={e=> save("notifications","new_mail_banner",String(e.target.checked))} /></label>
                </div>
              </div>
            )}
            {tab==="shortcuts" && (
              <div className="rounded-2xl border border-[#e8e0c8] dark:border-zinc-700 bg-[#fefcf6] dark:bg-zinc-800 p-5">
                <h3 className="font-semibold">Keyboard shortcuts</h3>
                <label className="flex items-center justify-between text-sm"><span>Enable shortcuts</span><input type="checkbox" checked={(settings.shortcuts?.enabled||"true")==="true"} onChange={e=> save("shortcuts","enabled",String(e.target.checked))} /></label>
                <div className="mt-3 text-xs text-zinc-500 dark:text-zinc-400">c compose, e archive, r reply, / search.</div>
              </div>
            )}
            {tab==="storage" && (
              <div className="rounded-2xl border border-[#e8e0c8] dark:border-zinc-700 bg-[#fefcf6] dark:bg-zinc-800 p-5">
                <h3 className="font-semibold">Storage & Offline</h3>
                <div className="mt-4 grid gap-4">
                  <label className="flex items-center justify-between text-sm"><span>Days to sync</span>
                    <select value={settings.storage?.days_to_sync || "30"} onChange={e=> save("storage","days_to_sync",e.target.value)} className="rounded border px-3 py-1 text-sm dark:bg-zinc-900 dark:text-zinc-100 dark:border-zinc-600"><option value="7">7</option><option value="30">30</option><option value="90">90</option></select>
                  </label>
                  <label className="flex items-center justify-between text-sm"><span>Download on WiFi only</span><input type="checkbox" checked={(settings.storage?.download_attachments_wifi_only||"true")==="true"} onChange={e=> save("storage","download_attachments_wifi_only",String(e.target.checked))} /></label>
                </div>
              </div>
            )}
            {tab==="integrations" && (
              <div className="space-y-4">
                {integMsg && <div className="rounded-xl bg-amber-50 px-4 py-2 text-sm text-amber-800 ring-1 ring-amber-200">{integMsg} <button onClick={()=> setIntegMsg("")} className="ml-2 text-xs underline">×</button></div>}
                {integLoading ? <div className="rounded-2xl border border-[#e8e0c8] dark:border-zinc-700 bg-white dark:bg-zinc-800 p-8 text-center text-sm text-zinc-400 dark:text-zinc-500">Loading…</div>
                : integration?.connected && !integShowForm ? (
                  <div className="rounded-2xl border border-emerald-200 bg-white dark:bg-zinc-800 p-5 shadow-sm">
                    <div className="flex items-start justify-between gap-3">
                      <div>
                        <div className="flex items-center gap-2">
                          <span className="h-2.5 w-2.5 rounded-full bg-emerald-500 animate-pulse" />
                          <h3 className="font-semibold text-[#202124] dark:text-white">Connected</h3>
                          <span className="rounded-lg bg-emerald-50 px-2 py-0.5 text-xs font-medium text-emerald-700">IMAP ready</span>
                        </div>
                        <p className="mt-1 text-sm text-zinc-600 dark:text-zinc-400">Connected as <span className="font-mono font-semibold text-[#202124] dark:text-white">{integration.username || integration.address}</span></p>
                        <p className="mt-1 text-xs text-zinc-500 dark:text-zinc-400 font-mono">{integration.host}:{integration.port} · IMAP 993 SSL · SMTP 587 STARTTLS · username = full address</p>
                        {integration.updated_at && <p className="mt-1 text-xs text-zinc-400 dark:text-zinc-500">Last updated {new Date(integration.updated_at).toLocaleString()}</p>}
                      </div>
                      <div className="flex gap-2">
                        <button onClick={()=> setIntegShowForm(true)} className="rounded-lg border border-[#e8e0c8] dark:border-zinc-700 bg-[#fefcf6] dark:bg-zinc-800 px-4 py-1.5 text-xs font-medium hover:bg-[#f8f6ef] dark:hover:bg-white/10">Reconnect</button>
                        <button onClick={disconnectIntegration} className="rounded-lg border border-red-200 bg-white dark:bg-zinc-800 px-4 py-1.5 text-xs font-medium text-red-600 hover:bg-red-50">Disconnect</button>
                      </div>
                    </div>
                    <div className="mt-4 rounded-xl bg-[#f8f6ef] dark:bg-zinc-900 px-3 py-2 text-xs text-zinc-500 dark:text-zinc-400">Password tidak pernah ditampilkan lagi setelah save — seperti App Passwords. Jika lupa, gunakan Reconnect. Admin bisa cek status di Admin Console.</div>
                  </div>
                ) : (
                  <div className="rounded-2xl border border-[#e8e0c8] dark:border-zinc-700 bg-[#fefcf6] dark:bg-zinc-800 p-5 shadow-sm">
                    <h3 className="font-semibold text-[#202124] dark:text-white">Integrations • Account • IMAP</h3>
                    <p className="mt-1 text-xs text-zinc-500 dark:text-zinc-400">Sub-section terpisah dari profile — host/port/username/password. Test dulu sebelum Save. Setelah tersimpan, hanya status Connected yang tampil.</p>
                    <div className="mt-4 grid gap-4">
                      <div className="grid md:grid-cols-3 gap-3">
                        <label className="flex flex-col gap-1 text-sm"><span className="text-xs font-medium text-zinc-600 dark:text-zinc-400">IMAP host</span><input value={integHost} onChange={e=>{setIntegHost(e.target.value); setIntegTestOk(null);}} placeholder="mail.aivory.uk" className="rounded-lg border border-[#e8e0c8] dark:border-zinc-700 bg-white dark:bg-zinc-800 dark:text-zinc-100 px-3 py-2 text-sm font-mono focus:border-[#ff6d00] focus:outline-none" /></label>
                        <label className="flex flex-col gap-1 text-sm"><span className="text-xs font-medium text-zinc-600 dark:text-zinc-400">Port</span><input value={integPort} onChange={e=>{setIntegPort(e.target.value); setIntegTestOk(null);}} placeholder="993" inputMode="numeric" className="rounded-lg border border-[#e8e0c8] dark:border-zinc-700 bg-white dark:bg-zinc-800 dark:text-zinc-100 px-3 py-2 text-sm font-mono focus:border-[#ff6d00] focus:outline-none" /></label>
                        <label className="flex flex-col gap-1 text-sm"><span className="text-xs font-medium text-zinc-600 dark:text-zinc-400">Username</span><input value={integUser} onChange={e=>{setIntegUser(e.target.value); setIntegTestOk(null);}} placeholder="you@domain.com" className="rounded-lg border border-[#e8e0c8] dark:border-zinc-700 bg-white dark:bg-zinc-800 dark:text-zinc-100 px-3 py-2 text-sm font-mono focus:border-[#ff6d00] focus:outline-none" /></label>
                      </div>
                      <label className="flex flex-col gap-1 text-sm"><span className="text-xs font-medium text-zinc-600 dark:text-zinc-400">Password</span>
                        <div className="flex gap-2">
                          <div className="relative flex-1">
                            <input type={integShowPw ? "text":"password"} value={integPw} onChange={e=>{setIntegPw(e.target.value); setIntegTestOk(null); setIntegTestMsg("");}} placeholder="IMAP password (min 8 chars)" className="w-full rounded-lg border border-[#e8e0c8] dark:border-zinc-700 bg-white dark:bg-zinc-800 dark:text-zinc-100 px-3 py-2 pr-10 text-sm font-mono focus:border-[#ff6d00] focus:outline-none" />
                            <button type="button" onClick={()=> setIntegShowPw(v=>!v)} className="absolute right-2 top-1/2 -translate-y-1/2 rounded p-1 text-zinc-400 dark:text-zinc-500 hover:bg-zinc-100">{integShowPw ? "Hide" : "Show"}</button>
                          </div>
                          <button type="button" onClick={()=> setIntegShowPw(v=>!v)} className="rounded-lg border border-[#e8e0c8] dark:border-zinc-700 bg-white dark:bg-zinc-800 dark:text-zinc-100 px-3 py-2 text-xs hover:bg-[#f8f6ef] dark:hover:bg-white/10">{integShowPw ? "Hide":"Show"}</button>
                        </div>
                        <span className="text-xs text-zinc-400 dark:text-zinc-500">Terpisah dari password web login — App-passwords parity.</span>
                      </label>
                      <div className="flex flex-wrap items-center gap-2">
                        <button onClick={testIntegration} disabled={integTesting || !integHost || !integUser || !integPw} className="rounded-lg border border-[#005a5e] bg-white dark:bg-zinc-800 px-4 py-2 text-sm font-medium text-[#005a5e] hover:bg-[#f0f7f7] disabled:opacity-50">{integTesting ? "Testing…":"Test connection"}</button>
                        {integTestOk===true && <span className="text-xs font-medium text-emerald-700">✓ {integTestMsg}</span>}
                        {integTestOk===false && <span className="text-xs font-medium text-red-600">✗ {integTestMsg}</span>}
                        {integTestOk===null && <span className="text-xs text-zinc-400 dark:text-zinc-500">Wajib test sebelum Save</span>}
                      </div>
                      <div className="flex justify-end gap-2 pt-2 border-t border-[#f0ece0] dark:border-zinc-700">
                        {integration?.connected && <button onClick={()=> {setIntegShowForm(false); setIntegTestOk(null);}} className="rounded-lg border border-[#e8e0c8] dark:border-zinc-700 bg-white dark:bg-zinc-800 px-4 py-2 text-sm hover:bg-[#f8f6ef] dark:hover:bg-white/10">Cancel</button>}
                        <button onClick={saveIntegration} disabled={integSaving || integTestOk!==true} className="rounded-lg bg-[#005a5e] px-6 py-2 text-sm font-semibold text-white hover:bg-[#00454a] disabled:opacity-40">{integSaving ? "Saving…":"Save & connect"}</button>
                      </div>
                    </div>
                  </div>
                )}
                <div className="rounded-xl border border-dashed border-[#e8e0c8] dark:border-zinc-700 bg-white dark:bg-zinc-800 p-4 text-xs text-zinc-500 dark:text-zinc-400">
                  <div className="font-medium text-zinc-600 dark:text-zinc-400">Untuk mail client (Thunderbird/Apple Mail/Outlook)</div>
                  <div className="mt-1 font-mono">IMAP: {integHost || "mail.aivory.uk"}:993 SSL · SMTP: 587 STARTTLS · username = full address</div>
                  <div className="mt-1">Kredensial dipakai Dovecot 993 & submission 587. Disconnect = clear password (web login tetap jalan).</div>
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
