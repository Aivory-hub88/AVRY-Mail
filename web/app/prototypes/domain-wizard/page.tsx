"use client";
import { useEffect, useRef, useState } from "react";

function Ico({ d, size = 16, cls = "" }: { d: string; size?: number; cls?: string }) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.65} strokeLinecap="round" strokeLinejoin="round" className={cls} aria-hidden><path d={d} /></svg>;
}
const P = {
  globe: "M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20z M2 12h20 M12 2a15 15 0 0 1 0 20 M12 2a15 15 0 0 0 0 20",
  check: "M5 13l4 4L19 7",
  alert: "M12 9v4 M12 17h.01 M10.3 3.3L3.3 18a2 2 0 0 0 1.7 3h13a2 2 0 0 0 1.7-3L13.7 3.3a2 2 0 0 0-3.4 0z",
  copy: "M8 5H6a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2v-1 M8 5a2 2 0 0 0 2 2h2a2 2 0 0 0 2-2M8 5a2 2 0 0 1 2-2h2a2 2 0 0 1 2 2",
  link: "M10 13a5 5 0 0 1 0-7l1-1a5 5 0 0 1 7 7l-1 1 M14 11a5 5 0 0 1 0 7l-1 1a5 5 0 0 1-7-7l1-1",
  settings: "M3 6h18 M3 12h18 M3 18h18 M7 6a2 2 0 1 0 0 4 2 2 0 0 0 0-4z M14 12a2 2 0 1 0 0 4 2 2 0 0 0 0-4z M9 18a2 2 0 1 0 0 4 2 2 0 0 0 0-4z",
  key: "M9 8V6a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2 M5 11h14v4a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2v-4z M12 15v7 M12 22c0 1.2-1 2-2 1.2 M12 22c0 1.2 1 2 2 1.2",
  calendar: "M8 2v4 M16 2v4 M3 8h18 M5 4h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z",
  mail: "M4 4h16a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z M22 6l-10 7L2 6",
  search: "M21 21l-4.35-4.35 M11 19a8 8 0 1 1 0-16 8 8 0 0 1 0 16z",
  plus: "M12 5v14 M5 12h14",
};

// Emil stagger helper — 40ms per row, transform/opacity only
function StaggerRow({ i, children }: { i: number; children: React.ReactNode }) {
  return <div className="animate-row" style={{ animationDelay: `${i * 40}ms` }}>{children}</div>;
}

function Chip({ ok, label }: { ok: boolean; label: string }) {
  return <span className={`inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-semibold ring-1 ${ok ? "bg-emerald-50 text-emerald-700 ring-emerald-200" : "bg-amber-50 text-amber-700 ring-amber-200"}`}>{ok ? <Ico d={P.check} size={10} /> : <Ico d={P.alert} size={10} />}{label}</span>;
}

