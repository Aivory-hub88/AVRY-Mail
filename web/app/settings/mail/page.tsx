"use client";
import { useEffect, useState } from "react";
const API = process.env.NEXT_PUBLIC_MAIL_API || "http://localhost:8095";
const TABS = [
  {id:"general", label:"General"},
  {id:"inbox", label:"Inbox"},
  {id:"signatures", label:"Signatures"},
  {id:"compose", label:"Compose"},
  {id:"filters", label:"Filters & Labels"},
  {id:"forwarding", label:"Forwarding & POP/IMAP"},
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
  async function loadVac(){ const r=await fetch(`${API}/v1/vacation?mailbox_id=`); const j=await r.json(); setVac(j.data||{enabled:false}); }
  useEffect(()=>{ TABS.forEach(t=> loadSettings(t.id)); loadLabels(); loadFilters(); loadVac(); },[]);
  return (
    <div className="min-h-screen bg-zinc-50 font-[Manrope]">
      <div className="mx-auto max-w-5xl p-6">
        <div className="flex items-center justify-between">
          <div className="text-sm text-zinc-500"><a href="/settings" className="underline">Settings</a> / <span className="font-semibold text-zinc-900">Mail</span></div>
          <a href="/settings" className="rounded-full border border-zinc-200 bg-white px-3 py-1 text-xs">← API & MCP</a>
        </div>
        <h1 className="mt-2 text-3xl font-bold font-[Manrope]">Mail user settings</h1>
        <p className="mt-1 text-sm text-zinc-500">Gmail / Zoho / Outlook parity — Manrope throughout</p>
        <div className="mt-6 flex gap-6">
          <nav className="hidden w-48 shrink-0 flex-col gap-1 lg:flex">
            {TABS.map(t=> (
              <button key={t.id} onClick={()=> setTab(t.id)} className={`rounded-lg px-3 py-2 text-left text-sm ${tab===t.id ? "bg-zinc-900 text-white" : "hover:bg-white border border-transparent hover:border-zinc-200"}`}>{t.label}</button>
            ))}
          </nav>
          <div className="flex-1 space-y-4">
            <div className="flex gap-2 lg:hidden overflow-x-auto pb-2">
              {TABS.map(t=> <button key={t.id} onClick={()=> setTab(t.id)} className={`whitespace-nowrap rounded-full px-3 py-1.5 text-xs ${tab===t.id ? "bg-zinc-900 text-white" : "bg-white border"}`}>{t.label}</button>)}
            </div>
            {tab==="general" && (
              <div className="rounded-2xl border border-zinc-200 bg-white p-5">
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
              <div className="rounded-2xl border border-zinc-200 bg-white p-5">
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
              <div className="rounded-2xl border border-zinc-200 bg-white p-5">
                <h3 className="font-semibold">Signatures</h3>
                <p className="text-sm text-zinc-500">Multi per mailbox — like Zoho/Gmail</p>
                <div className="mt-3 text-sm text-zinc-400">Manage in mail detail → Signature • Default. API: POST /v1/signatures</div>
                <a href="/" className="mt-3 inline-flex rounded-lg border px-3 py-1.5 text-xs">Open mail → Signature modal</a>
              </div>
            )}
            {tab==="compose" && (
              <div className="rounded-2xl border border-zinc-200 bg-white p-5">
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
                <div className="rounded-2xl border border-zinc-200 bg-white p-5">
                  <h3 className="font-semibold">Filters & Labels</h3>
                  <div className="mt-3 flex gap-2">
                    <input value={newFilter} onChange={e=> setNewFilter(e.target.value)} placeholder="Filter name" className="flex-1 rounded border px-3 py-1.5 text-sm" />
                    <button onClick={async()=>{ await fetch(`${API}/v1/filters`,{method:"POST",headers:{"content-type":"application/json"}, body: JSON.stringify({name:newFilter, criteria:{from:"finance@"}, action:{move:"Inbox"}})}); setNewFilter(""); loadFilters();}} className="rounded bg-zinc-900 px-4 py-1.5 text-sm text-white">Add filter</button>
                  </div>
                  <div className="mt-3 space-y-2">{filters.map((f:any)=> <div key={f.id} className="flex justify-between rounded border px-3 py-1.5 text-sm"><span>{f.name}</span><span className="text-xs text-zinc-400">{f.criteria}</span></div>)}{filters.length===0 && <div className="text-xs text-zinc-400">No filters yet</div>}</div>
                </div>
                <div className="rounded-2xl border border-zinc-200 bg-white p-5">
                  <h3 className="font-semibold">Labels</h3>
                  <div className="mt-3 flex gap-2">
                    <input value={newLabel} onChange={e=> setNewLabel(e.target.value)} placeholder="Label name" className="flex-1 rounded border px-3 py-1.5 text-sm" />
                    <button onClick={async()=>{ await fetch(`${API}/v1/labels`,{method:"POST",headers:{"content-type":"application/json"}, body: JSON.stringify({name:newLabel, color:"#3b82f6"})}); setNewLabel(""); loadLabels();}} className="rounded bg-zinc-900 px-4 py-1.5 text-sm text-white">Add label</button>
                  </div>
                  <div className="mt-3 flex flex-wrap gap-2">{labels.map((l:any)=> <span key={l.id} className="rounded-full px-2.5 py-1 text-xs text-white" style={{background:l.color}}>{l.name}</span>)}{labels.length===0 && <span className="text-xs text-zinc-400">No labels</span>}</div>
                </div>
              </div>
            )}
            {tab==="forwarding" && (
              <div className="rounded-2xl border border-zinc-200 bg-white p-5">
                <h3 className="font-semibold">Forwarding & POP/IMAP · Send As</h3>
                <div className="mt-4 grid gap-4">
                  <label className="flex items-center justify-between text-sm"><span>Forward to</span>
                    <input value={settings.forwarding?.forward_to || ""} onChange={e=> save("forwarding","forward_to",e.target.value)} placeholder="forward@aivory.uk" className="rounded border px-3 py-1 text-sm" />
                  </label>
                  <label className="flex items-center justify-between text-sm"><span>Keep copy</span><input type="checkbox" checked={(settings.forwarding?.keep_copy||"true")==="true"} onChange={e=> save("forwarding","keep_copy",String(e.target.checked))} /></label>
                  <label className="flex items-center justify-between text-sm"><span>POP enabled</span><input type="checkbox" checked={(settings.forwarding?.pop_enabled||"false")==="true"} onChange={e=> save("forwarding","pop_enabled",String(e.target.checked))} /></label>
                  <label className="flex items-center justify-between text-sm"><span>IMAP enabled</span><input type="checkbox" checked={(settings.forwarding?.imap_enabled||"true")==="true"} onChange={e=> save("forwarding","imap_enabled",String(e.target.checked))} /></label>
                </div>
              </div>
            )}
            {tab==="appearance" && (
              <div className="rounded-2xl border border-zinc-200 bg-white p-5">
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
              <div className="rounded-2xl border border-zinc-200 bg-white p-5">
                <h3 className="font-semibold">Notifications</h3>
                <div className="mt-4 grid gap-4">
                  <label className="flex items-center justify-between text-sm"><span>Desktop sound</span><input type="checkbox" checked={(settings.notifications?.desktop_sound||"true")==="true"} onChange={e=> save("notifications","desktop_sound",String(e.target.checked))} /></label>
                  <label className="flex items-center justify-between text-sm"><span>New mail banner</span><input type="checkbox" checked={(settings.notifications?.new_mail_banner||"true")==="true"} onChange={e=> save("notifications","new_mail_banner",String(e.target.checked))} /></label>
                </div>
              </div>
            )}
            {tab==="shortcuts" && (
              <div className="rounded-2xl border border-zinc-200 bg-white p-5">
                <h3 className="font-semibold">Keyboard shortcuts</h3>
                <label className="flex items-center justify-between text-sm"><span>Enable shortcuts</span><input type="checkbox" checked={(settings.shortcuts?.enabled||"true")==="true"} onChange={e=> save("shortcuts","enabled",String(e.target.checked))} /></label>
                <div className="mt-3 text-xs text-zinc-500">c compose, e archive, r reply, / search.</div>
              </div>
            )}
            {tab==="storage" && (
              <div className="rounded-2xl border border-zinc-200 bg-white p-5">
                <h3 className="font-semibold">Storage & Offline</h3>
                <div className="mt-4 grid gap-4">
                  <label className="flex items-center justify-between text-sm"><span>Days to sync</span>
                    <select value={settings.storage?.days_to_sync || "30"} onChange={e=> save("storage","days_to_sync",e.target.value)} className="rounded border px-3 py-1 text-sm"><option value="7">7</option><option value="30">30</option><option value="90">90</option></select>
                  </label>
                  <label className="flex items-center justify-between text-sm"><span>Download on WiFi only</span><input type="checkbox" checked={(settings.storage?.download_attachments_wifi_only||"true")==="true"} onChange={e=> save("storage","download_attachments_wifi_only",String(e.target.checked))} /></label>
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
