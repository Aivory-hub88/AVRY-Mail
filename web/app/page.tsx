"use client";
import { useEffect, useState } from "react";
import ComposeModal from "../components/ComposeModal";

const API = process.env.NEXT_PUBLIC_MAIL_API || "http://localhost:8095";
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
  const [tabs, setTabs] = useState<{id:string,label:string,compose?:any}[]>([{id:"mail",label:"Mail"}]);
  const [activeTab, setActiveTab] = useState("mail");

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
    const tid = (data as any)?.thread_id;
    if (tid) {
      fetch(`${API}/v1/threads/${tid}/crawl`).then(r=>r.json()).then(j=> setCrawl(j.data?.crawl || null)).catch(()=> setCrawl(null));
    } else setCrawl(null);
  }

  return (
    <div className="flex h-screen bg-zinc-50 text-zinc-900">
      {/* Sidebar */}
      <aside className="flex w-[280px] shrink-0 flex-col border-r border-zinc-200 bg-white">
        <div className="border-b border-zinc-100 px-4 py-5">
          <h1 className="text-xl font-bold tracking-tight">Aivory Mail</h1>
          <p className="mt-1 text-xs leading-relaxed text-zinc-500">
            Business email, without
            <br /> the email tax.
          </p>
        </div>

        <div className="px-3 pt-3">
          <button onClick={()=>openCompose()} className="w-full rounded-xl bg-zinc-900 px-4 py-3 text-sm font-semibold text-white shadow hover:bg-black">✏️ Compose</button>
        </div>
        <nav className="flex flex-1 flex-col gap-1.5 px-3 py-4">
          {["Inbox", "Sent", "Drafts", "Spam", "Trash"].map((f) => (
            <button
              key={f}
              onClick={() => setActiveFolder(f)}
              className={`rounded-lg border px-3 py-2.5 text-left text-sm font-medium transition ${
                f === activeFolder
                  ? "border-zinc-900 bg-zinc-900 text-white shadow-sm"
                  : "border-zinc-200 bg-white text-zinc-700 hover:bg-zinc-50 hover:border-zinc-300"
              }`}
            >
              <span className="flex items-center justify-between">
                {f}
                {f === "Inbox" && msgs.length > 0 && (
                  <span
                    className={`ml-2 rounded-full px-2 py-0.5 text-xs font-semibold ${
                      activeFolder === "Inbox" ? "bg-white text-zinc-900" : "bg-zinc-900 text-white"
                    }`}
                  >
                    {msgs.length}
                  </span>
                )}
              </span>
            </button>
          ))}
        </nav>

        <div className="mx-3 mb-4 rounded-xl border border-zinc-100 bg-zinc-50 p-3">
          <div className="text-xs font-semibold text-zinc-900">AI Triage</div>
          <div className="mt-1 text-[11px] leading-relaxed text-zinc-500">
            Email → Intelligence → Workflow → Action
          </div>
          <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-zinc-200">
            <div className="h-full w-2/3 rounded-full bg-zinc-900" />
          </div>
          <div className="mt-1.5 text-[10px] text-zinc-400">Heuristic + Cerveau gateway</div>
        </div>

        <div className="px-3 py-2 space-y-1">
          <button onClick={()=> setShowSigModal(true)} className="w-full rounded-lg border border-zinc-200 bg-white px-3 py-1.5 text-xs font-medium hover:bg-zinc-50">✒️ Signature {activeSig ? `• ${activeSig.name}` : ""}</button>
          <a href="https://book.aivory.uk" target="_blank" className="flex items-center justify-between rounded-lg border border-zinc-200 bg-white px-3 py-1.5 text-xs hover:bg-zinc-50">
            <span>📅 CalNode {calStatus?.status ? "• "+String(calStatus.status).slice(0,12) : "• book.aivory.uk"}</span>
            <span className="text-[11px] text-zinc-400">↗</span>
          </a>
        </div>
        <div className="border-t border-zinc-100 px-3 py-3">
          <div className="text-[11px] text-zinc-400">MAIL_MODE: vps · storage: local</div>
          <a
            href="http://localhost:8095/health"
            target="_blank"
            className="text-[11px] font-medium text-zinc-600 underline decoration-zinc-300 underline-offset-2 hover:text-zinc-900"
          >
            API health ↗
          </a>
        </div>
      </aside>

      {/* Content — inbox always visible, compose in detail pane (red box) */}
      <div className="flex min-w-0 flex-1 flex-col bg-zinc-100">
        <div className="flex h-9 shrink-0 items-center gap-2 border-b border-zinc-200 bg-zinc-800 px-3 text-xs text-zinc-300">
          <span className="rounded bg-white px-2 py-1 text-xs font-semibold text-zinc-900">📧 Mail</span>
          <span className="text-zinc-400">·</span>
          <span className="hidden sm:inline">Search</span>
          <input value={search} onChange={e=>setSearch(e.target.value)} placeholder="Search ( / )" className="ml-2 hidden w-48 rounded-full bg-white px-3 py-1 text-xs text-zinc-700 placeholder:text-zinc-400 focus:outline-none sm:block" />
          {composeOpen && <span className="ml-auto rounded bg-amber-400 px-2 py-1 text-xs font-semibold text-zinc-900">Composing…</span>}
        </div>

        <section className="flex min-w-0 flex-1">
        {/* Message list */}
        <div className="flex w-[400px] shrink-0 flex-col border-r border-zinc-200 bg-white">
          <div className="sticky top-0 z-10 border-b border-zinc-200 bg-white">
            <div className="px-3 py-2">
              <input value={search} onChange={e=>setSearch(e.target.value)} placeholder="Search messages..." className="w-full rounded-lg border border-zinc-200 bg-zinc-50 px-3 py-1.5 text-sm placeholder:text-zinc-400 focus:bg-white focus:border-zinc-900 focus:outline-none" />
            </div>
            <div className="flex items-center justify-between px-4 py-2">
            <span className="text-sm font-semibold">
              {activeFolder} — {msgs.length}
            </span>
              <span className="rounded-full bg-zinc-900 px-2 py-0.5 text-[11px] font-semibold text-white">
                {msgs.filter((m) => !m.is_read).length} new
              </span>
            </div>
          </div>

          <div className="flex-1 overflow-y-auto">
            {msgs.length === 0 && (
              <div className="p-8 text-center">
                <div className="mx-auto flex h-10 w-10 items-center justify-center rounded-full bg-zinc-100 text-zinc-400">
                  ✉️
                </div>
                <p className="mt-3 text-sm font-medium text-zinc-700">No messages yet</p>
                <p className="mt-1 text-xs text-zinc-500">Send a test email to your mailbox.</p>
              </div>
            )}
            {msgs.map((m) => (
              <button
                key={m.id}
                onClick={() => open(m.id)}
                className={`flex w-full flex-col gap-1 border-b border-zinc-100 px-4 py-3 text-left transition hover:bg-zinc-50 ${
                  selected?.id === m.id ? "bg-zinc-50 ring-1 ring-inset ring-zinc-900" : "bg-white"
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

        {/* Detail — red box: compose inline keeps inbox visible */}
        <div className="flex min-w-0 flex-1 flex-col bg-zinc-50">
          {composeOpen ? (
            <div className="flex min-w-0 flex-1 flex-col bg-white">
              <ComposeModal open={true} onClose={()=> { setComposeOpen(false); setReplyInfo(null); }} onSent={()=> { setComposeOpen(false); setReplyInfo(null); setSelected(null); }} defaultFrom={defaultFrom} replyTo={replyInfo} inline />
            </div>
          ) : !selected ? (
            <div className="flex flex-1 flex-col items-center justify-center p-10 text-center">
              <div className="rounded-2xl border border-dashed border-zinc-300 bg-white px-8 py-10">
                <div className="mx-auto flex h-12 w-12 items-center justify-center rounded-xl bg-zinc-900 text-white">
                  ✉️
                </div>
                <p className="mt-4 text-sm font-semibold text-zinc-900">Select a message</p>
                <p className="mt-1 max-w-[260px] text-xs leading-relaxed text-zinc-500">
                  Click a message on the left. Intelligence panel will show intent, urgency, and suggested actions.
                </p>
              </div>
            </div>
          ) : (
            <div className="flex flex-1 flex-col overflow-y-auto">
              <div className="border-b border-zinc-200 bg-white px-6 py-5">
                <h2 className="text-lg font-bold leading-tight text-zinc-900">{selected.subject}</h2>
                <div className="mt-2 flex flex-wrap items-center gap-2 text-xs text-zinc-500">
                  <span className="rounded-full border border-zinc-200 bg-white px-2.5 py-1 font-medium text-zinc-700">
                    From {selected.from}
                  </span>
                  <span>{new Date(selected.created_at).toLocaleString()}</span>
                  <span className="rounded-full bg-emerald-50 px-2 py-0.5 text-[11px] font-semibold text-emerald-700 ring-1 ring-emerald-200">
                    Inbox
                  </span>
                </div>
              </div>

              <div className="space-y-6 p-6">
                <div className="rounded-xl border border-zinc-200 bg-white p-5 shadow-sm">
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
                  <button onClick={()=>openCompose(selected)} className="rounded-lg bg-zinc-900 px-4 py-2 text-sm font-medium text-white shadow hover:bg-black">↩ Reply</button>
                  <button onClick={()=>{ setReplyInfo({ to: "", subject: `Fwd: ${selected.subject||""}`, body: selected.body_text || "" }); setComposeOpen(true);}} className="rounded-lg border border-zinc-200 bg-white px-4 py-2 text-sm font-medium text-zinc-700 hover:bg-zinc-50">↪ Forward</button>
                  <button onClick={()=>fetch(`${API}/v1/messages/${selected.id}/move`,{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({folder:"Archive"})}).then(()=> setSelected(null))} className="rounded-lg border border-zinc-200 bg-white px-4 py-2 text-sm font-medium text-zinc-500 hover:bg-zinc-50">Archive</button>
                  <button onClick={()=>toggleStar(selected.id)} className={`rounded-lg border px-3 py-2 text-xs font-semibold ${selected.is_starred ? "border-amber-300 bg-amber-50 text-amber-800" : "border-zinc-200 bg-white text-zinc-600"}`}>{selected.is_starred ? "★ Starred" : "☆ Star"}</button>
                  <button onClick={()=>doShare(selected.id)} className="rounded-lg border border-zinc-200 bg-white px-3 py-2 text-xs font-medium hover:bg-zinc-50">🔗 Share link</button>
                  <button className="ml-auto rounded-lg border border-amber-200 bg-amber-50 px-3 py-2 text-xs font-semibold text-amber-800">AI: Create Finance Task</button>
                </div>
                {shareUrl && <div className="rounded-lg border border-emerald-200 bg-emerald-50 px-3 py-2 text-xs"><span className="font-semibold">Share link copied:</span> <a href={shareUrl} target="_blank" className="break-all text-emerald-800 underline">{shareUrl}</a></div>}
                {selected.attachments?.length > 0 && (
                  <div className="rounded-xl border border-zinc-200 bg-white p-4">
                    <div className="text-xs font-semibold">Attachments · {selected.attachments.length}</div>
                    <div className="mt-2 space-y-2">
                      {selected.attachments.map((a:any)=> (
                        <a key={a.id} href={`${API}/v1/messages/${selected.id}/attachments/${a.id}`} target="_blank" className="flex items-center justify-between rounded-lg border border-zinc-200 bg-zinc-50 px-3 py-2 text-xs hover:bg-white">
                          <span className="truncate font-medium">{a.filename} · {(a.size_bytes/1024).toFixed(1)} KB · {a.content_type}</span>
                          <span className="ml-2 shrink-0 rounded bg-zinc-900 px-2 py-1 text-[11px] font-semibold text-white">Download</span>
                        </a>
                      ))}
                    </div>
                  </div>
                )}

                {crawl && (
                  <div className="rounded-xl border border-zinc-200 bg-white p-4">
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
                      <a href="https://book.aivory.uk" target="_blank" className="rounded border border-zinc-200 bg-white px-2.5 py-1 text-xs hover:bg-zinc-50">📅 Insert booking link</a>
                      <span className="text-[11px] text-zinc-400 self-center">via CalNode</span>
                    </div>
                  </div>
                )}
                <div className="rounded-xl border border-zinc-200 bg-white p-4">
                  <div className="text-xs font-semibold text-zinc-900">Intelligence (heuristic)</div>
                  <div className="mt-2 flex flex-wrap gap-1.5">
                    <span className="rounded-full bg-zinc-900 px-2.5 py-1 text-xs font-medium text-white">invoice</span>
                    <span className="rounded-full bg-red-50 px-2.5 py-1 text-xs font-semibold text-red-700 ring-1 ring-red-200">
                      High urgency
                    </span>
                    <span className="rounded-full bg-zinc-100 px-2.5 py-1 text-xs text-zinc-700">AED 18,500</span>
                  </div>
                  <div className="mt-3 text-xs leading-relaxed text-zinc-500">
                    Email will trigger <span className="font-semibold text-zinc-700">Aivory Workflow</span> → create task, notify finance, draft reminder after approval.
                  </div>
                </div>
              </div>
            </div>
          )}
        </div>
      </section>
      </div>
      {showSigModal && (
        <div className="fixed inset-0 z-40 flex items-center justify-center bg-black/20 p-4">
          <div className="w-full max-w-md rounded-xl border border-zinc-200 bg-white p-4 shadow-xl">
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
