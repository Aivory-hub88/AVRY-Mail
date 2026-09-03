"use client";
import { useEffect, useState, useRef } from "react";
const API = process.env.NEXT_PUBLIC_MAIL_API || "http://localhost:8095";
type Ev = { id: string; calendar: string; title: string; description?: string; start_at: string; end_at: string; guests?: string; color?: string; location?: string; conferencing?: string; conferencing_link?: string };
type Mailbox = { id: string; address: string; display_name?: string | null };

const CATEGORIES = [
  { name: "My calendar", color: "bg-blue-600", dot: "bg-blue-600", text: "text-blue-600" },
  { name: "Birthdays", color: "bg-emerald-500", dot: "bg-emerald-500", text: "text-emerald-600" },
  { name: "Tasks", color: "bg-violet-600", dot: "bg-violet-600", text: "text-violet-600" },
  { name: "Holidays in Indonesia", color: "bg-emerald-600", dot: "bg-emerald-600", text: "text-emerald-600" },
];

function toLocalDate(iso: string) { return new Date(iso); }

export default function CalendarPage() {
  const [weekStart, setWeekStart] = useState(() => { const d = new Date(); d.setHours(0,0,0,0); d.setDate(d.getDate()-d.getDay()); return d; });
  const [view, setView] = useState<"Week"|"Day"|"Month">("Week");
  const [events, setEvents] = useState<Ev[]>([]);
  const [visible, setVisible] = useState<Record<string, boolean>>({ "My calendar": true, "Birthdays": true, "Tasks": true, "Holidays in Indonesia": true });
  const [miniMonth, setMiniMonth] = useState(() => new Date());
  const [searchPeople, setSearchPeople] = useState("");
  const [selected, setSelected] = useState<Ev|null>(null);
  const [showCreate, setShowCreate] = useState(false);
  const [createAt, setCreateAt] = useState<{day: Date, hour: number} | null>(null);
  const [form, setForm] = useState({ title: "", calendar: "My calendar", start_at: "", end_at: "", guests: "", description: "", location: "", conferencing: "none", conferencing_link: "", color: "blue", recurring: "never", notifications: "10m" });
  const [eventTypes, setEventTypes] = useState<any[]>([]);
  const [mailboxes, setMailboxes] = useState<Mailbox[]>([]);
  const [mailboxId, setMailboxId] = useState<string>("");

  useEffect(()=>{
    fetch(`${API}/v1/mailboxes`).then(r=>r.json()).then(j=>{
      const list: Mailbox[] = j.data||[];
      setMailboxes(list);
      const saved = typeof window!=="undefined" ? window.localStorage.getItem("aivory_calendar_mailbox_id") : null;
      const initial = list.find(m=> m.id===saved)?.id || list[0]?.id || "";
      setMailboxId(initial);
    }).catch(()=>{});
  },[]);
  function selectMailbox(id: string){ setMailboxId(id); try{ window.localStorage.setItem("aivory_calendar_mailbox_id", id); }catch{} }

  const days = Array.from({length: view==="Day"?1:7}, (_,i)=> { const d=new Date(weekStart); if(view==="Day"){ const today=new Date(); today.setHours(0,0,0,0); return today; } d.setDate(weekStart.getDate()+i); return d; });
  const hours = Array.from({length:14}, (_,i)=> i+7); // 7AM..8PM
  const monthLabel = view==="Day" ? days[0].toLocaleString('en',{month:'long', day:'numeric', year:'numeric'}) : `${days[0].toLocaleString('en',{month:'short'})} – ${days[days.length-1].toLocaleString('en',{month:'short', year:'numeric'})}`;

  function shift(dir:number){ const n=new Date(weekStart); n.setDate(n.getDate()+dir*(view==="Day"?1:7)); setWeekStart(n); }
  function goToday(){ const d=new Date(); d.setHours(0,0,0,0); d.setDate(d.getDate()-d.getDay()); setWeekStart(d); }

  async function fetchEvents(){
    if(!mailboxId) return;
    const from = new Date(weekStart); from.setDate(from.getDate()-1);
    const to = new Date(weekStart); to.setDate(to.getDate()+8);
    try{
      const r = await fetch(`${API}/v1/calendar/events?mailbox_id=${encodeURIComponent(mailboxId)}&from=${encodeURIComponent(from.toISOString())}&to=${encodeURIComponent(to.toISOString())}`);
      const j = await r.json();
      if(j.success) setEvents(j.data||[]);
    }catch{}
  }
  useEffect(()=>{ fetchEvents(); }, [weekStart, mailboxId]);
  useEffect(()=>{
    fetch(`${API}/v1/calendar/event-types`).then(r=>r.json()).then(j=>{ const list=j.data?.data||j.data||[]; if(Array.isArray(list)) setEventTypes(list.slice(0,4)); }).catch(()=>{});
  },[]);

  function openCreate(day:Date, hour:number){
    const s=new Date(day); s.setHours(hour,0,0,0);
    const e=new Date(s); e.setHours(hour+1);
    setCreateAt({day, hour});
    setForm({ title:"", calendar:"My calendar", start_at:s.toISOString().slice(0,16), end_at:e.toISOString().slice(0,16), guests:"", description:"", location:"", conferencing:"none", conferencing_link:"", color: CATEGORIES.find(c=>c.name==="My calendar")?.color || "blue", recurring:"never", notifications:"10m" });
    setShowCreate(true);
  }
  async function saveEvent(){
    if(!form.title.trim() || !mailboxId) return;
    let confLink = form.conferencing_link; if(form.conferencing !== "none" && !confLink){ if(form.conferencing==="google-meet") confLink="https://meet.google.com/new"; else if(form.conferencing==="zoom") confLink="https://zoom.us/start"; else if(form.conferencing==="teams") confLink="https://teams.live.com/meet"; }
    const payload = { mailbox_id: mailboxId, title: form.title, calendar: form.calendar, start_at: new Date(form.start_at).toISOString(), end_at: new Date(form.end_at).toISOString(), guests: form.guests.split(",").map(s=>s.trim()).filter(Boolean), description: form.description, location: form.location, conferencing: form.conferencing, conferencing_link: confLink, color: form.color, recurring: form.recurring, notifications: form.notifications };
    try{
      await fetch(`${API}/v1/calendar/events`, {method:"POST", headers:{"content-type":"application/json"}, body: JSON.stringify(payload)});
      setShowCreate(false);
      fetchEvents();
    }catch{}
  }

  const filtered = events.filter(e=> visible[e.calendar] !== false).filter(e=>{
    if(!searchPeople.trim()) return true;
    const q=searchPeople.toLowerCase();
    return e.title.toLowerCase().includes(q) || (e.guests||"").toLowerCase().includes(q);
  });

  return (
    <div className="flex h-screen flex-col bg-[#fefcf6] text-[#202124]">
      <header className="flex h-16 shrink-0 items-center gap-3 border-b border-[#e8e0c8] px-3">
        <button className="rounded p-2 hover:bg-zinc-100">☰</button>
        <div className="flex items-center gap-2"><span className="flex h-8 w-8 items-center justify-center rounded bg-blue-600 text-white text-xs font-bold">31</span><span className="text-xl font-normal">Calendar</span></div>
        <button onClick={goToday} className="ml-4 rounded-lg border border-zinc-300 px-4 py-1.5 text-sm font-medium hover:bg-[#f8f6ef]">Today</button>
        <div className="flex gap-1"><button onClick={()=> shift(-1)} className="rounded-lg p-1.5 hover:bg-zinc-100">‹</button><button onClick={()=> shift(1)} className="rounded-lg p-1.5 hover:bg-zinc-100">›</button></div>
        <span className="text-xl font-normal">{monthLabel}</span>
        <div className="ml-auto flex items-center gap-2">
          <div className="relative">
            <select value={view} onChange={e=> setView(e.target.value as any)} className="rounded-lg border border-zinc-300 bg-[#fefcf6] px-4 py-1.5 text-sm">
              <option>Week</option><option>Day</option><option>Month</option>
            </select>
          </div>
          <a href="https://book.aivory.uk/book/aivory-call" target="_blank" className="hidden sm:inline-flex rounded-lg bg-[#005a5e] px-4 py-1.5 text-sm font-medium text-white hover:bg-[#00454a]">Book via Aivory Calendar ↗</a>
          <a href="https://mail.aivory.uk/calendar" className="hidden sm:inline-flex rounded-lg bg-[#e6f3f0] px-4 py-1.5 text-sm font-medium text-[#005a5e]">mail.aivory.uk/calendar</a>
          {mailboxes.length>1 ? (
            <select value={mailboxId} onChange={e=> selectMailbox(e.target.value)} title="Switch mailbox — each mailbox has its own isolated calendar" className="rounded-lg border border-zinc-300 bg-[#fefcf6] px-3 py-1.5 text-xs font-medium">
              {mailboxes.map(m=> <option key={m.id} value={m.id}>{m.display_name || m.address}</option>)}
            </select>
          ) : (
            <span className="flex h-8 w-8 items-center justify-center rounded-lg bg-amber-200 text-xs">{(mailboxes[0]?.display_name || mailboxes[0]?.address || "?").slice(0,2).toUpperCase()}</span>
          )}
        </div>
      </header>

      <div className="flex min-h-0 flex-1">
        <aside className="hidden w-[260px] shrink-0 flex-col border-r border-[#e8e0c8] bg-[#fefcf6] p-3 lg:flex">
          <div className="relative">
            <button onClick={()=> {
              const d=new Date(); d.setHours(12,0,0,0);
              const day = new Date(); day.setHours(0,0,0,0);
              // create at today 9AM
              openCreate(day, 9);
            }} className="flex items-center gap-2 rounded-lg border border-[#e8e0c8] bg-[#fefcf6] px-4 py-2.5 text-sm font-medium shadow hover:bg-[#f8f6ef]">+ Create ▾</button>
          </div>

          <div className="mt-4">
            <div className="flex items-center justify-between text-sm font-medium">
              <span>{miniMonth.toLocaleString('en',{month:'long', year:'numeric'})}</span>
              <span className="flex gap-1"><button onClick={()=> setMiniMonth(d=>{ const n=new Date(d); n.setMonth(n.getMonth()-1); return n; })} className="rounded p-1 hover:bg-zinc-100">‹</button><button onClick={()=> setMiniMonth(d=>{ const n=new Date(d); n.setMonth(n.getMonth()+1); return n; })} className="rounded p-1 hover:bg-zinc-100">›</button></span>
            </div>
            <div className="mt-2 grid grid-cols-7 gap-1 text-center text-xs text-zinc-500">
              {["S","M","T","W","T","F","S"].map(d=> <span key={d} className="py-1">{d}</span>)}
              {Array.from({length: 35}, (_,i)=> {
                const first = new Date(miniMonth.getFullYear(), miniMonth.getMonth(), 1);
                const start = new Date(first); start.setDate(1 - first.getDay());
                const d = new Date(start); d.setDate(start.getDate()+i);
                const isToday = d.toDateString()===new Date().toDateString();
                const isCurrent = d.getMonth()===miniMonth.getMonth();
                const isSelected = d.toDateString()===weekStart.toDateString();
                return <button key={i} onClick={()=> { const n=new Date(d); n.setHours(0,0,0,0); setWeekStart(new Date(n.getFullYear(), n.getMonth(), n.getDate()-n.getDay())); }} className={`flex h-7 w-7 items-center justify-center rounded-lg text-xs ${isToday ? "bg-[#005a5e] text-white" : isSelected ? "bg-[#005a5e] text-white" : isCurrent ? "hover:bg-zinc-100 text-zinc-700" : "text-zinc-400"}`}>{d.getDate()}</button>;
              })}
            </div>
          </div>

          <div className="mt-3">
            <div className="relative">
              <span className="pointer-events-none absolute left-2 top-2 text-zinc-400">👥</span>
              <input value={searchPeople} onChange={e=> setSearchPeople(e.target.value)} placeholder="Search for people" className="w-full rounded-lg bg-zinc-100 py-2 pl-8 pr-3 text-sm placeholder:text-zinc-500 focus:bg-[#fefcf6] focus:outline-none focus:ring-1 focus:ring-zinc-300" />
            </div>
            {searchPeople && <div className="mt-1 text-xs text-zinc-500">{filtered.length} result{filtered.length!==1?"s":""} for "{searchPeople}"</div>}
          </div>

          <div className="mt-4 space-y-3 overflow-y-auto">
            <div className="flex items-center justify-between text-sm font-semibold"><span>Booking pages</span><a href="https://book.aivory.uk" target="_blank" className="rounded p-1 hover:bg-zinc-100">+</a></div>
            {eventTypes.length>0 && <div className="space-y-1">{eventTypes.map((e:any)=> <a key={e.slug||e.id} href={`https://book.aivory.uk/${e.slug}`} target="_blank" className="flex justify-between rounded px-2 py-1 text-xs hover:bg-[#f8f6ef]"><span className="truncate">{e.title||e.slug}</span><span className="text-blue-600">↗</span></a> )}</div>}
            <div className="space-y-1">
              <div className="flex items-center justify-between text-xs font-semibold text-zinc-700"><span>My calendars</span><span className="text-zinc-400">∧</span></div>
              {CATEGORIES.slice(0,3).map(c=> (
                <label key={c.name} className="flex items-center gap-2 text-sm">
                  <input type="checkbox" checked={visible[c.name]!==false} onChange={e=> setVisible(v=> ({...v, [c.name]: e.target.checked}))} className="rounded" />
                  <span className={`h-3 w-3 rounded-sm ${c.color}`} />
                  <span className="flex-1 truncate">{c.name}</span>
                </label>
              ))}
              <div className="flex items-center justify-between pt-1 text-xs font-semibold text-zinc-700"><span>Other calendars</span><span className="text-zinc-400">∧</span></div>
              <label className="flex items-center gap-2 text-sm">
                <input type="checkbox" checked={visible["Holidays in Indonesia"]!==false} onChange={e=> setVisible(v=> ({...v, ["Holidays in Indonesia"]: e.target.checked}))} className="rounded" />
                <span className="h-3 w-3 rounded-sm bg-emerald-600" />
                <span className="flex-1 truncate">Holidays in Indonesia</span>
              </label>
            </div>
          </div>
        </aside>

        <div className="flex min-w-0 flex-1 flex-col">
          <div className="grid border-b border-[#e8e0c8] text-center text-xs" style={{gridTemplateColumns:`60px repeat(${days.length},1fr)`}}>
            <div className="border-r border-[#e8e0c8] py-2 text-[11px] text-zinc-500">GMT+07</div>
            {days.map(d=>{
              const isToday=d.toDateString()===new Date().toDateString();
              return <div key={d.toISOString()} className="border-r border-[#f0ece0] py-2"><div className={`text-[11px] uppercase ${isToday?"text-[#005a5e]":"text-zinc-500"}`}>{d.toLocaleString('en',{weekday:'short'}).toUpperCase()}</div><div className={`mx-auto mt-1 flex h-8 w-8 items-center justify-center rounded-lg text-lg ${isToday?"bg-[#005a5e] text-white":"text-[#202124]"}`}>{d.getDate()}</div></div>;
            })}
          </div>

          <div className="relative flex-1 overflow-y-auto">
            <div className="grid" style={{gridTemplateColumns:`60px repeat(${days.length},1fr)`}}>
              {hours.map(h=> (
                <div key={h} className="contents">
                  <div className="border-b border-[#f0ece0] border-r py-2 pr-2 text-right text-[11px] text-zinc-500">{h===12? "12 PM" : h<12? `${h} AM` : `${h-12} PM`}</div>
                  {days.map(d=> {
                    const slotEvents = filtered.filter(e=>{
                      const s=toLocalDate(e.start_at);
                      return s.toDateString()===d.toDateString() && s.getHours()===h;
                    });
                    return (
                      <div key={d.toISOString()+h} onClick={()=> openCreate(d,h)} className="relative h-12 cursor-pointer border-b border-r border-[#f0ece0] hover:bg-[#f8f6ef]">
                        {slotEvents.map(ev=> {
                          const s=toLocalDate(ev.start_at);
                          const e=toLocalDate(ev.end_at);
                          const top = s.getMinutes();
                          const dur = (e.getTime()-s.getTime())/60000;
                          const hgt = Math.max(18, dur);
                          const color = ev.color==="blue" ? "bg-blue-600" : ev.color==="emerald" ? "bg-emerald-500" : ev.color==="violet" ? "bg-violet-600" : "bg-zinc-600";
                          const confIcon = ev.conferencing==="google-meet" ? "🎥 Meet" : ev.conferencing==="teams" ? "👥 Teams" : ev.conferencing==="zoom" ? "🔵 Zoom" : "";
                          return <button key={ev.id} onClick={(ex)=>{ex.stopPropagation(); setSelected(ev);}} className={`absolute left-1 right-1 rounded px-1 py-0.5 text-left text-[11px] font-medium text-white ${color}`} style={{top: `${top/60*100}%`, height: `${hgt}px`}}><span className="truncate">{ev.title} {confIcon && `• ${confIcon}`}</span></button>;
                        })}
                        {d.getDay()===1 && h===18 && slotEvents.length===0 && <div className="pointer-events-none mx-1 mt-1 rounded bg-red-500/90 px-1 py-0.5 text-[11px] text-white">Focus — 6 PM</div>}
                      </div>
                    );
                  })}
                </div>
              ))}
            </div>
            <div className="pointer-events-none absolute left-[60px] right-0 hidden lg:block" style={{top: `${((new Date().getHours()-7)*48 + new Date().getMinutes()*0.8)}px`}}>
              <div className="relative mx-[1%]"><div className="h-0.5 bg-red-500"/><div className="absolute -left-1 -top-1 h-2 w-2 rounded-full bg-red-500"/></div>
            </div>
          </div>
        </div>

        <div className="hidden w-12 shrink-0 flex-col items-center gap-4 border-l border-[#e8e0c8] py-4 lg:flex">
          <span className="text-amber-400">💡</span><span className="text-blue-500">✓</span><span className="text-zinc-600">👤</span><span className="text-emerald-500">📍</span><span className="mt-auto text-zinc-400">+</span>
        </div>
      </div>

      {showCreate && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/20 p-4">
          <div className="w-full max-w-lg rounded-xl border border-[#e8e0c8] bg-[#fefcf6] p-4 shadow-xl">
            <div className="flex items-center justify-between"><span className="text-sm font-semibold">Create event</span><button onClick={()=> setShowCreate(false)} className="rounded p-1 hover:bg-zinc-100">✕</button></div>
            <div className="mt-3 space-y-3">
              <input value={form.title} onChange={e=> setForm({...form, title:e.target.value})} placeholder="Add title" className="w-full rounded-lg border border-[#e8e0c8] px-3 py-2 text-sm" />
              <div className="grid grid-cols-2 gap-2">
                <select value={form.calendar} onChange={e=> setForm({...form, calendar:e.target.value})} className="rounded-lg border border-[#e8e0c8] px-3 py-2 text-sm">
                  {CATEGORIES.map(c=> <option key={c.name} value={c.name}>{c.name}</option>)}
                </select>
                <select value={form.color} onChange={e=> setForm({...form, color:e.target.value})} className="rounded-lg border border-[#e8e0c8] px-3 py-2 text-sm">
                  <option value="blue">Blue</option><option value="emerald">Emerald</option><option value="violet">Violet</option><option value="zinc">Zinc</option>
                </select>
              </div>
              <div className="grid grid-cols-2 gap-2">
                <input type="datetime-local" value={form.start_at} onChange={e=> setForm({...form, start_at:e.target.value})} className="rounded-lg border border-[#e8e0c8] px-3 py-2 text-sm" />
                <input type="datetime-local" value={form.end_at} onChange={e=> setForm({...form, end_at:e.target.value})} className="rounded-lg border border-[#e8e0c8] px-3 py-2 text-sm" />
              </div>
              <input value={form.guests} onChange={e=> setForm({...form, guests:e.target.value})} placeholder="Add guests (comma separated)" className="w-full rounded-lg border border-[#e8e0c8] px-3 py-2 text-sm" />
              <input value={form.location} onChange={e=> setForm({...form, location:e.target.value})} placeholder="Add location" className="w-full rounded-lg border border-[#e8e0c8] px-3 py-2 text-sm" />
              <div className="rounded-lg border border-[#e8e0c8] bg-[#f8f6ef] p-2">
                <div className="text-xs font-semibold text-zinc-700">Conferencing — choose preference</div>
                <div className="mt-2 grid grid-cols-2 gap-2 sm:grid-cols-3">
                  {[
                    {id:"none", label:"No conferencing"},
                    {id:"google-meet", label:"Google Meet"},
                    {id:"teams", label:"Microsoft Teams"},
                    {id:"zoom", label:"Zoom"},
                    {id:"custom", label:"Custom link"},
                  ].map(opt=> (
                    <button key={opt.id} onClick={()=> setForm({...form, conferencing: opt.id})} className={`rounded-lg border px-2 py-2 text-left text-xs font-medium ${form.conferencing===opt.id ? "border-[#005a5e] bg-[#005a5e] text-white" : "border-[#e8e0c8] bg-[#fefcf6] hover:bg-[#f8f6ef]"}`}>{opt.label}</button>
                  ))}
                </div>
                {form.conferencing!=="none" && (
                  <div className="mt-2 flex gap-2">
                    <input value={form.conferencing_link} onChange={e=> setForm({...form, conferencing_link:e.target.value})} placeholder={form.conferencing==="google-meet" ? "https://meet.google.com/..." : form.conferencing==="teams" ? "https://teams.live.com/..." : form.conferencing==="zoom" ? "https://zoom.us/j/..." : "https://..."} className="flex-1 rounded-lg border border-[#e8e0c8] bg-[#fefcf6] px-3 py-1.5 text-xs" />
                    <span className="self-center text-[11px] text-zinc-400">{form.conferencing==="google-meet" ? "Google" : form.conferencing==="teams" ? "Teams" : form.conferencing==="zoom" ? "Zoom" : "Custom"}</span>
                  </div>
                )}
                <div className="mt-1 text-[11px] text-zinc-500">Preference saved per event. CalNode will auto-create Meet/Teams/Zoom link if host calendar connected.</div>
              </div>
              <textarea value={form.description} onChange={e=> setForm({...form, description:e.target.value})} placeholder="Add description" rows={2} className="w-full rounded-lg border border-[#e8e0c8] px-3 py-2 text-sm" />
              <div className="grid grid-cols-2 gap-2">
                <select value={form.recurring} onChange={e=> setForm({...form, recurring:e.target.value})} className="rounded-lg border border-[#e8e0c8] px-3 py-2 text-sm">
                  <option value="never">Does not repeat</option><option value="daily">Daily</option><option value="weekly">Weekly</option><option value="monthly">Monthly</option>
                </select>
                <select value={form.notifications} onChange={e=> setForm({...form, notifications:e.target.value})} className="rounded-lg border border-[#e8e0c8] px-3 py-2 text-sm">
                  <option value="10m">Notification 10 minutes before</option><option value="30m">30 minutes before</option><option value="1h">1 hour before</option><option value="1d">1 day before</option>
                </select>
              </div>
              <div className="flex gap-2">
                <button onClick={saveEvent} className="rounded-lg bg-[#005a5e] px-4 py-2 text-sm font-semibold text-white hover:bg-[#00454a]">Save</button>
                <button onClick={()=> setShowCreate(false)} className="rounded-lg border border-[#e8e0c8] px-4 py-2 text-sm">Cancel</button>
                <span className="ml-auto text-xs text-zinc-400">via CalNode bridge + local</span>
              </div>
            </div>
          </div>
        </div>
      )}

      {selected && (
        <div className="fixed inset-0 z-40 flex items-center justify-center bg-black/20 p-4" onClick={()=> setSelected(null)}>
          <div onClick={e=> e.stopPropagation()} className="w-full max-w-md rounded-xl border border-[#e8e0c8] bg-[#fefcf6] p-4 shadow-xl">
            <div className="text-sm font-semibold">{selected.title} {selected.conferencing && selected.conferencing!=="none" && <span className="ml-2 rounded bg-blue-50 px-1.5 py-0.5 text-[11px] font-semibold text-blue-700">{selected.conferencing==="google-meet" ? "Google Meet" : selected.conferencing==="teams" ? "Teams" : selected.conferencing==="zoom" ? "Zoom" : selected.conferencing}</span>}</div>
            <div className="mt-1 text-xs text-zinc-500">{new Date(selected.start_at).toLocaleString()} → {new Date(selected.end_at).toLocaleString()} · {selected.calendar}</div>
            {selected.guests && <div className="mt-1 text-xs text-zinc-600">Guests: {selected.guests}</div>}
            {selected.conferencing_link && <div className="mt-1 text-xs"><a href={selected.conferencing_link} target="_blank" className="text-blue-600 underline">Join {selected.conferencing==="google-meet" ? "Google Meet" : selected.conferencing==="teams" ? "Teams" : selected.conferencing==="zoom" ? "Zoom" : "Meeting"} ↗</a></div>}
            {selected.location && <div className="mt-1 text-xs text-zinc-500">📍 {selected.location}</div>}
            <div className="mt-3 flex gap-2">
              <button onClick={async()=>{ await fetch(`${API}/v1/calendar/events/${selected.id}?mailbox_id=${encodeURIComponent(mailboxId)}`, {method:"DELETE"}); setSelected(null); fetchEvents(); }} className="rounded border border-red-200 bg-red-50 px-3 py-1.5 text-xs font-semibold text-red-700">Delete</button>
              <button onClick={()=> setSelected(null)} className="rounded border border-[#e8e0c8] px-3 py-1.5 text-xs">Close</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