function AutoCF() {
  const [host, setHost] = useState("aivory.uk");
  const [zone, setZone] = useState("aivory.uk");
  return (
    <div className="space-y-4">
      <div className="rounded-2xl border border-[#e8e0c8] bg-[#fefcf6] p-5 shadow-sm">
        <div className="text-xs font-semibold tracking-widest text-zinc-400 uppercase">Add domain — Auto (Mailflare)</div>
        <p className="mt-1 text-sm text-zinc-600">Domains must be on your Cloudflare account. Routing + Sending DNS provisioned automatically.</p>
        <div className="mt-4 flex gap-2">
          <input value={host} onChange={e=>setHost(e.target.value)} placeholder="example.com" className="flex-1 rounded-full border border-[#e8e0c8] bg-[#f8f6ef] px-4 py-2 text-sm focus:border-[#005a5e] focus:bg-[#fefcf6] focus:outline-none" />
          <button className="rounded-full bg-[#005a5e] px-5 py-2 text-sm font-semibold text-white shadow hover:bg-[#00454a] active:scale-[0.97] transition-[transform] duration-150 ease-out">Add domain</button>
        </div>
        <div className="mt-3 rounded-xl bg-[#f0ece0] px-3 py-2 text-xs text-zinc-600">Zone detected: <span className="font-semibold text-[#005a5e]">{zone}</span> via <code className="rounded bg-[#fefcf6] px-1">findZoneByHostname</code> loop • catch-all → {`{CF_EMAIL_WORKER_NAME}`}</div>
      </div>
      <div className="rounded-2xl border border-[#e8e0c8] bg-[#fefcf6] p-5 shadow-sm">
        <div className="flex items-center justify-between">
          <span className="text-sm font-semibold">{host}</span>
          <span className="flex gap-1">
            <Chip ok label="active" />
            <Chip ok label="routing ✓" />
            <Chip ok label="sending ✓" />
          </span>
        </div>
        <div className="mt-3 grid gap-2">
          {[
            { t: "MX", v: "route.mx.cloudflare.net", ok: true },
            { t: "TXT", v: "v=spf1 include:_spf.mx.cloudflare.net ~all", ok: true },
            { t: "DKIM", v: "cf2024-1._domainkey TXT → CF", ok: true },
          ].map((r,i)=>(
            <StaggerRow key={r.t} i={i}><div className="flex items-center justify-between rounded-xl border border-[#f0ece0] bg-[#f8f6ef] px-3 py-2 text-xs"><span className="font-mono font-semibold">{r.t}</span><span className="truncate text-zinc-500">{r.v}</span><Ico d={P.check} size={12} cls="text-emerald-600" /></div></StaggerRow>
          ))}
        </div>
        <div className="mt-2 text-[11px] text-emerald-700">No missing records — configured automatically. No TXT copy.</div>
      </div>
    </div>
  );
}

