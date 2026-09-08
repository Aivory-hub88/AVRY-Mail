"use client";
import { useEffect, useState } from "react";
const API = process.env.NEXT_PUBLIC_MAIL_API || "http://localhost:8095";

// /v1/mailboxes and the /v1/webhooks registry are gated behind domain-admin
// auth (see authz.rs) — neither was ever sent a bearer token from this page,
// so the mailbox picker silently came back empty, which cascaded into
// Vacation/Aliases/Signatures never loading either (they all key off the
// first mailbox id from that now-401'd fetch).
function authFetch(path: string, opts: RequestInit = {}) {
  const token = typeof window !== "undefined" ? localStorage.getItem("aivory_mail_token") : null;
  const headers: Record<string, string> = { ...(opts.headers as Record<string, string> | undefined) };
  if (token) headers["Authorization"] = `Bearer ${token}`;
  return fetch(`${API}${path}`, { ...opts, headers });
}
const TABS = [
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
  const [tab, setTab] = useState("general");
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
  const [aliases, setAliases] = useState<any[]>([]);
  const [newAlias, setNewAlias] = useState("");
  const [newAliasName, setNewAliasName] = useState("");
  const [signatures, setSignatures] = useState<any[]>([]);
  const [newSigName, setNewSigName] = useState("");
  const [newSigHtml, setNewSigHtml] = useState("");
  const [newSigDefault, setNewSigDefault] = useState(false);
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
  async function loadSettings(cat:string){
    const r=await fetch(`${API}/v1/settings?category=${cat}`);
    const j=await r.json();
    setSettings((s:any)=> ({...s, [cat]: j.data}));
  }
  async function save(cat:string, key:string, value:string){
    await fetch(`${API}/v1/settings`, {method:"POST", headers:{"content-type":"application/json"}, body: JSON.stringify({category:cat, key, value})});
    loadSettings(cat);
  }
  async function loadLabels(){ const r=await fetch(`${API}/v1/labels`); const j=await r.json(); setLabels(j.data||[]); }
  async function loadFilters(){ const r=await fetch(`${API}/v1/filters`); const j=await r.json(); setFilters(j.data||[]); }
  async function loadContacts(){ const r=await fetch(`${API}/v1/contacts`); const j=await r.json(); setContacts(j.data||[]); }
  async function loadWebhooks(){ const r=await authFetch("/v1/webhooks"); const j=await r.json(); setWebhooks(j.data||[]); }
  async function loadAgentTasks(){ const url = agentFilterState ? `${API}/v1/agent/tasks?state=${agentFilterState}` : `${API}/v1/agent/tasks`; const r=await fetch(url); const j=await r.json(); setAgentTasks(j.data||[]); }
  async function loadVac(mbId:string){ if(!mbId) return; const r=await fetch(`${API}/v1/vacation?mailbox_id=${mbId}`); const j=await r.json(); setVac(j.data||{enabled:false}); }
  async function saveVac(next:any){
    if(!mailboxId) return;
    const body = {mailbox_id: mailboxId, enabled: next.enabled, subject: next.subject, body: next.body};
    await fetch(`${API}/v1/vacation`, {method:"POST", headers:{"content-type":"application/json"}, body: JSON.stringify(body)});
    setVac(next);
  }
  async function loadAliases(mbId:string){ if(!mbId) return; const r=await fetch(`${API}/v1/send-as?mailbox_id=${mbId}`); const j=await r.json(); setAliases(j.data||[]); }
  async function addAlias(){
    if(!mailboxId || !newAlias.trim()) return;
    await fetch(`${API}/v1/send-as`, {method:"POST", headers:{"content-type":"application/json"}, body: JSON.stringify({mailbox_id: mailboxId, alias_email: newAlias.trim(), display_name: newAliasName.trim()})});
    setNewAlias(""); setNewAliasName(""); loadAliases(mailboxId);
  }
  async function removeAlias(id:string){ await fetch(`${API}/v1/send-as/${id}`, {method:"DELETE"}); loadAliases(mailboxId); }
  async function loadSigs(mbId:string){ if(!mbId) return; const r=await fetch(`${API}/v1/signatures?mailbox_id=${mbId}`); const j=await r.json(); setSignatures(j.data||[]); }
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
    TABS.forEach(t=> loadSettings(t.id)); loadLabels(); loadFilters(); loadContacts(); loadWebhooks(); loadAgentTasks(); loadIntegration();
    authFetch("/v1/mailboxes").then(r=>r.json()).then(j=>{
      const list = j.data || [];
      setMailboxes(list);
      const first = list[0]?.id;
      if (first) { setMailboxId(first); loadVac(first); loadAliases(first); loadSigs(first); }
    }).catch(()=>{});
  },[]);
  function switchMailbox(id:string){ setMailboxId(id); loadVac(id); loadAliases(id); loadSigs(id); }
  return (
    <div className="min-h-screen bg-[#f8f6ef] font-[Manrope]">
      <div className="mx-auto max-w-5xl p-6">
        <div className="flex items-center justify-between">
          <div className="text-sm text-zinc-500"><a href="/settings" target="_top" className="underline">Settings</a> / <span className="font-semibold text-[#202124]">Mail</span></div>
          <a href="/settings" target="_top" className="rounded-lg border border-[#e8e0c8] bg-[#fefcf6] px-3 py-1 text-xs">← API & MCP</a>
        </div>
        <h1 className="mt-2 text-3xl font-bold font-[Manrope]">Mail user settings</h1>
        <p className="mt-1 text-sm text-zinc-500">Gmail / Zoho / Outlook parity — Manrope throughout</p>

        {mailboxes.length >= 1 && (tab === "vacation" || tab === "forwarding" || tab === "signatures") && (
          <div className="mt-3 flex items-center gap-2 text-xs">
            <span className="text-zinc-500">Mailbox</span>
            <select value={mailboxId} onChange={(e)=> switchMailbox(e.target.value)} className="rounded border border-zinc-200 px-2 py-1">
              {mailboxes.map((m:any)=> <option key={m.id} value={m.id}>{m.address}</option>)}
            </select>
          </div>
        )}
        <div className="mt-6 flex gap-6">
          <nav className="hidden w-48 shrink-0 flex-col gap-1 lg:flex">
            {TABS.map(t=> (
              <button key={t.id} onClick={()=> setTab(t.id)} className={`rounded-lg px-3 py-2 text-left text-sm ${tab===t.id ? "bg-[#ccc1a8] text-[#202124]" : "hover:bg-[#fefcf6] border border-transparent hover:border-[#e8e0c8]"}`}>{t.label}</button>
            ))}
          </nav>
          <div className="flex-1 space-y-4">
            <div className="flex gap-2 lg:hidden overflow-x-auto pb-2">
              {TABS.map(t=> <button key={t.id} onClick={()=> setTab(t.id)} className={`whitespace-nowrap rounded-lg px-3 py-1.5 text-xs ${tab===t.id ? "bg-[#ccc1a8] text-[#202124]" : "bg-[#fefcf6] border"}`}>{t.label}</button>)}
            </div>
            {tab==="general" && (
              <div className="rounded-2xl border border-[#e8e0c8] bg-[#fefcf6] p-5">
                <h3 className="font-semibold">General</h3>
                <div className="mt-4 grid gap-4">
                  <label className="flex items-center justify-between text-sm"><span>Undo send</span>
                    <select value={settings.general?.undo_send_seconds || "10"} onChange={e=> save("general","undo_send_seconds",e.target.value)} className="rounded border px-3 py-1 text-sm"><option value="5">5s</option><option value="10">10s</option><option value="20">20s</option><option value="30">30s</option></select>
                  </label>
                  <label className="flex items-center justify-between text-sm"><span>Density</span>
                    <select value={settings.general?.density || "comfortable"} onChange={e=> save("general","density",e.target.value)} className="rounded border px-3 py-1 text-sm"><option value="comfortable">Comfortable</option><option value="compact">Compact</option><option value="cozy">Cozy</option></select>
                  </label>
                  <label className="flex items-center justify-between text-sm"><span>Conversation view</span>
                    <input type="checkbox" checked={(settings.general?.conversation_view||"true")==="true"} onChange={e=> save("general","conversation_view",String(e.target.checked))} />
                  </label>
                  <label className="flex items-center justify-between text-sm"><span>Max page size</span>
                    <select value={settings.general?.page_size || "20"} onChange={e=> save("general","page_size",e.target.value)} className="rounded border px-3 py-1 text-sm"><option value="20">20</option><option value="50">50</option><option value="100">100</option></select>
                  </label>
                </div>
              </div>
            )}
            {tab==="inbox" && (
              <div className="rounded-2xl border border-[#e8e0c8] bg-[#fefcf6] p-5">
                <h3 className="font-semibold">Inbox</h3>
                <div className="mt-4 grid gap-4">
                  <label className="flex items-center justify-between text-sm"><span>Inbox type</span>
                    <select value={settings.inbox?.inbox_type || "Default"} onChange={e=> save("inbox","inbox_type",e.target.value)} className="rounded border px-3 py-1 text-sm"><option>Default</option><option>Unread first</option><option>Starred</option><option>Priority Inbox</option></select>
                  </label>
                  <label className="flex items-center justify-between text-sm"><span>Categories</span>
                    <input value={settings.inbox?.categories || "Primary,Promotions,Social"} onChange={e=> save("inbox","categories",e.target.value)} className="rounded border px-3 py-1 text-sm" />
                  </label>
                </div>
              </div>
            )}
            {tab==="signatures" && (
              <div className="space-y-4">
                <div className="rounded-2xl border border-[#e8e0c8] bg-[#fefcf6] p-5">
                  <h3 className="font-semibold">Signatures</h3>
                  <p className="text-sm text-zinc-500">Multi per mailbox — like Zoho/Gmail. {mailboxId ? `For ${mailboxes.find((m:any)=>m.id===mailboxId)?.address || mailboxId}` : "Pilih mailbox dulu."}</p>
                  {mailboxId && (
                    <>
                      <div className="mt-4 space-y-2">
                        {(() => {
                          const list = (signatures as any[]) || [];
                          if (list.length===0) return <div className="text-xs text-zinc-400">Belum ada signature — buat di bawah.</div>;
                          return list.map((s:any)=> (
                            <div key={s.id} className="flex items-center justify-between rounded-xl border border-[#e8e0c8] bg-white px-3 py-2">
                              <div className="min-w-0">
                                <div className="text-sm font-medium truncate">{s.name} {s.is_default ? <span className="ml-2 rounded-lg bg-[#ccc1a8] px-2 py-0.5 text-xs text-[#202124]">Default</span> : null}</div>
                                <div className="text-xs text-zinc-500 truncate max-w-[320px]" dangerouslySetInnerHTML={{__html: s.html?.slice(0,80) || ""}} />
                              </div>
                              <div className="flex gap-1">
                                {!s.is_default && <button onClick={async()=>{ await fetch(`${API}/v1/signatures/${s.id}`,{method:"PUT", headers:{"content-type":"application/json"}, body: JSON.stringify({is_default:true})}); loadSigs(mailboxId); }} className="rounded border border-[#e8e0c8] px-2 py-1 text-xs hover:bg-[#f8f6ef]">Set default</button>}
                                <button onClick={async()=>{ await fetch(`${API}/v1/signatures/${s.id}`,{method:"DELETE"}); loadSigs(mailboxId); }} className="rounded border border-red-200 px-2 py-1 text-xs text-red-600 hover:bg-red-50">Hapus</button>
                              </div>
                            </div>
                          ));
                        })()}
                      </div>
                      <div className="mt-4 rounded-xl border border-dashed border-[#e8e0c8] bg-[#f8f6ef] p-3">
                        <div className="text-xs font-semibold">Tambah signature</div>
                        <input value={newSigName} onChange={e=> setNewSigName(e.target.value)} placeholder="Nama (Default, Formal...)" className="mt-2 w-full rounded border border-[#e8e0c8] px-3 py-1.5 text-sm" />
                        <textarea value={newSigHtml} onChange={e=> setNewSigHtml(e.target.value)} placeholder="<p>Best,<br/>Nama — Aivory</p> (HTML)" rows={3} className="mt-2 w-full rounded border border-[#e8e0c8] px-3 py-1.5 text-xs font-mono" />
                        <label className="mt-2 flex items-center gap-2 text-xs"><input type="checkbox" checked={newSigDefault} onChange={e=> setNewSigDefault(e.target.checked)} /> Jadikan default</label>
                        <button onClick={async()=>{ if(!newSigHtml.trim()) return; await fetch(`${API}/v1/signatures`,{method:"POST", headers:{"content-type":"application/json"}, body: JSON.stringify({mailbox_id: mailboxId, name: newSigName.trim()||"Default", html: newSigHtml, text: newSigHtml.replace(/<[^>]+>/g,""), is_default: newSigDefault})}); setNewSigName(""); setNewSigHtml(""); setNewSigDefault(false); loadSigs(mailboxId); }} className="mt-3 rounded-lg bg-[#ccc1a8] px-4 py-1.5 text-xs font-semibold text-[#202124] hover:bg-[#ada48f]">Simpan signature</button>
                      </div>
                    </>
                  )}
                  {!mailboxId && <div className="mt-3 text-xs text-amber-700">Buat mailbox dulu di Domains / API.</div>}
                </div>
              </div>
            )}
            {tab==="compose" && (
              <div className="rounded-2xl border border-[#e8e0c8] bg-[#fefcf6] p-5">
                <h3 className="font-semibold">Compose</h3>
                <div className="mt-4 grid gap-4">
                  <label className="flex items-center justify-between text-sm"><span>Default font</span>
                    <select value={settings.compose?.default_font || "Manrope"} onChange={e=> save("compose","default_font",e.target.value)} className="rounded border px-3 py-1 text-sm"><option>Manrope</option><option>Verdana</option><option>Arial</option></select>
                  </label>
                  <label className="flex items-center justify-between text-sm"><span>Font size</span>
                    <select value={settings.compose?.font_size || "14"} onChange={e=> save("compose","font_size",e.target.value)} className="rounded border px-3 py-1 text-sm"><option value="12">12</option><option value="14">14</option><option value="16">16</option></select>
                  </label>
                  <label className="flex items-center justify-between text-sm"><span>Always show Cc</span><input type="checkbox" checked={(settings.compose?.always_show_cc||"false")==="true"} onChange={e=> save("compose","always_show_cc",String(e.target.checked))} /></label>
                  <label className="flex items-center justify-between text-sm"><span>Always show Bcc</span><input type="checkbox" checked={(settings.compose?.always_show_bcc||"false")==="true"} onChange={e=> save("compose","always_show_bcc",String(e.target.checked))} /></label>
                  <label className="flex items-center justify-between text-sm"><span>Outbox delay (min)</span>
                    <select value={settings.compose?.outbox_delay_minutes || "0"} onChange={e=> save("compose","outbox_delay_minutes",e.target.value)} className="rounded border px-3 py-1 text-sm"><option value="0">0</option><option value="1">1</option><option value="2">2</option><option value="5">5</option></select>
                  </label>
                </div>
              </div>
            )}
            {tab==="filters" && (
              <div className="space-y-4">
                <div className="rounded-2xl border border-[#e8e0c8] bg-[#fefcf6] p-5">
                  <h3 className="font-semibold">Filters & Labels — priority + reject/block</h3>
                  <p className="mt-1 text-xs text-zinc-500">Priority kecil menang duluan (0 tertinggi). Action: Move / Reject 550 / Block (auto Spam) / Forward copy. Match "contains" case-insensitive.</p>
                  <div className="mt-3 grid grid-cols-1 md:grid-cols-2 gap-2">
                    <input value={newFilter} onChange={e=> setNewFilter(e.target.value)} placeholder="From contains e.g. spam@evil.com" className="rounded border px-3 py-1.5 text-sm" />
                    <input value={newFilterSubject} onChange={e=> setNewFilterSubject(e.target.value)} placeholder="Subject contains (optional)" className="rounded border px-3 py-1.5 text-sm" />
                    <select value={newFilterAction} onChange={e=> setNewFilterAction(e.target.value)} className="rounded border px-3 py-1.5 text-sm">
                      <option value="move:Spam">Move to Spam</option>
                      <option value="move:Trash">Move to Trash</option>
                      <option value="move:Archive">Move to Archive</option>
                      <option value="move:Inbox">Move to Inbox</option>
                      <option value="reject">Reject 550</option>
                      <option value="block">Block + Spam</option>
                      <option value="forward">Forward copy</option>
                    </select>
                    <input type="number" value={newFilterPriority} onChange={e=> setNewFilterPriority(e.target.value)} placeholder="Priority 0" className="rounded border px-3 py-1.5 text-sm" />
                  </div>
                  {newFilterAction==="forward" && (
                    <input value={newFilterForward} onChange={e=> setNewFilterForward(e.target.value)} placeholder="Forward to email" className="mt-2 w-full rounded border px-3 py-1.5 text-sm" />
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
                    await fetch(`${API}/v1/filters`,{method:"POST",headers:{"content-type":"application/json"}, body: JSON.stringify({name:`filter prio ${prio}: ${JSON.stringify(criteria)} -> ${JSON.stringify(action)}`, criteria, action, priority:prio})});
                    setNewFilter(""); setNewFilterSubject(""); setNewFilterForward(""); setNewFilterPriority("0"); loadFilters();
                  }} className="mt-3 rounded bg-[#ccc1a8] px-4 py-1.5 text-sm font-medium text-[#202124] transition-transform duration-150 active:scale-[0.97]">Add filter (prio {newFilterPriority})</button>
                  <div className="mt-4 space-y-2">
                    {filters.map((f:any)=> (
                      <div key={f.id} className="flex items-center justify-between rounded border bg-white px-3 py-2 text-sm">
                        <div className="min-w-0">
                          <div className="font-medium truncate">[{f.priority??0}] {f.name}</div>
                          <div className="text-xs text-zinc-400 truncate">crit {typeof f.criteria==="string"?f.criteria:JSON.stringify(f.criteria)} → act {typeof f.action==="string"?f.action:JSON.stringify(f.action)}</div>
                        </div>
                        <div className="flex items-center gap-2">
                          <span className={`rounded-lg px-2 py-0.5 text-xs ${f.enabled?"bg-emerald-50 text-emerald-700":"bg-zinc-100 text-zinc-500"}`}>{f.enabled?"enabled":"disabled"}</span>
                          <button onClick={async()=>{ await fetch(`${API}/v1/filters/${f.id}`,{method:"PUT", headers:{"content-type":"application/json"}, body: JSON.stringify({enabled: !f.enabled})}); loadFilters(); }} className="rounded border px-2 py-1 text-xs">{f.enabled?"Disable":"Enable"}</button>
                          <button onClick={async()=>{ await fetch(`${API}/v1/filters/${f.id}`,{method:"DELETE"}); loadFilters(); }} className="rounded border border-red-200 px-2 py-1 text-xs text-red-600">Delete</button>
                        </div>
                      </div>
                    ))}
                    {filters.length===0 && <div className="text-xs text-zinc-400">No filters yet — add one above (from/subject → move/reject/block/forward).</div>}
                  </div>
                </div>
                <div className="rounded-2xl border border-[#e8e0c8] bg-[#fefcf6] p-5">
                  <h3 className="font-semibold">Labels</h3>
                  <div className="mt-3 flex gap-2">
                    <input value={newLabel} onChange={e=> setNewLabel(e.target.value)} placeholder="Label name" className="flex-1 rounded border px-3 py-1.5 text-sm" />
                    <button onClick={async()=>{ await fetch(`${API}/v1/labels`,{method:"POST",headers:{"content-type":"application/json"}, body: JSON.stringify({name:newLabel, color:"#3b82f6"})}); setNewLabel(""); loadLabels();}} className="rounded bg-[#ccc1a8] px-4 py-1.5 text-sm text-[#202124]">Add label</button>
                  </div>
                  <div className="mt-3 flex flex-wrap gap-2">{labels.map((l:any)=> <span key={l.id} className="rounded-lg px-2.5 py-1 text-xs text-white" style={{background:l.color}}>{l.name}</span>)}{labels.length===0 && <span className="text-xs text-zinc-400">No labels</span>}</div>
                </div>
              </div>
            )}
            {tab==="contacts" && (
              <div className="space-y-4">
                <div className="rounded-2xl border border-[#e8e0c8] bg-[#fefcf6] p-5">
                  <h3 className="font-semibold">Contacts — import & blocklist</h3>
                  <p className="mt-1 text-xs text-zinc-500">{contacts.length} contacts. Auto-aggregated from inbound From. Import CSV: email,display_name per line.</p>
                  <div className="mt-3 space-y-2 max-h-64 overflow-y-auto rounded border bg-white p-2 text-xs">
                    {contacts.slice(0,50).map((c:any)=> (
                      <div key={c.id} className="flex justify-between border-b border-zinc-100 py-1">
                        <span className="font-mono">{c.email}</span>
                        <span className={c.blocked?"text-red-600":"text-zinc-500"}>{c.blocked?"blocked":""} {c.display_name}</span>
                      </div>
                    ))}
                    {contacts.length===0 && <div className="text-zinc-400">No contacts yet</div>}
                  </div>
                  <div className="mt-3">
                    <textarea value={csvInput} onChange={e=> setCsvInput(e.target.value)} placeholder={"email,display_name\nalice@example.com,Alice\nbob@example.com,Bob"} rows={4} className="w-full rounded border px-3 py-2 text-xs font-mono" />
                    <div className="mt-2 flex gap-2">
                      <button onClick={async()=>{ if(!csvInput.trim()) return; const r=await fetch(`${API}/v1/contacts/import`,{method:"POST",headers:{"content-type":"application/json"}, body: JSON.stringify({csv: csvInput})}); const j=await r.json(); setImportResult(j.success?`Imported ${j.data?.imported||0}`: (j.error||"failed")); loadContacts(); }} className="rounded bg-[#ccc1a8] px-4 py-1.5 text-xs text-[#202124]">Import CSV</button>
                      <button onClick={async()=>{ const r=await fetch(`${API}/v1/contacts/import`,{method:"POST",headers:{"content-type":"application/json"}, body: JSON.stringify({contacts: [{email:"demo@example.com", display_name:"Demo"}]})}); const j=await r.json(); setImportResult(`Demo: ${JSON.stringify(j.data)}`); loadContacts(); }} className="rounded border px-3 py-1.5 text-xs">Demo import</button>
                      <span className="text-xs text-zinc-500 self-center">{importResult}</span>
                    </div>
                  </div>
                </div>
              </div>
            )}
            {tab==="webhooks" && (
              <div className="space-y-4">
                <div className="rounded-2xl border border-[#e8e0c8] bg-[#fefcf6] p-5">
                  <h3 className="font-semibold">Webhooks — delivery & retry</h3>
                  <p className="mt-1 text-xs text-zinc-500">Fire on email.received to any URL, HMAC secret optional, retry visibility per delivery.</p>
                  <div className="mt-3 flex flex-wrap gap-2">
                    <input value={newWebhookUrl} onChange={e=> setNewWebhookUrl(e.target.value)} placeholder="https://example.com/webhook" className="flex-1 rounded border px-3 py-1.5 text-sm" />
                    <input value={newWebhookEvents} onChange={e=> setNewWebhookEvents(e.target.value)} placeholder="events csv: email.received" className="w-40 rounded border px-3 py-1.5 text-sm" />
                    <button onClick={async()=>{ if(!newWebhookUrl.trim()) return; const evs = newWebhookEvents.split(",").map(s=>s.trim()).filter(Boolean); await authFetch("/v1/webhooks",{method:"POST",headers:{"content-type":"application/json"}, body: JSON.stringify({url:newWebhookUrl.trim(), events:evs})}); setNewWebhookUrl(""); loadWebhooks(); }} className="rounded bg-[#ccc1a8] px-4 py-1.5 text-sm text-[#202124]">Add webhook</button>
                  </div>
                  <div className="mt-3 space-y-2">
                    {webhooks.map((w:any)=> (
                      <div key={w.id} className="rounded border bg-white p-3 text-sm">
                        <div className="flex justify-between">
                          <span className="font-mono text-xs truncate">{w.url}</span>
                          <button onClick={async()=>{ await authFetch(`/v1/webhooks/${w.id}`,{method:"DELETE"}); loadWebhooks(); }} className="text-xs text-red-600">Delete</button>
                        </div>
                        <div className="text-xs text-zinc-400">events: {JSON.stringify(w.events)} • {w.enabled?"enabled":"disabled"}</div>
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
                    {webhooks.length===0 && <div className="text-xs text-zinc-400">No webhooks yet</div>}
                  </div>
                </div>
              </div>
            )}
            {tab==="agent" && (
              <div className="space-y-4">
                <div className="rounded-2xl border border-[#e8e0c8] bg-[#fefcf6] p-5">
                  <h3 className="font-semibold">Agent Tasks — inbox by state</h3>
                  <p className="mt-1 text-xs text-zinc-500">States: needs_reply / waiting_on_me / waiting_on_them / fyi / auto_handled / needs_approval — human-approved actions.</p>
                  <div className="mt-3 flex gap-2">
                    <select value={agentFilterState} onChange={e=> setAgentFilterState(e.target.value)} className="rounded border px-3 py-1.5 text-sm">
                      <option value="">All states</option>
                      <option value="needs_reply">needs_reply</option>
                      <option value="waiting_on_me">waiting_on_me</option>
                      <option value="waiting_on_them">waiting_on_them</option>
                      <option value="fyi">fyi</option>
                      <option value="auto_handled">auto_handled</option>
                      <option value="needs_approval">needs_approval</option>
                    </select>
                    <button onClick={loadAgentTasks} className="rounded border px-3 py-1.5 text-sm">Filter</button>
                    <button onClick={async()=>{ await fetch(`${API}/v1/agent/tasks`,{method:"POST",headers:{"content-type":"application/json"}, body: JSON.stringify({type:"triage", state:"needs_reply", title:"Demo task "+Date.now(), body:"Follow up demo"})}); loadAgentTasks(); }} className="rounded bg-[#ccc1a8] px-4 py-1.5 text-sm text-[#202124]">Create demo task</button>
                  </div>
                  <div className="mt-3 space-y-2 max-h-80 overflow-y-auto">
                    {agentTasks.map((t:any)=> (
                      <div key={t.id} className="rounded border bg-white p-3 text-sm">
                        <div className="flex justify-between">
                          <span className="font-medium">{t.title}</span>
                          <span className={`rounded-lg px-2 py-0.5 text-xs ${t.state==="needs_reply"?"bg-amber-50 text-amber-700": t.state==="needs_approval"?"bg-red-50 text-red-700":"bg-zinc-100 text-zinc-600"}`}>{t.state}</span>
                        </div>
                        <div className="text-xs text-zinc-500 truncate">{t.body}</div>
                        <div className="mt-1 flex gap-1">
                          <select defaultValue={t.state} onChange={async(e)=>{ await fetch(`${API}/v1/agent/tasks/${t.id}`,{method:"PUT", headers:{"content-type":"application/json"}, body: JSON.stringify({state: e.target.value})}); loadAgentTasks(); }} className="rounded border px-2 py-1 text-xs">
                            <option value="needs_reply">needs_reply</option>
                            <option value="waiting_on_me">waiting_on_me</option>
                            <option value="waiting_on_them">waiting_on_them</option>
                            <option value="fyi">fyi</option>
                            <option value="auto_handled">auto_handled</option>
                            <option value="needs_approval">needs_approval</option>
                            <option value="done">done</option>
                          </select>
                          <span className="text-xs text-zinc-400">{t.type} • {new Date(t.created_at).toLocaleString()}</span>
                        </div>
                      </div>
                    ))}
                    {agentTasks.length===0 && <div className="text-xs text-zinc-400">No agent tasks — create demo or trigger via AI intelligence.</div>}
                  </div>
                </div>
              </div>
            )}
            {tab==="vacation" && (
              <div className="rounded-2xl border border-[#e8e0c8] bg-[#fefcf6] p-5">
                <h3 className="font-semibold">Vacation responder</h3>
                <p className="mt-1 text-xs text-zinc-500">{mailboxId ? `For ${mailboxes.find((m:any)=>m.id===mailboxId)?.address || mailboxId}` : "No mailbox yet — create one first."} — auto-replies once per sender per day while enabled.</p>
                <div className="mt-4 grid gap-4">
                  <label className="flex items-center justify-between text-sm"><span>Enabled</span>
                    <input type="checkbox" checked={!!vac.enabled} onChange={e=> saveVac({...vac, enabled: e.target.checked})} disabled={!mailboxId} />
                  </label>
                  <label className="flex flex-col gap-1 text-sm"><span className="text-zinc-500">Subject</span>
                    <input value={vac.subject||""} onChange={e=> setVac({...vac, subject: e.target.value})} onBlur={()=> saveVac(vac)} disabled={!mailboxId} className="rounded border px-3 py-1.5 text-sm disabled:bg-zinc-50" />
                  </label>
                  <label className="flex flex-col gap-1 text-sm"><span className="text-zinc-500">Message</span>
                    <textarea value={vac.body||""} onChange={e=> setVac({...vac, body: e.target.value})} onBlur={()=> saveVac(vac)} disabled={!mailboxId} rows={4} className="rounded border px-3 py-1.5 text-sm disabled:bg-zinc-50" />
                  </label>
                </div>
              </div>
            )}
            {tab==="forwarding" && (
              <div className="space-y-4">
                <div className="rounded-2xl border border-[#e8e0c8] bg-[#fefcf6] p-5">
                  <h3 className="font-semibold">Forwarding & POP/IMAP</h3>
                  <div className="mt-4 grid gap-4">
                    <label className="flex items-center justify-between text-sm"><span>Forward to</span>
                      <input value={settings.forwarding?.forward_to || ""} onChange={e=> save("forwarding","forward_to",e.target.value)} placeholder="forward@aivory.uk" className="rounded border px-3 py-1 text-sm" />
                    </label>
                    <label className="flex items-center justify-between text-sm"><span>Keep copy</span><input type="checkbox" checked={(settings.forwarding?.keep_copy||"true")==="true"} onChange={e=> save("forwarding","keep_copy",String(e.target.checked))} /></label>
                    <label className="flex items-center justify-between text-sm"><span>POP enabled</span><input type="checkbox" checked={(settings.forwarding?.pop_enabled||"false")==="true"} onChange={e=> save("forwarding","pop_enabled",String(e.target.checked))} /></label>
                    <label className="flex items-center justify-between text-sm"><span>IMAP enabled</span><input type="checkbox" checked={(settings.forwarding?.imap_enabled||"true")==="true"} onChange={e=> save("forwarding","imap_enabled",String(e.target.checked))} /></label>
                  </div>
                </div>
                <div className="rounded-2xl border border-[#e8e0c8] bg-[#fefcf6] p-5">
                  <h3 className="font-semibold">Send As</h3>
                  <p className="mt-1 text-xs text-zinc-500">{mailboxId ? `Aliases for ${mailboxes.find((m:any)=>m.id===mailboxId)?.address || mailboxId}` : "No mailbox yet — create one first."} Appears in the compose From dropdown. Sending still requires the alias's domain to be verified.</p>
                  <div className="mt-3 flex flex-wrap gap-2">
                    <input value={newAliasName} onChange={e=> setNewAliasName(e.target.value)} placeholder="Display name (optional)" className="w-40 rounded border px-3 py-1.5 text-sm" disabled={!mailboxId} />
                    <input value={newAlias} onChange={e=> setNewAlias(e.target.value)} placeholder="alias@yourdomain.com" className="flex-1 rounded border px-3 py-1.5 text-sm" disabled={!mailboxId} />
                    <button onClick={addAlias} disabled={!mailboxId} className="rounded bg-zinc-900 px-4 py-1.5 text-sm font-medium text-white transition-transform duration-150 active:scale-[0.97] disabled:opacity-50">Add alias</button>
                  </div>
                  <div className="mt-3 space-y-2">
                    {aliases.map((a:any)=> (
                      <div key={a.id} className="flex items-center justify-between rounded border px-3 py-1.5 text-sm">
                        <span>{a.display_name ? `${a.display_name} <${a.alias_email}>` : a.alias_email}{a.is_default && <span className="ml-2 rounded-lg bg-zinc-100 px-2 py-0.5 text-xs text-zinc-500">Default</span>}</span>
                        <button onClick={()=> removeAlias(a.id)} className="text-xs text-zinc-400 hover:text-red-600">Remove</button>
                      </div>
                    ))}
                    {aliases.length===0 && <div className="text-xs text-zinc-400">No aliases yet</div>}
                  </div>
                </div>
              </div>
            )}
            {tab==="appearance" && (
              <div className="rounded-2xl border border-[#e8e0c8] bg-[#fefcf6] p-5">
                <h3 className="font-semibold">Appearance</h3>
                <div className="mt-4 grid gap-4">
                  <label className="flex items-center justify-between text-sm"><span>Theme</span>
                    <select value={settings.appearance?.theme || "light"} onChange={e=> save("appearance","theme",e.target.value)} className="rounded border px-3 py-1 text-sm"><option value="light">Light</option><option value="dark">Dark</option></select>
                  </label>
                  <label className="flex items-center justify-between text-sm"><span>Reading pane</span>
                    <select value={settings.appearance?.reading_pane || "right"} onChange={e=> save("appearance","reading_pane",e.target.value)} className="rounded border px-3 py-1 text-sm"><option value="right">Right</option><option value="bottom">Bottom</option><option value="no-split">No split</option></select>
                  </label>
                </div>
              </div>
            )}
            {tab==="notifications" && (
              <div className="rounded-2xl border border-[#e8e0c8] bg-[#fefcf6] p-5">
                <h3 className="font-semibold">Notifications</h3>
                <div className="mt-4 grid gap-4">
                  <label className="flex items-center justify-between text-sm"><span>Desktop sound</span><input type="checkbox" checked={(settings.notifications?.desktop_sound||"true")==="true"} onChange={e=> save("notifications","desktop_sound",String(e.target.checked))} /></label>
                  <label className="flex items-center justify-between text-sm"><span>New mail banner</span><input type="checkbox" checked={(settings.notifications?.new_mail_banner||"true")==="true"} onChange={e=> save("notifications","new_mail_banner",String(e.target.checked))} /></label>
                </div>
              </div>
            )}
            {tab==="shortcuts" && (
              <div className="rounded-2xl border border-[#e8e0c8] bg-[#fefcf6] p-5">
                <h3 className="font-semibold">Keyboard shortcuts</h3>
                <label className="flex items-center justify-between text-sm"><span>Enable shortcuts</span><input type="checkbox" checked={(settings.shortcuts?.enabled||"true")==="true"} onChange={e=> save("shortcuts","enabled",String(e.target.checked))} /></label>
                <div className="mt-3 text-xs text-zinc-500">c compose, e archive, r reply, / search.</div>
              </div>
            )}
            {tab==="storage" && (
              <div className="rounded-2xl border border-[#e8e0c8] bg-[#fefcf6] p-5">
                <h3 className="font-semibold">Storage & Offline</h3>
                <div className="mt-4 grid gap-4">
                  <label className="flex items-center justify-between text-sm"><span>Days to sync</span>
                    <select value={settings.storage?.days_to_sync || "30"} onChange={e=> save("storage","days_to_sync",e.target.value)} className="rounded border px-3 py-1 text-sm"><option value="7">7</option><option value="30">30</option><option value="90">90</option></select>
                  </label>
                  <label className="flex items-center justify-between text-sm"><span>Download on WiFi only</span><input type="checkbox" checked={(settings.storage?.download_attachments_wifi_only||"true")==="true"} onChange={e=> save("storage","download_attachments_wifi_only",String(e.target.checked))} /></label>
                </div>
              </div>
            )}
            {tab==="integrations" && (
              <div className="space-y-4">
                {integMsg && <div className="rounded-xl bg-amber-50 px-4 py-2 text-sm text-amber-800 ring-1 ring-amber-200">{integMsg} <button onClick={()=> setIntegMsg("")} className="ml-2 text-xs underline">×</button></div>}
                {integLoading ? <div className="rounded-2xl border border-[#e8e0c8] bg-white p-8 text-center text-sm text-zinc-400">Loading…</div>
                : integration?.connected && !integShowForm ? (
                  <div className="rounded-2xl border border-emerald-200 bg-white p-5 shadow-sm">
                    <div className="flex items-start justify-between gap-3">
                      <div>
                        <div className="flex items-center gap-2">
                          <span className="h-2.5 w-2.5 rounded-full bg-emerald-500 animate-pulse" />
                          <h3 className="font-semibold text-[#202124]">Connected</h3>
                          <span className="rounded-lg bg-emerald-50 px-2 py-0.5 text-xs font-medium text-emerald-700">IMAP ready</span>
                        </div>
                        <p className="mt-1 text-sm text-zinc-600">Connected as <span className="font-mono font-semibold text-[#202124]">{integration.username || integration.address}</span></p>
                        <p className="mt-1 text-xs text-zinc-500 font-mono">{integration.host}:{integration.port} · IMAP 993 SSL · SMTP 587 STARTTLS · username = full address</p>
                        {integration.updated_at && <p className="mt-1 text-xs text-zinc-400">Last updated {new Date(integration.updated_at).toLocaleString()}</p>}
                      </div>
                      <div className="flex gap-2">
                        <button onClick={()=> setIntegShowForm(true)} className="rounded-lg border border-[#e8e0c8] bg-[#fefcf6] px-4 py-1.5 text-xs font-medium hover:bg-[#f8f6ef]">Reconnect</button>
                        <button onClick={disconnectIntegration} className="rounded-lg border border-red-200 bg-white px-4 py-1.5 text-xs font-medium text-red-600 hover:bg-red-50">Disconnect</button>
                      </div>
                    </div>
                    <div className="mt-4 rounded-xl bg-[#f8f6ef] px-3 py-2 text-xs text-zinc-500">Password tidak pernah ditampilkan lagi setelah save — seperti App Passwords. Jika lupa, gunakan Reconnect. Admin bisa cek status di Admin Console.</div>
                  </div>
                ) : (
                  <div className="rounded-2xl border border-[#e8e0c8] bg-[#fefcf6] p-5 shadow-sm">
                    <h3 className="font-semibold text-[#202124]">Integrations • Account • IMAP</h3>
                    <p className="mt-1 text-xs text-zinc-500">Sub-section terpisah dari profile — host/port/username/password. Test dulu sebelum Save. Setelah tersimpan, hanya status Connected yang tampil.</p>
                    <div className="mt-4 grid gap-4">
                      <div className="grid md:grid-cols-3 gap-3">
                        <label className="flex flex-col gap-1 text-sm"><span className="text-xs font-medium text-zinc-600">IMAP host</span><input value={integHost} onChange={e=>{setIntegHost(e.target.value); setIntegTestOk(null);}} placeholder="mail.aivory.uk" className="rounded-lg border border-[#e8e0c8] bg-white px-3 py-2 text-sm font-mono focus:border-[#ccc1a8] focus:outline-none" /></label>
                        <label className="flex flex-col gap-1 text-sm"><span className="text-xs font-medium text-zinc-600">Port</span><input value={integPort} onChange={e=>{setIntegPort(e.target.value); setIntegTestOk(null);}} placeholder="993" inputMode="numeric" className="rounded-lg border border-[#e8e0c8] bg-white px-3 py-2 text-sm font-mono focus:border-[#ccc1a8] focus:outline-none" /></label>
                        <label className="flex flex-col gap-1 text-sm"><span className="text-xs font-medium text-zinc-600">Username</span><input value={integUser} onChange={e=>{setIntegUser(e.target.value); setIntegTestOk(null);}} placeholder="you@domain.com" className="rounded-lg border border-[#e8e0c8] bg-white px-3 py-2 text-sm font-mono focus:border-[#ccc1a8] focus:outline-none" /></label>
                      </div>
                      <label className="flex flex-col gap-1 text-sm"><span className="text-xs font-medium text-zinc-600">Password</span>
                        <div className="flex gap-2">
                          <div className="relative flex-1">
                            <input type={integShowPw ? "text":"password"} value={integPw} onChange={e=>{setIntegPw(e.target.value); setIntegTestOk(null); setIntegTestMsg("");}} placeholder="IMAP password (min 8 chars)" className="w-full rounded-lg border border-[#e8e0c8] bg-white px-3 py-2 pr-10 text-sm font-mono focus:border-[#ccc1a8] focus:outline-none" />
                            <button type="button" onClick={()=> setIntegShowPw(v=>!v)} className="absolute right-2 top-1/2 -translate-y-1/2 rounded p-1 text-zinc-400 hover:bg-zinc-100">{integShowPw ? "Hide" : "Show"}</button>
                          </div>
                          <button type="button" onClick={()=> setIntegShowPw(v=>!v)} className="rounded-lg border border-[#e8e0c8] bg-white px-3 py-2 text-xs hover:bg-[#f8f6ef]">{integShowPw ? "Hide":"Show"}</button>
                        </div>
                        <span className="text-xs text-zinc-400">Terpisah dari password web login — App-passwords parity.</span>
                      </label>
                      <div className="flex flex-wrap items-center gap-2">
                        <button onClick={testIntegration} disabled={integTesting || !integHost || !integUser || !integPw} className="rounded-lg border border-[#005a5e] bg-white px-4 py-2 text-sm font-medium text-[#005a5e] hover:bg-[#f0f7f7] disabled:opacity-50">{integTesting ? "Testing…":"Test connection"}</button>
                        {integTestOk===true && <span className="text-xs font-medium text-emerald-700">✓ {integTestMsg}</span>}
                        {integTestOk===false && <span className="text-xs font-medium text-red-600">✗ {integTestMsg}</span>}
                        {integTestOk===null && <span className="text-xs text-zinc-400">Wajib test sebelum Save</span>}
                      </div>
                      <div className="flex justify-end gap-2 pt-2 border-t border-[#f0ece0]">
                        {integration?.connected && <button onClick={()=> {setIntegShowForm(false); setIntegTestOk(null);}} className="rounded-lg border border-[#e8e0c8] bg-white px-4 py-2 text-sm hover:bg-[#f8f6ef]">Cancel</button>}
                        <button onClick={saveIntegration} disabled={integSaving || integTestOk!==true} className="rounded-lg bg-[#005a5e] px-6 py-2 text-sm font-semibold text-white hover:bg-[#00454a] disabled:opacity-40">{integSaving ? "Saving…":"Save & connect"}</button>
                      </div>
                    </div>
                  </div>
                )}
                <div className="rounded-xl border border-dashed border-[#e8e0c8] bg-white p-4 text-xs text-zinc-500">
                  <div className="font-medium text-zinc-600">Untuk mail client (Thunderbird/Apple Mail/Outlook)</div>
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
