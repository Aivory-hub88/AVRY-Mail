"use client";
import { useEffect, useState } from "react";
const API = process.env.NEXT_PUBLIC_MAIL_API || "http://localhost:8095";

function Stat({ label, value }: { label: string; value: string | number }) {
  return (
    <div className="rounded-2xl border border-[#e8e0c8] bg-[#fefcf6] p-4">
      <div className="text-xs font-semibold tracking-widest text-zinc-500 uppercase">{label}</div>
      <div className="mt-1 text-2xl font-bold text-[#202124]">{value}</div>
    </div>
  );
}

export default function AdminPage() {
  const [tab, setTab] = useState<"overview" | "users" | "groups" | "domains" | "aliases" | "logs">("overview");
  const [stats, setStats] = useState<any>(null);
  const [domains, setDomains] = useState<any[]>([]);
  const [mailboxes, setMailboxes] = useState<any[]>([]);
  const [groups, setGroups] = useState<any[]>([]);
  const [aliases, setAliases] = useState<any[]>([]);
  const [logs, setLogs] = useState<any[]>([]);
  const [newDomain, setNewDomain] = useState("");
  const [newUserAddr, setNewUserAddr] = useState("");
  const [newUserName, setNewUserName] = useState("");
  const [newGroupName, setNewGroupName] = useState("");
  const [newGroupEmail, setNewGroupEmail] = useState("");
  const [newAliasEmail, setNewAliasEmail] = useState("");
  const [newAliasMbId, setNewAliasMbId] = useState("");
  const [msg, setMsg] = useState("");

  useEffect(() => {
    const t = localStorage.getItem("aivory_mail_token");
    if (!t) window.location.href = "/login";
  }, []);

  function loadAll() {
    fetch(`${API}/v1/stats`).then(r => r.json()).then(j => setStats(j)).catch(() => {});
    fetch(`${API}/v1/domains`).then(r => r.json()).then(j => setDomains(j.data || [])).catch(() => {});
    fetch(`${API}/v1/mailboxes`).then(r => r.json()).then(j => setMailboxes(j.data || [])).catch(() => {});
    fetch(`${API}/v1/groups`).then(r => r.json()).then(j => setGroups(j.data || [])).catch(() => {});
    fetch(`${API}/v1/audit-logs`).then(r => r.json()).then(j => setLogs(j.data || [])).catch(() => {});
    // aliases: aggregate per mailbox
    fetch(`${API}/v1/mailboxes`).then(r => r.json()).then(async j => {
      const mbs = j.data || [];
      const all: any[] = [];
      for (const mb of mbs) {
        try {
          const r2 = await fetch(`${API}/v1/send-as?mailbox_id=${mb.id}`);
          const j2 = await r2.json();
          (j2.data || []).forEach((a: any) => all.push({ ...a, mailbox: mb.address }));
        } catch {}
      }
      setAliases(all);
    }).catch(() => {});
  }
  useEffect(() => { loadAll(); }, []);

  async function createDomain() {
    if (!newDomain.trim()) return;
    const r = await fetch(`${API}/v1/domains`, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ domain: newDomain.trim() }) });
    const j = await r.json();
    if (!j.success) setMsg(j.error || "Failed to create domain");
    else { setMsg(`Domain ${newDomain} created`); setNewDomain(""); loadAll(); }
  }
  async function createUser() {
    if (!newUserAddr.trim() || !newUserAddr.includes("@")) { setMsg("Enter valid email"); return; }
    const r = await fetch(`${API}/v1/mailboxes`, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ address: newUserAddr.trim(), display_name: newUserName.trim() }) });
    const j = await r.json();
    if (!j.success) setMsg(j.error || "Failed to create account");
    else { setMsg(`Account ${newUserAddr} created`); setNewUserAddr(""); setNewUserName(""); loadAll(); }
  }
  async function deleteUser(id: string) {
    if (!confirm("Delete this account?")) return;
    await fetch(`${API}/v1/mailboxes/${id}`, { method: "DELETE" });
    loadAll();
  }
  async function createGroup() {
    if (!newGroupName.trim() || !newGroupEmail.trim()) { setMsg("Name and email required"); return; }
    const r = await fetch(`${API}/v1/groups`, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ name: newGroupName.trim(), email: newGroupEmail.trim() }) });
    const j = await r.json();
    if (!j.success) setMsg(j.error || "Failed to create group");
    else { setMsg(`Group ${newGroupName} created`); setNewGroupName(""); setNewGroupEmail(""); loadAll(); }
  }
  async function deleteGroup(id: string) {
    if (!confirm("Delete group?")) return;
    await fetch(`${API}/v1/groups/${id}`, { method: "DELETE" });
    loadAll();
  }
  async function createAlias() {
    if (!newAliasMbId || !newAliasEmail.trim()) { setMsg("Select mailbox and alias"); return; }
    const r = await fetch(`${API}/v1/send-as`, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ mailbox_id: newAliasMbId, alias_email: newAliasEmail.trim() }) });
    const j = await r.json();
    if (!j.success) setMsg(j.error || "Failed to create alias");
    else { setMsg(`Alias ${newAliasEmail} created`); setNewAliasEmail(""); loadAll(); }
  }

  function doLogout() {
    localStorage.removeItem("aivory_mail_token");
    document.cookie = "aivory_mail_token=; path=/; max-age=0";
    window.location.href = "/login";
  }

  return (
    <div className="min-h-screen bg-[#f8f6ef]">
      <div className="border-b border-[#e8e0c8] bg-[#fefcf6]">
        <div className="mx-auto flex max-w-6xl items-center justify-between px-6 py-4">
          <div className="flex items-center gap-3">
            <img src="/aivory-mail-logo3.svg" alt="Aivory Mail" className="w-[160px] h-auto" />
            <span className="rounded-full bg-[#005a5e] px-2 py-0.5 text-xs font-semibold text-white">Admin</span>
          </div>
          <div className="flex items-center gap-2">
            <a href="/" className="rounded-full border border-[#e8e0c8] bg-white px-4 py-1.5 text-sm hover:bg-[#f8f6ef]">← Inbox</a>
            <button onClick={doLogout} className="rounded-full border border-zinc-200 bg-white px-4 py-1.5 text-sm">Logout</button>
          </div>
        </div>
        <div className="mx-auto max-w-6xl px-6 pb-3">
          <div className="flex gap-2 overflow-x-auto">
            {(["overview", "users", "groups", "domains", "aliases", "logs"] as const).map(t => (
              <button key={t} onClick={() => setTab(t)} className={`rounded-full px-4 py-1.5 text-sm font-medium capitalize ${tab === t ? "bg-[#005a5e] text-white" : "bg-white border border-[#e8e0c8] hover:bg-[#f8f6ef]"}`}>
                {t}
              </button>
            ))}
          </div>
        </div>
      </div>

      <div className="mx-auto max-w-6xl p-6">
        {msg && <div className="mb-4 rounded-xl bg-amber-50 px-4 py-2 text-sm text-amber-800 ring-1 ring-amber-200">{msg} <button onClick={() => setMsg("")} className="ml-2 text-xs underline">×</button></div>}

        {tab === "overview" && (
          <div className="space-y-6">
            <h2 className="text-xl font-bold">Overview</h2>
            <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
              <Stat label="Domains" value={stats?.domains ?? domains.length} />
              <Stat label="Accounts" value={stats?.mailboxes ?? mailboxes.length} />
              <Stat label="Messages" value={stats?.messages ?? 0} />
              <Stat label="Groups" value={groups.length} />
            </div>
            <div className="rounded-2xl border border-[#e8e0c8] bg-[#fefcf6] p-4">
              <div className="text-sm font-semibold">By folder</div>
              <div className="mt-2 grid grid-cols-3 md:grid-cols-6 gap-2 text-xs">
                {stats?.by_folder ? Object.entries(stats.by_folder).map(([k, v]: any) => (
                  <div key={k} className="rounded-xl bg-[#f8f6ef] px-3 py-2 text-center">
                    <div className="font-semibold">{k}</div>
                    <div className="text-lg">{String(v)}</div>
                  </div>
                )) : <div className="text-zinc-400">No data</div>}
              </div>
            </div>
            <div className="rounded-2xl border border-[#e8e0c8] bg-white p-4 text-sm">
              <div className="font-semibold">Quick actions</div>
              <div className="mt-2 flex flex-wrap gap-2">
                <button onClick={() => setTab("users")} className="rounded-full bg-[#005a5e] px-4 py-1.5 text-sm text-white">Create account</button>
                <button onClick={() => setTab("domains")} className="rounded-full border border-[#e8e0c8] bg-[#fefcf6] px-4 py-1.5 text-sm">Add domain</button>
                <button onClick={() => setTab("groups")} className="rounded-full border border-[#e8e0c8] bg-[#fefcf6] px-4 py-1.5 text-sm">Create group</button>
              </div>
            </div>
          </div>
        )}

        {tab === "users" && (
          <div className="space-y-4">
            <h2 className="text-xl font-bold">Accounts (Mailboxes)</h2>
            <div className="rounded-2xl border border-[#e8e0c8] bg-[#fefcf6] p-4">
              <div className="text-sm font-semibold">Create account</div>
              <div className="mt-2 flex flex-col md:flex-row gap-2">
                <input value={newUserAddr} onChange={e => setNewUserAddr(e.target.value)} placeholder="user@domain.com" className="flex-1 rounded-full border border-[#e8e0c8] px-4 py-2 text-sm" />
                <input value={newUserName} onChange={e => setNewUserName(e.target.value)} placeholder="Display name (optional)" className="flex-1 rounded-full border border-[#e8e0c8] px-4 py-2 text-sm" />
                <button onClick={createUser} className="rounded-full bg-[#005a5e] px-6 py-2 text-sm font-semibold text-white">Create</button>
              </div>
              <p className="mt-2 text-xs text-zinc-500">Domain must be verified first. Password: shared <span className="font-mono">MAIL_ADMIN_PASSWORD</span> (Avry786876!@ default).</p>
            </div>
            <div className="rounded-2xl border border-[#e8e0c8] bg-white overflow-hidden">
              <table className="w-full text-sm">
                <thead className="bg-[#f8f6ef] text-xs text-zinc-500">
                  <tr><th className="px-4 py-2 text-left">Address</th><th className="px-4 py-2 text-left">Name</th><th className="px-4 py-2">Actions</th></tr>
                </thead>
                <tbody>
                  {mailboxes.map((mb: any) => (
                    <tr key={mb.id} className="border-t border-[#f0ece0]">
                      <td className="px-4 py-2 font-mono text-xs">{mb.address}</td>
                      <td className="px-4 py-2">{mb.display_name || "-"}</td>
                      <td className="px-4 py-2 text-center">
                        <button onClick={() => deleteUser(mb.id)} className="text-xs text-red-600 hover:underline">Delete</button>
                      </td>
                    </tr>
                  ))}
                  {mailboxes.length === 0 && <tr><td colSpan={3} className="px-4 py-6 text-center text-sm text-zinc-400">No accounts yet</td></tr>}
                </tbody>
              </table>
            </div>
          </div>
        )}

        {tab === "groups" && (
          <div className="space-y-4">
            <h2 className="text-xl font-bold">Groups (Shared inboxes)</h2>
            <div className="rounded-2xl border border-[#e8e0c8] bg-[#fefcf6] p-4">
              <div className="text-sm font-semibold">Create group</div>
              <div className="mt-2 grid md:grid-cols-3 gap-2">
                <input value={newGroupName} onChange={e => setNewGroupName(e.target.value)} placeholder="Sales" className="rounded-full border border-[#e8e0c8] px-4 py-2 text-sm" />
                <input value={newGroupEmail} onChange={e => setNewGroupEmail(e.target.value)} placeholder="sales@domain.com" className="rounded-full border border-[#e8e0c8] px-4 py-2 text-sm" />
                <button onClick={createGroup} className="rounded-full bg-[#005a5e] px-6 py-2 text-sm font-semibold text-white">Create group</button>
              </div>
              <p className="mt-2 text-xs text-zinc-500">Group email acts as shared inbox. Add members by mailbox address (future: member picker).</p>
            </div>
            <div className="grid md:grid-cols-2 gap-4">
              {groups.map((g: any) => (
                <div key={g.id} className="rounded-2xl border border-[#e8e0c8] bg-white p-4">
                  <div className="flex items-center justify-between">
                    <div>
                      <div className="font-semibold">{g.name}</div>
                      <div className="text-xs font-mono text-zinc-500">{g.email}</div>
                    </div>
                    <button onClick={() => deleteGroup(g.id)} className="text-xs text-red-600 hover:underline">Delete</button>
                  </div>
                  <div className="mt-2 text-xs">
                    <div className="font-medium">Members: {g.members?.length || 0}</div>
                    <div className="mt-1 flex flex-wrap gap-1">
                      {(g.members || []).map((m: string) => <span key={m} className="rounded-full bg-[#f0ece0] px-2 py-0.5 text-xs">{m}</span>)}
                      {(!g.members || g.members.length === 0) && <span className="text-zinc-400">No members</span>}
                    </div>
                  </div>
                  <div className="mt-2 text-xs text-zinc-400">{g.description}</div>
                </div>
              ))}
              {groups.length === 0 && <div className="col-span-2 rounded-2xl border border-dashed border-[#e8e0c8] bg-[#fefcf6] p-8 text-center text-sm text-zinc-400">No groups yet. Create sales@, support@, etc.</div>}
            </div>
          </div>
        )}

        {tab === "domains" && (
          <div className="space-y-4">
            <h2 className="text-xl font-bold">Domains</h2>
            <div className="rounded-2xl border border-[#e8e0c8] bg-[#fefcf6] p-4">
              <div className="text-sm font-semibold">Add domain</div>
              <div className="mt-2 flex gap-2">
                <input value={newDomain} onChange={e => setNewDomain(e.target.value)} placeholder="example.com" className="flex-1 rounded-full border border-[#e8e0c8] px-4 py-2 text-sm" />
                <button onClick={createDomain} className="rounded-full bg-[#005a5e] px-6 py-2 text-sm font-semibold text-white">Add</button>
              </div>
            </div>
            <div className="space-y-2">
              {domains.map((d: any) => (
                <div key={d.id} className="rounded-2xl border border-[#e8e0c8] bg-white p-4 flex items-center justify-between">
                  <div>
                    <div className="font-mono text-sm font-semibold">{d.domain}</div>
                    <div className="text-xs text-zinc-500">Status: <span className={`rounded-full px-2 py-0.5 text-xs ${d.status === "Active" ? "bg-emerald-50 text-emerald-700" : "bg-amber-50 text-amber-700"}`}>{d.status}</span></div>
                  </div>
                  <a href={`/domains`} className="text-xs text-[#005a5e] hover:underline">Manage →</a>
                </div>
              ))}
              {domains.length === 0 && <div className="rounded-2xl border border-dashed border-[#e8e0c8] bg-[#fefcf6] p-8 text-center text-sm text-zinc-400">No domains</div>}
            </div>
          </div>
        )}

        {tab === "aliases" && (
          <div className="space-y-4">
            <h2 className="text-xl font-bold">Aliases (Send As)</h2>
            <div className="rounded-2xl border border-[#e8e0c8] bg-[#fefcf6] p-4">
              <div className="text-sm font-semibold">Create alias</div>
              <div className="mt-2 flex flex-col md:flex-row gap-2">
                <select value={newAliasMbId} onChange={e => setNewAliasMbId(e.target.value)} className="rounded-full border border-[#e8e0c8] px-4 py-2 text-sm">
                  <option value="">Select mailbox</option>
                  {mailboxes.map((mb: any) => <option key={mb.id} value={mb.id}>{mb.address}</option>)}
                </select>
                <input value={newAliasEmail} onChange={e => setNewAliasEmail(e.target.value)} placeholder="alias@domain.com" className="flex-1 rounded-full border border-[#e8e0c8] px-4 py-2 text-sm" />
                <button onClick={createAlias} className="rounded-full bg-[#005a5e] px-6 py-2 text-sm font-semibold text-white">Create</button>
              </div>
              <p className="mt-2 text-xs text-zinc-500">Alias appears in Compose From dropdown. Domain must be verified.</p>
            </div>
            <div className="rounded-2xl border border-[#e8e0c8] bg-white overflow-hidden">
              <table className="w-full text-sm">
                <thead className="bg-[#f8f6ef] text-xs text-zinc-500">
                  <tr><th className="px-4 py-2 text-left">Alias</th><th className="px-4 py-2 text-left">Mailbox</th><th className="px-4 py-2">Actions</th></tr>
                </thead>
                <tbody>
                  {aliases.map((a: any) => (
                    <tr key={a.id} className="border-t border-[#f0ece0]">
                      <td className="px-4 py-2 font-mono text-xs">{a.alias_email}</td>
                      <td className="px-4 py-2 text-xs">{a.mailbox}</td>
                      <td className="px-4 py-2 text-center">
                        <button onClick={async () => { await fetch(`${API}/v1/send-as/${a.id}`, { method: "DELETE" }); loadAll(); }} className="text-xs text-red-600 hover:underline">Delete</button>
                      </td>
                    </tr>
                  ))}
                  {aliases.length === 0 && <tr><td colSpan={3} className="px-4 py-6 text-center text-sm text-zinc-400">No aliases</td></tr>}
                </tbody>
              </table>
            </div>
          </div>
        )}

        {tab === "logs" && (
          <div className="space-y-4">
            <h2 className="text-xl font-bold">Audit Logs</h2>
            <div className="rounded-2xl border border-[#e8e0c8] bg-white overflow-hidden">
              <table className="w-full text-sm">
                <thead className="bg-[#f8f6ef] text-xs text-zinc-500">
                  <tr><th className="px-4 py-2 text-left">Time</th><th className="px-4 py-2 text-left">Action</th><th className="px-4 py-2 text-left">Target</th></tr>
                </thead>
                <tbody>
                  {logs.slice(0, 50).map((l: any) => (
                    <tr key={l.id} className="border-t border-[#f0ece0]">
                      <td className="px-4 py-2 text-xs">{new Date(l.created_at).toLocaleString()}</td>
                      <td className="px-4 py-2 text-xs font-mono">{l.action}</td>
                      <td className="px-4 py-2 text-xs">{l.target_id || l.mailbox_id || "-"}</td>
                    </tr>
                  ))}
                  {logs.length === 0 && <tr><td colSpan={3} className="px-4 py-6 text-center text-sm text-zinc-400">No logs</td></tr>}
                </tbody>
              </table>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
