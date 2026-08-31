"use client";
import { useEffect, useState } from "react";
const API = process.env.NEXT_PUBLIC_MAIL_API || "http://localhost:8095";

type Booking = { id: string; start_at: string; end_at?: string; title?: string; guest?: string };

export default function CalendarPage() {
  const [weekStart, setWeekStart] = useState(() => {
    const d = new Date();
    d.setHours(0,0,0,0);
    const day = d.getDay(); // 0 SUN
    d.setDate(d.getDate() - day);
    return d;
  });
  const [bookings, setBookings] = useState<Booking[]>([]);
  const [eventTypes, setEventTypes] = useState<any[]>([]);
  const [calStatus, setCalStatus] = useState<any>(null);

  useEffect(() => {
    fetch(`${API}/v1/calendar/status`).then(r=>r.json()).then(j=> setCalStatus(j.data || j)).catch(()=>{});
    fetch(`${API}/v1/calendar/event-types`).then(r=>r.json()).then(j=> {
      const list = j.data?.data || j.data || [];
      if (Array.isArray(list)) setEventTypes(list.slice(0,3));
    }).catch(()=>{});
    // try fetch bookings via CalNode list (if proxy works, else stays empty)
    fetch(`${API}/v1/calendar/slots?event_type_slug=intro-call&from=${new Date().toISOString().slice(0,10)}&to=${new Date(Date.now()+7*864e5).toISOString().slice(0,10)}&tz=Asia/Jakarta`).then(r=>r.json()).then(j=>{
      // slots are free times; bookings would be separate endpoint, keep empty for now
    }).catch(()=>{});
  }, []);

  const days = Array.from({length:7}, (_,i)=> {
    const d = new Date(weekStart);
    d.setDate(weekStart.getDate()+i);
    return d;
  });
  const hours = Array.from({length:13}, (_,i)=> i+7); // 7AM..7PM

  const monthLabel = `${days[0].toLocaleString('en', {month:'short'})} – ${days[6].toLocaleString('en', {month:'short', year:'numeric'})}`;

  function shift(dir: number) {
    const n = new Date(weekStart);
    n.setDate(n.getDate()+ dir*7);
    setWeekStart(n);
  }
  function goToday() {
    const d = new Date();
    d.setHours(0,0,0,0);
    d.setDate(d.getDate()- d.getDay());
    setWeekStart(d);
  }

  return (
    <div className="flex h-screen flex-col bg-white text-zinc-900">
      {/* Header — Google-like */}
      <header className="flex h-16 shrink-0 items-center gap-3 border-b border-zinc-200 px-3">
        <button className="rounded p-2 hover:bg-zinc-100">☰</button>
        <div className="flex items-center gap-2">
          <span className="flex h-8 w-8 items-center justify-center rounded bg-blue-600 text-white text-xs font-bold">31</span>
          <span className="text-xl font-normal">Calendar</span>
        </div>
        <button onClick={goToday} className="ml-4 rounded-full border border-zinc-300 px-4 py-1.5 text-sm font-medium hover:bg-zinc-50">Today</button>
        <div className="flex gap-1">
          <button onClick={()=> shift(-1)} className="rounded-full p-1.5 hover:bg-zinc-100">‹</button>
          <button onClick={()=> shift(1)} className="rounded-full p-1.5 hover:bg-zinc-100">›</button>
        </div>
        <span className="text-xl font-normal">{monthLabel}</span>
        <div className="ml-auto flex items-center gap-2">
          <button className="hidden sm:inline-flex rounded-full border border-zinc-300 px-4 py-1.5 text-sm">Week ▾</button>
          <a href="https://book.aivory.uk/book/aivory-call" target="_blank" className="rounded-full bg-blue-600 px-4 py-1.5 text-sm font-medium text-white hover:bg-blue-700">Book via CalNode ↗</a>
          <a href="https://mail.aivory.uk/calendar" target="_blank" className="rounded-full bg-blue-50 px-4 py-1.5 text-sm font-medium text-blue-700 hover:bg-blue-100">mail.aivory.uk/calendar</a>
          <span className="flex h-8 w-8 items-center justify-center rounded-full bg-amber-200 text-xs">👋</span>
        </div>
      </header>

      <div className="flex min-h-0 flex-1">
        {/* Left sidebar — mini calendar + lists */}
        <aside className="hidden w-[260px] shrink-0 flex-col border-r border-zinc-200 bg-white p-3 lg:flex">
          <button className="flex items-center gap-2 rounded-full border border-zinc-200 bg-white px-4 py-2 text-sm font-medium shadow hover:bg-zinc-50">+ Create ▾</button>
          <div className="mt-4">
            <div className="flex items-center justify-between text-sm font-medium">
              <span>August 2026</span>
              <span className="flex gap-1 text-zinc-400">‹ ›</span>
            </div>
            <div className="mt-2 grid grid-cols-7 gap-1 text-center text-xs text-zinc-500">
              {["S","M","T","W","T","F","S"].map(d=> <span key={d}>{d}</span>)}
              {Array.from({length:35}, (_,i)=> {
                const d = new Date(2026,7,26+i); // start 26 Jul
                const isToday = d.getDate()===31 && d.getMonth()===7;
                return <span key={i} className={`flex h-6 w-6 items-center justify-center rounded-full text-xs ${isToday ? "bg-blue-600 text-white" : "hover:bg-zinc-100"}`}>{d.getDate()}</span>;
              })}
            </div>
          </div>
          <button className="mt-3 flex w-full items-center gap-2 rounded-lg bg-zinc-100 px-3 py-2 text-sm text-zinc-600">👥 Search for people</button>

          <div className="mt-4 space-y-3">
            <div className="flex items-center justify-between text-sm font-semibold">
              <span>Booking pages</span><span className="rounded p-1 hover:bg-zinc-100">+</span>
            </div>
            <div className="space-y-1 text-sm">
              <div className="text-xs font-semibold text-zinc-500">My calendars</div>
              {["Daemon Larkin","Birthdays","Tasks"].map(n=> (
                <label key={n} className="flex items-center gap-2 text-sm"><input type="checkbox" defaultChecked className="rounded" /> {n}</label>
              ))}
              <div className="text-xs font-semibold text-zinc-500">Other calendars</div>
              <label className="flex items-center gap-2 text-sm"><input type="checkbox" defaultChecked className="rounded" /> Holidays in Indonesia</label>
            </div>
          </div>

          {eventTypes.length >0 && (
            <div className="mt-4 rounded-lg border border-zinc-200 p-2">
              <div className="text-xs font-semibold">CalNode event types</div>
              {eventTypes.map((e:any)=> (
                <div key={e.slug||e.id} className="mt-1 flex justify-between text-xs">
                  <span className="truncate">{e.title || e.slug}</span>
                  <span className="flex gap-2"><a href={`/calendar?event=${e.slug}`} className="text-blue-600 hover:underline">View in Aivory Calendar</a><a href={`https://book.aivory.uk/${e.slug}`} target="_blank" className="text-zinc-500 hover:underline">book.aivory.uk ↗</a></span>
                </div>
              ))}
              <div className="mt-2 text-[11px] text-zinc-400 break-all">API: {process.env.NEXT_PUBLIC_MAIL_API || "http://localhost:8095"}/v1/calendar</div>
            </div>
          )}
        </aside>

        {/* Week grid */}
        <div className="flex min-w-0 flex-1 flex-col">
          <div className="grid grid-cols-[60px_repeat(7,1fr)] border-b border-zinc-200 text-center text-xs">
            <div className="border-r border-zinc-200 py-2 text-[11px] text-zinc-500">GMT+07</div>
            {days.map(d=> {
              const isToday = d.toDateString()===new Date().toDateString();
              return (
                <div key={d.toISOString()} className="border-r border-zinc-100 py-2">
                  <div className={`text-[11px] uppercase ${isToday ? "text-blue-600" : "text-zinc-500"}`}>{d.toLocaleString('en',{weekday:'short'}).toUpperCase()}</div>
                  <div className={`mx-auto mt-1 flex h-8 w-8 items-center justify-center rounded-full text-lg ${isToday ? "bg-blue-600 text-white" : "text-zinc-900"}`}>{d.getDate()}</div>
                </div>
              );
            })}
          </div>

          <div className="relative flex-1 overflow-y-auto">
            <div className="grid grid-cols-[60px_repeat(7,1fr)]">
              {hours.map(h=> (
                <div key={h} className="contents">
                  <div className="border-b border-zinc-100 border-r py-2 pr-2 text-right text-[11px] text-zinc-500">{h===12 ? "12 PM" : h<12 ? `${h} AM` : `${h-12} PM`}</div>
                  {days.map(d=> (
                    <div key={d.toISOString()+h} className="h-12 border-b border-r border-zinc-100 hover:bg-zinc-50">
                      {/* bookings overlay: simple example at Mon 6PM */}
                      {d.getDay()===1 && h===18 && (
                        <div className="mx-1 mt-1 rounded bg-red-500 px-1 py-0.5 text-[11px] text-white">Focus — 6 PM</div>
                      )}
                    </div>
                  ))}
                </div>
              ))}
            </div>
            {/* Now line like screenshot */}
            <div className="pointer-events-none absolute left-[60px] right-0 top-[60%] hidden lg:block">
              <div className="relative mx-[14.28%] mr-[28.5%]">
                <div className="h-0.5 bg-red-500" />
                <div className="absolute -left-1 -top-1 h-2 w-2 rounded-full bg-red-500" />
              </div>
            </div>
          </div>
        </div>

        {/* Right rail icons (Google-like) */}
        <div className="hidden w-12 shrink-0 flex-col items-center gap-4 border-l border-zinc-200 py-4 lg:flex">
          <span className="text-amber-400">💡</span>
          <span className="text-blue-500">✓</span>
          <span className="text-zinc-600">👤</span>
          <span className="text-emerald-500">📍</span>
          <span className="mt-auto text-zinc-400">+</span>
        </div>
      </div>
    </div>
  );
}