function ManualHash() {
  const [host, setHost] = useState("example.com");
  const hash = `avry-verification=${host.split(".")[0]}-zb${Math.random().toString(36).slice(2,8)}`;
  const [copied, setCopied] = useState(false);
  return (
    <div className="space-y-4">
      <div className="rounded-2xl border border-[#e8e0c8] bg-[#fefcf6] p-5 shadow-sm">
        <div className="text-xs font-semibold tracking-widest text-zinc-400 uppercase">Add domain — Manual (Zoho)</div>
        <p className="mt-1 text-sm text-zinc-600">Works with any DNS. Copy hash, add TXT/CNAME, then Verify.</p>
        <div className="mt-4 flex gap-2">
          <input value={host} onChange={e=>setHost(e.target.value)} placeholder="example.com" className="flex-1 rounded-full border border-[#e8e0c8] bg-[#f8f6ef] px-4 py-2 text-sm focus:border-[#005a5e] focus:outline-none" />
          <button className="rounded-full bg-[#005a5e] px-5 py-2 text-sm font-semibold text-white hover:bg-[#00454a] active:scale-[0.97] transition-transform">Add</button>
        </div>
        <div className="mt-4 rounded-xl border border-amber-200 bg-amber-50 p-3">
          <div className="text-xs font-semibold text-amber-900">TXT method — steps</div>
          <ol className="mt-1 list-decimal space-y-0.5 pl-4 text-xs text-amber-800">
            <li>Copy hash below</li><li>DNS Manager → Add TXT @ → paste Value → TTL minimum</li><li>Wait 1–2h → Click Verify TXT</li>
          </ol>
          <div className="mt-2 flex items-center gap-2 rounded-full border border-amber-200 bg-[#fefcf6] px-3 py-1.5 font-mono text-xs">
            <span className="flex-1 truncate">{hash}</span>
            <button onClick={()=>{navigator.clipboard.writeText(hash); setCopied(true); setTimeout(()=>setCopied(false),1200);}} className="rounded-full bg-zinc-900 px-2.5 py-1 text-[11px] font-semibold text-white hover:bg-black active:scale-95 transition-transform"><span className="flex items-center gap-1"><Ico d={P.copy} size={11} />{copied ? "Copied" : "Copy"}</span></button>
          </div>
          <div className="mt-2 flex gap-2 text-[11px]"><span className="rounded-full bg-amber-100 px-2 py-0.5 ring-1 ring-amber-200">TTL: minimum</span><span className="rounded-full bg-[#fefcf6] px-2 py-0.5 border">Provider: Cloudflare / GoDaddy / cPanel</span></div>
        </div>
        <div className="mt-3 flex gap-2">
          <button className="flex-1 rounded-full bg-[#005a5e] px-4 py-2 text-sm font-semibold text-white">Verify TXT</button>
          <button className="rounded-full border border-[#e8e0c8] bg-[#fefcf6] px-4 py-2 text-xs">Verify CNAME: zb… → zmverify</button>
        </div>
      </div>
      <div className="rounded-2xl border border-[#e8e0c8] bg-[#fefcf6] p-5">
        <div className="text-xs font-semibold">View DNS Records — Status</div>
        <div className="mt-2 overflow-hidden rounded-xl border border-[#e8e0c8]">
          <div className="grid grid-cols-[80px_1fr_90px] gap-px bg-[#e8e0c8] text-[11px] font-semibold">
            <div className="bg-[#f8f6ef] px-2 py-1.5">Type</div><div className="bg-[#f8f6ef] px-2 py-1.5">Value</div><div className="bg-[#f8f6ef] px-2 py-1.5">Status</div>
          </div>
          {[
            ["MX", "mx.zoho.com 10", "verified"],
            ["MX", "mx2.zoho.com 20", "pending"],
            ["TXT", "v=spf1 include:zohomail.com ~all", "verified"],
            ["TXT", "zoho._domainkey DKIM", "yet to configure"],
          ].map((r,i)=>(
            <StaggerRow key={i} i={i}><div className="grid grid-cols-[80px_1fr_90px] gap-px bg-[#e8e0c8] text-xs">
              <div className="bg-[#fefcf6] px-2 py-1.5 font-mono">{r[0]}</div><div className="bg-[#fefcf6] px-2 py-1.5 truncate">{r[1]}</div><div className="bg-[#fefcf6] px-2 py-1.5"><Chip ok={r[2]==="verified"} label={r[2]} /></div>
            </div></StaggerRow>
          ))}
        </div>
      </div>
    </div>
  );
}

