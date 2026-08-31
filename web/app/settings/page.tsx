"use client";
import { useEffect, useState } from "react";
const API = process.env.NEXT_PUBLIC_MAIL_API || "http://localhost:8095";

export default function SettingsPage() {
  const [keys, setKeys] = useState<any[]>([]);
  const [showRaw, setShowRaw] = useState<string | null>(null);
  const [rawMap, setRawMap] = useState<Record<string, string>>({});
  const [mcpLink, setMcpLink] = useState("");
  const [selectedKey, setSelectedKey] = useState("default");
  const [coupon, setCoupon] = useState("");

  async function load() {
    const r = await fetch(`${API}/v1/api-keys`);
    const j = await r.json();
    const list = j.data || [];
    setKeys(list);
    const raws: Record<string,string> = {};
    list.forEach((k:any)=> { if(k.key_raw) raws[k.id]=k.key_raw; });
    setRawMap(prev=> ({...prev, ...raws}));
    if(list[0]?.name) setSelectedKey(list[0].name);
  }
  useEffect(()=> { load(); }, []);

  async function create() {
    const r = await fetch(`${API}/v1/api-keys`, {method:"POST", headers:{"content-type":"application/json"}, body: JSON.stringify({name: "dev"})});
    const j = await r.json();
    if(j.data?.key_raw) { setRawMap(m=> ({...m, [j.data.id]: j.data.key_raw})); setShowRaw(j.data.key_raw); }
    load();
  }
  async function del(id:string) {
    await fetch(`${API}/v1/api-keys/${id}`, {method:"DELETE"});
    load();
  }
  async function generate() {
    const r = await fetch(`${API}/v1/mcp/generate-link`, {method:"POST", headers:{"content-type":"application/json"}, body: JSON.stringify({name: selectedKey})});
    const j = await r.json();
    setMcpLink(j.data?.mcp_link || j.data?.mcp_url || "");
  }

  return (
    <div className="min-h-screen bg-zinc-50">
      <div className="mx-auto max-w-4xl p-6">
        {/* Header like Tavily */}
        <div className="flex items-center justify-between">
          <div className="text-sm text-zinc-500">Pages / <span className="font-semibold text-zinc-900">Overview</span></div>
          <div className="flex items-center gap-2">
            <span className="flex items-center gap-2 rounded-full border border-zinc-200 bg-white px-3 py-1.5 text-sm"><span className="h-2 w-2 rounded-full bg-emerald-500" /> Operational</span>
            <span className="hidden sm:inline text-zinc-400">⋯</span>
          </div>
        </div>
        <h1 className="mt-2 text-3xl font-bold">Overview</h1>
        <div className="mt-3 flex gap-2">
          <a href="/domains" className="rounded-full border border-zinc-200 bg-white px-3 py-1.5 text-xs font-medium hover:bg-zinc-50">Domains</a>
          <a href="/settings/mail" className="rounded-full border border-zinc-200 bg-white px-3 py-1.5 text-xs font-medium hover:bg-zinc-50">Mail settings</a>
        </div>

        {/* API Key row — Tavily style */}
        <div className="mt-6 rounded-2xl border border-zinc-200 bg-white p-4 shadow-sm">
          <div className="flex flex-wrap items-center gap-3">
            <span className="text-sm text-zinc-500">default</span>
            <span className="rounded bg-zinc-100 px-2 py-0.5 text-xs">dev</span>
            <span className="text-sm text-zinc-400">1</span>
            <div className="ml-auto flex flex-1 items-center gap-2 sm:ml-4">
              <div className="flex-1 rounded-full border border-zinc-200 bg-zinc-50 px-3 py-1.5 font-mono text-sm">
                {showRaw ? showRaw : (keys[0]?.key_masked || "avry-dev-************************")}
              </div>
              <button onClick={()=> setShowRaw(showRaw ? null : (keys[0] ? rawMap[keys[0].id] || keys[0].key_masked : null))} className="rounded p-1.5 text-zinc-500 hover:bg-zinc-100" title="Reveal">
                <svg className="h-4 w-4" fill="none" stroke="currentColor" strokeWidth="1.5" viewBox="0 0 24 24"><path strokeLinecap="round" strokeLinejoin="round" d="M2.036 12.322a1.012 1.012 0 0 1 0-.639C3.423 7.51 7.36 4.5 12 4.5c4.638 0 8.573 3.007 9.963 7.178.07.207.07.431 0 .639C20.577 16.49 16.64 19.5 12 19.5c-4.638 0-8.573-3.007-9.963-7.178Z"/><path strokeLinecap="round" strokeLinejoin="round" d="M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z"/></svg>
              </button>
              <button onClick={()=> navigator.clipboard?.writeText(showRaw || rawMap[keys[0]?.id] || "")} className="rounded p-1.5 text-zinc-500 hover:bg-zinc-100" title="Copy">
                <svg className="h-4 w-4" fill="none" stroke="currentColor" strokeWidth="1.5" viewBox="0 0 24 24"><path strokeLinecap="round" strokeLinejoin="round" d="M15.75 17.25v3.375c0 .621-.504 1.125-1.125 1.125h-9.75a1.125 1.125 0 0 1-1.125-1.125V7.875A1.125 1.125 0 0 1 4.875 6.75h9.75c.621 0 1.125.504 1.125 1.125v3.375m3-3V9.75m0 6v-3.375c0-.621-.504-1.125-1.125-1.125h-3.375m3 4.5-3-3 3-3"/></svg>
              </button>
              <button onClick={create} className="rounded p-1.5 text-zinc-500 hover:bg-zinc-100" title="Create">
                <svg className="h-4 w-4" fill="none" stroke="currentColor" strokeWidth="1.5" viewBox="0 0 24 24"><path strokeLinecap="round" strokeLinejoin="round" d="M16.862 4.487 19.5 7.125a1.875 1.875 0 0 1 0 2.65l-8.48 8.48a4.5 4.5 0 0 1-1.897 1.13l-2.685.8.8-2.685a4.5 4.5 0 0 1 1.13-1.897l8.48-8.48a1.875 1.875 0 0 1 2.65 0Z"/><path strokeLinecap="round" strokeLinejoin="round" d="M6 18 18 6"/></svg>
              </button>
              <button onClick={()=> keys[0] && del(keys[0].id)} className="rounded p-1.5 text-zinc-500 hover:bg-zinc-100" title="Delete">
                <svg className="h-4 w-4" fill="none" stroke="currentColor" strokeWidth="1.5" viewBox="0 0 24 24"><path strokeLinecap="round" strokeLinejoin="round" d="m14.74 9-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 0 1-2.244 2.077H8.084a2.25 2.25 0 0 1-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 0 0-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 0 1 3.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 0 0-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 0 0-7.5 0"/></svg>
              </button>
            </div>
          </div>
        </div>

        {/* Coupon */}
        <div className="mt-4 rounded-2xl border border-zinc-100 bg-white p-5 shadow-sm">
          <div className="text-sm font-semibold">Coupon</div>
          <div className="mt-1 text-sm text-zinc-500">Enter a coupon code to receive free API credits.</div>
          <div className="mt-3 flex gap-2">
            <input value={coupon} onChange={e=> setCoupon(e.target.value)} placeholder="Enter coupon code" className="max-w-xs flex-1 rounded-full border border-zinc-200 px-4 py-2 text-sm" />
            <button className="rounded-full bg-zinc-500 px-5 py-2 text-sm font-medium text-white">Apply</button>
          </div>
        </div>

        {/* Remote MCP */}
        <div className="mt-4 rounded-2xl border border-zinc-100 bg-white p-5 shadow-sm">
          <div className="text-sm font-semibold">Remote MCP</div>
          <div className="mt-1 text-sm leading-relaxed text-zinc-600">
            Connect directly to Aivory Mail's remote MCP server for a seamless experience without local installation or configuration. Select your desired API key and click the button below to generate the MCP connection URL. For examples on how to use the remote MCP, click <a href="/mcp" className="text-blue-600 underline">here</a>.
          </div>
          <div className="mt-4 flex flex-wrap items-center gap-3">
            <div className="flex items-center gap-2">
              <span className="text-sm font-medium">API Key</span>
              <select value={selectedKey} onChange={e=> setSelectedKey(e.target.value)} className="rounded-lg border border-zinc-200 bg-white px-3 py-2 text-sm">
                {keys.map((k:any)=> <option key={k.id} value={k.name}>{k.name}</option>)}
                {keys.length===0 && <option value="default">default</option>}
              </select>
            </div>
            <button onClick={generate} className="inline-flex items-center gap-2 rounded-full bg-zinc-900 px-5 py-2.5 text-sm font-semibold text-white hover:bg-black">
              <svg className="h-4 w-4" fill="none" stroke="currentColor" strokeWidth="1.5" viewBox="0 0 24 24"><path strokeLinecap="round" strokeLinejoin="round" d="M13.19 8.688a4.5 4.5 0 0 1 1.242 7.244l-4.5 4.5a4.5 4.5 0 0 1-6.364-6.364l1.757-1.757"/><path strokeLinecap="round" strokeLinejoin="round" d="M10.81 15.312a4.5 4.5 0 0 1-1.242-7.244l4.5-4.5a4.5 4.5 0 0 1 6.364 6.364l-1.757 1.757"/></svg>
              Generate MCP Link
            </button>
          </div>
          {mcpLink && (
            <div className="mt-3 flex gap-2">
              <input readOnly value={mcpLink} className="flex-1 rounded-lg border border-zinc-200 bg-zinc-50 px-3 py-2 font-mono text-xs" />
              <button onClick={()=> navigator.clipboard?.writeText(mcpLink)} className="rounded-lg border border-zinc-200 bg-white px-3 py-2 text-xs hover:bg-zinc-50">Copy</button>
            </div>
          )}
          <div className="mt-2 text-xs text-zinc-400">MCP: POST https://mail.aivory.uk/mcp with Authorization: Bearer &lt;api_key&gt; or ?api_key=</div>
        </div>

        {/* Footer contact */}
        <div className="mt-4 flex items-center justify-between rounded-2xl border border-zinc-100 bg-white p-4">
          <span className="text-sm">Have any questions, feedback or need support? We'd love to hear from you!</span>
          <a href="mailto:hello@aivory.uk" className="rounded-full bg-zinc-900 px-5 py-2.5 text-sm font-semibold text-white">Contact us</a>
        </div>

        <div className="mt-6 flex justify-between text-xs text-zinc-400">
          <span>© 2026 Aivory Mail. All Rights Reserved.</span>
          <span className="flex gap-4">Github · Privacy · Terms</span>
        </div>
      </div>
    </div>
  );
}
