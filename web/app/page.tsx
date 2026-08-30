"use client";
import { useEffect, useState } from "react";

const API = process.env.NEXT_PUBLIC_MAIL_API || "http://localhost:8095";

type Msg = { id: string; from: string; subject: string; snippet: string; created_at: string; is_read: boolean };

export default function InboxPage() {
  const [msgs, setMsgs] = useState<Msg[]>([]);
  const [selected, setSelected] = useState<any>(null);
  const [mailbox, setMailbox] = useState("");

  useEffect(() => {
    fetch(`${API}/v1/messages?folder=Inbox&per_page=20`)
      .then(r => r.json()).then(j => setMsgs(j.data || [])).catch(()=>{});
  }, []);

  async function open(id: string) {
    const r = await fetch(`${API}/v1/messages/${id}`);
    const j = await r.json();
    setSelected(j.data);
  }

  return (
    <div style={{ display: "flex", height: "100vh" }}>
      <aside style={{ width: 280, borderRight: "1px solid #e5e7eb", background: "#fff", padding: 16 }}>
        <h1 style={{ fontSize: 20, fontWeight: 700 }}>Aivory Mail</h1>
        <p style={{ color: "#6b7280", fontSize: 12 }}>Business email, without the email tax.</p>
        <nav style={{ marginTop: 24, display: "flex", flexDirection: "column", gap: 8 }}>
          {["Inbox","Sent","Drafts","Spam","Trash"].map(f => (
            <button key={f} style={{ textAlign: "left", padding: "8px 12px", borderRadius: 8, border: "1px solid #e5e7eb", background: f==="Inbox"?"#111":"#fff", color: f==="Inbox"?"#fff":"#111" }}>{f}</button>
          ))}
        </nav>
        <div style={{ marginTop: 24, padding: 12, background: "#f3f4f6", borderRadius: 8 }}>
          <div style={{ fontSize: 12, fontWeight: 600 }}>AI Triage</div>
          <div style={{ fontSize: 11, color: "#6b7280", marginTop: 4 }}>Email → Intelligence → Workflow → Action</div>
        </div>
      </aside>
      <section style={{ flex: 1, display: "flex" }}>
        <div style={{ width: 380, borderRight: "1px solid #e5e7eb", overflowY: "auto", background: "#fff" }}>
          <div style={{ padding: 12, borderBottom: "1px solid #e5e7eb", fontWeight: 600 }}>Inbox — {msgs.length}</div>
          {msgs.length===0 && <div style={{ padding: 24, color: "#9ca3af", fontSize: 13 }}>No messages yet. Send a test email to your mailbox.</div>}
          {msgs.map(m => (
            <div key={m.id} onClick={()=>open(m.id)} style={{ padding: 12, borderBottom: "1px solid #f3f4f6", cursor: "pointer", background: selected?.id===m.id?"#f9fafb":"#fff" }}>
              <div style={{ fontWeight: m.is_read?400:600, fontSize: 13 }}>{m.from}</div>
              <div style={{ fontSize: 13, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{m.subject || "(no subject)"}</div>
              <div style={{ fontSize: 11, color: "#6b7280" }}>{m.snippet}</div>
            </div>
          ))}
        </div>
        <div style={{ flex: 1, padding: 24, overflowY: "auto" }}>
          {!selected ? <div style={{ color: "#9ca3af" }}>Select a message</div> : (
            <div>
              <h2 style={{ fontSize: 18, fontWeight: 700 }}>{selected.subject}</h2>
              <div style={{ fontSize: 12, color: "#6b7280" }}>From {selected.from} • {selected.created_at}</div>
              <div style={{ marginTop: 16, whiteSpace: "pre-wrap", fontSize: 14, lineHeight: 1.6 }}>{selected.body_text || selected.body_html || selected.snippet}</div>
              {selected.body_html && <div style={{ marginTop: 16, border: "1px solid #e5e7eb", borderRadius: 8, padding: 12 }} dangerouslySetInnerHTML={{ __html: selected.body_html }} />}
              <div style={{ marginTop: 24, display: "flex", gap: 8 }}>
                <button style={{ padding: "8px 14px", borderRadius: 8, background: "#111", color: "#fff" }}>Reply</button>
                <button style={{ padding: "8px 14px", borderRadius: 8, border: "1px solid #e5e7eb" }}>Forward</button>
                <button style={{ padding: "8px 14px", borderRadius: 8, border: "1px solid #e5e7eb" }}>Archive</button>
              </div>
            </div>
          )}
        </div>
      </section>
    </div>
  );
}