function Hybrid() {
  const [host, setHost] = useState("aivory.uk");
  const [mode, setMode] = useState<"auto"|"manual">("auto");
  const isCFHint = host.endsWith("aivory.uk") || host.endsWith(".com");
  return (
    <div className="space-y-4">
      <div className="rounded-2xl border border-[#e8e0c8] bg-[#fefcf6] p-5 shadow-sm">
        <div className="text-xs font-semibold tracking-widest text-zinc-400 uppercase">Add domain — Pilih metode</div>
        <p className="mt-1 text-xs text-zinc-500">Zoho kaya manual hash, Mailflare kaya auto CF — user pilih, bukan auto-detect. Rekomendasi: {isCFHint ? "Auto" : "Manual"}.</p>
        <div className="mt-3 inline-flex rounded-full border border-[#e8e0c8] bg-[#f8f6ef] p-1 text-xs">
          <button onClick={()=>setMode("auto")} className={`rounded-full px-4 py-1.5 font-semibold transition ${mode==="auto" ? "bg-[#005a5e] text-white shadow" : "text-zinc-600 hover:bg-[#fefcf6]"}`}>Auto — Cloudflare</button>
          <button onClick={()=>setMode("manual")} className={`rounded-full px-4 py-1.5 font-semibold transition ${mode==="manual" ? "bg-[#005a5e] text-white shadow" : "text-zinc-600 hover:bg-[#fefcf6]"}`}>Manual — TXT hash</button>
        </div>
        <div className="mt-3 flex gap-2">
          <input value={host} onChange={e=>setHost(e.target.value)} placeholder="example.com" className="flex-1 rounded-full border border-[#e8e0c8] bg-[#f8f6ef] px-4 py-2 text-sm focus:border-[#005a5e] focus:outline-none" />
          <button className="rounded-full bg-[#005a5e] px-5 py-2 text-sm font-semibold text-white active:scale-[0.97] transition-transform">{mode==="auto" ? "Provision auto" : "Generate hash"}</button>
        </div>
        <div className={`mt-3 rounded-xl px-3 py-2 text-xs ${mode==="auto" ? "bg-[#f0ece0]" : "border border-amber-200 bg-amber-50 font-mono"}`}>
          {mode==="auto" ? <span>Auto via <code className="rounded bg-[#fefcf6] px-1">GET /zones?name=</code> → <code>enableEmailRouting</code> → <code>PUT catch_all worker</code> → no copy. Butuh CF_TOKEN dengan Zone Read + Email Routing Edit.</span> : <span>avry-verification={host}-zb… → TXT @ → Verify. Works di any DNS (GoDaddy/cPanel/Cloudflare manual).</span>}
        </div>
        <div className="mt-2 flex gap-1.5">
          <span className="rounded-full bg-[#f0ece0] px-2.5 py-1 text-[11px]"><Ico d={P.globe} size={10} /> {host}</span>
          <span className={`rounded-full px-2 py-0.5 text-[11px] ring-1 ${mode==="auto" ? "bg-emerald-50 text-emerald-700 ring-emerald-200" : "bg-amber-50 text-amber-700 ring-amber-200"}`}>{mode==="auto" ? "Auto" : "Manual"}</span>
        </div>
      </div>
      <div className="rounded-2xl border border-[#e8e0c8] bg-[#fefcf6] p-5 shadow-sm">
        <div className="flex items-center justify-between">
          <span className="text-sm font-semibold">{host} — {mode==="auto" ? "Auto" : "Manual"}</span>
          <span className="flex gap-1"><Chip ok label={mode==="auto" ? "active" : "pending"} /><Chip ok={mode==="auto"} label={mode==="auto" ? "routing ✓" : "routing —"} /><Chip ok={false} label="DKIM —" /></span>
        </div>
        <div className="mt-3 overflow-hidden rounded-xl border border-[#e8e0c8]">
          <div className="grid grid-cols-[70px_1fr_110px_90px] gap-px bg-[#e8e0c8] text-[11px] font-semibold"><div className="bg-[#f8f6ef] px-2 py-1.5">Type</div><div className="bg-[#f8f6ef] px-2 py-1.5">Host / Value</div><div className="bg-[#f8f6ef] px-2 py-1.5">Priority</div><div className="bg-[#f8f6ef] px-2 py-1.5">Status</div></div>
          {[
            ["MX", "@ → mx.aivory.uk / route.mx.cloudflare.net", "10", mode==="auto" ? "verified" : "yet to point"],
            ["TXT", "@ v=spf1 include:…", "—", mode==="auto" ? "verified" : "unverified"],
            ["TXT", "aivory._domainkey (DKIM)", "—", "yet to configure"],
            ["TXT", "_dmarc v=DMARC1; p=quarantine;", "—", "optional"],
          ].map((r,i)=>(
            <StaggerRow key={i} i={i}><div className="grid grid-cols-[70px_1fr_110px_90px] gap-px bg-[#e8e0c8] text-xs"><div className="bg-[#fefcf6] px-2 py-1.5 font-mono">{r[0]}</div><div className="bg-[#fefcf6] px-2 py-1.5 truncate">{r[1]}</div><div className="bg-[#fefcf6] px-2 py-1.5">{r[2]}</div><div className="bg-[#fefcf6] px-2 py-1.5"><Chip ok={r[3]==="verified"} label={r[3]} /></div></div></StaggerRow>
          ))}
        </div>
        <div className="mt-3 flex gap-2">
          <button className="flex-1 rounded-full bg-[#005a5e] px-4 py-2 text-sm font-semibold text-white hover:bg-[#00454a] active:scale-[0.97] transition-transform">{mode==="auto" ? "Re-check DNS (CF API)" : "Verify TXT"}</button>
          <button className="rounded-full border border-[#e8e0c8] bg-[#fefcf6] px-4 py-2 text-xs">Send to DNS admin ✉</button>
          <button className="rounded-full border border-[#e8e0c8] bg-[#fefcf6] px-3 py-1.5 text-xs">Toolkit lookup</button>
        </div>
        <div className="mt-2 text-[11px] text-zinc-400">{mode==="auto" ? "CF auto — no copy, check missing via API" : "Manual — copy hash, TTL minimum • Propagation 1–2h TXT, 4–48h DKIM"}</div>
      </div>
    </div>
  );
}

export default function Page() {
  const variants = [
    { key: "auto", label: "Auto CF", el: <AutoCF /> },
    { key: "manual", label: "Manual Hash", el: <ManualHash /> },
    { key: "hybrid", label: "Hybrid", el: <Hybrid /> },
  ] as const;
  const [idx, setIdx] = useState(2);
  const pickerRef = useRef<HTMLElement>(null);
  const highlightRef = useRef<HTMLSpanElement>(null);
  useEffect(()=>{ const v=parseInt(new URLSearchParams(window.location.search).get("v")||"3",10); if(v>=1&&v<=variants.length) setIdx(v-1); requestAnimationFrame(()=>requestAnimationFrame(()=>pickerRef.current?.setAttribute("data-ready",""))); },[]);
  useEffect(()=>{
    const p=pickerRef.current, h=highlightRef.current; if(!p||!h) return;
    const items=[...p.querySelectorAll<HTMLButtonElement>(".proto-picker-item:not(.proto-picker-replay)")];
    const el=items[idx]; if(!el) return; h.style.width=el.offsetWidth+"px"; h.style.transform=`translateX(${el.offsetLeft}px)`;
    const url=new URL(window.location.href); url.searchParams.set("v",String(idx+1)); window.history.replaceState(null,"",url);
  },[idx]);
  useEffect(()=>{
    const onKey=(e:KeyboardEvent)=>{ if(/^(INPUT|TEXTAREA|SELECT)$/.test((e.target as HTMLElement).tagName)||(e.target as HTMLElement).isContentEditable) return; if(e.metaKey||e.ctrlKey||e.altKey) return; const n=parseInt(e.key,10); if(n>=1&&n<=variants.length) setIdx(n-1); else if(e.key==="ArrowRight") setIdx(i=>(i+1)%variants.length); else if(e.key==="ArrowLeft") setIdx(i=>(i-1+variants.length)%variants.length); };
    window.addEventListener("keydown",onKey); return()=>window.removeEventListener("keydown",onKey);
  },[]);
  return (
    <div className="min-h-dvh bg-[#f8f6ef] p-6">
      <style>{`.animate-row{opacity:0;transform:translateY(6px);animation:rowIn 300ms cubic-bezier(0.23,1,0.32,1) forwards}@keyframes rowIn{to{opacity:1;transform:translateY(0)}}@media(prefers-reduced-motion:reduce){.animate-row{animation:none;opacity:1;transform:none}}`}</style>
      <div className="mx-auto max-w-[1100px]">
        <div className="mb-4">
          <h1 className="text-xl font-bold tracking-tight text-[#202124]">Domain Wizard — Mailflare + Zoho combine</h1>
          <p className="mt-1 text-sm text-zinc-500">Tidy layout + Emil motion • Auto CF (Mailflare) • Manual hash (Zoho) • Hybrid (Aivory combine) • Keep 3-pane Zoho tab model</p>
        </div>
        <div className="rounded-3xl border border-[#e8e0c8] bg-[#f8f6ef] p-4 shadow-sm">
          <div className="flex items-center gap-2 border-b border-[#e8e0c8] bg-[#fefcf6] px-3 py-2 text-xs">
            <span className="flex h-6 w-6 items-center justify-center rounded bg-[#005a5e] text-white"><Ico d={P.globe} size={12} cls="text-white" /></span>
            <span className="font-semibold">Domains</span>
            <span className="ml-auto flex gap-1">
              <span className="rounded-full bg-emerald-50 px-2 py-0.5 text-[11px] ring-1 ring-emerald-200">active</span>
              <span className="rounded-full bg-[#fefcf6] px-2 py-0.5 text-[11px] border">MX verified</span>
            </span>
          </div>
          <div key={idx} className="bg-[#fefcf6] p-4">{variants[idx].el}</div>
        </div>
        <div className="mt-4 grid grid-cols-1 gap-3 sm:grid-cols-3 text-xs">
          <div className="rounded-xl border border-[#e8e0c8] bg-[#fefcf6] p-3"><div className="font-bold">Auto CF</div><div className="mt-1 text-zinc-600">Mailflare axis: CF zone loop, catch-all worker, no copy. Best for CF domains — 10s.</div></div>
          <div className="rounded-xl border border-[#e8e0c8] bg-[#fefcf6] p-3"><div className="font-bold">Manual Hash</div><div className="mt-1 text-zinc-600">Zoho axis: zb hash + TXT/CNAME/HTML, TTL min, Verify button, provider guide. Universal.</div></div>
          <div className="rounded-xl border border-[#005a5e] bg-[#f0ece0] p-3 ring-1 ring-[#005a5e]"><div className="font-bold">Hybrid — pick this</div><div className="mt-1 text-zinc-600">Detect CF → auto else hash + View DNS Records table (Zoho) + Missing badges (Mailflare). Aivory combine.</div></div>
        </div>
      </div>
      <nav ref={pickerRef} className="proto-picker" aria-label="Prototype variants">
        <span ref={highlightRef} className="proto-picker-highlight" aria-hidden="true" />
        {variants.map((v,i)=>(<button key={v.key} className="proto-picker-item" data-active={i===idx ? "" : undefined} aria-current={i===idx ? "true" : undefined} onClick={()=>setIdx(i)}>{v.label}</button>))}
      </nav>
      <style>{`.proto-picker{position:fixed;bottom:24px;left:50%;transform:translateX(-50%);z-index:2147483647;display:flex;align-items:center;gap:2px;padding:4px;border-radius:999px;background:rgba(10,10,10,.82);backdrop-filter:blur(12px) saturate(1.4);box-shadow:0 0 0 1px rgba(255,255,255,.08) inset,0 8px 24px rgba(0,0,0,.24),0 2px 6px rgba(0,0,0,.12);font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;font-size:13px;line-height:1;user-select:none}.proto-picker-highlight{position:absolute;top:4px;left:0;height:28px;border-radius:999px;background:rgba(255,255,255,.12);will-change:transform}.proto-picker[data-ready] .proto-picker-highlight{transition:transform 250ms cubic-bezier(0.23,1,0.32,1),width 250ms cubic-bezier(0.23,1,0.32,1)}@media(prefers-reduced-motion:reduce){.proto-picker[data-ready] .proto-picker-highlight{transition:none}}.proto-picker-item{position:relative;display:flex;align-items:center;height:28px;padding:0 12px;border:0;border-radius:999px;background:transparent;color:rgba(255,255,255,.55);font:inherit;cursor:pointer;transition:color 150ms ease-out}.proto-picker-item:hover{color:rgba(255,255,255,.85)}.proto-picker-item:active{transform:scale(0.97)}.proto-picker-item[data-active]{color:#fff}`}</style>
    </div>
  );
}
