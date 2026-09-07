"use client";
import { useEffect, useState } from "react";
const API = process.env.NEXT_PUBLIC_MAIL_API || "http://localhost:8095";

function authFetch(path: string, opts: RequestInit = {}) {
  const token = typeof window !== "undefined" ? localStorage.getItem("aivory_mail_token") : null;
  const headers: Record<string, string> = { ...(opts.headers as Record<string, string> | undefined) };
  if (token) headers["Authorization"] = `Bearer ${token}`;
  return fetch(`${API}${path}`, { ...opts, headers });
}

type Integration = {
  host: string;
  port: number;
  username: string;
  address: string;
  status: string;
  connected: boolean;
  has_password: boolean;
  last_tested_at?: string | null;
  last_connected_at?: string | null;
  updated_at?: string | null;
};

export default function IntegrationsPage() {
  const [sub, setSub] = useState<"email">("email");
  const [me, setMe] = useState<{ email: string; mailbox_id: string | null; address: string | null } | null>(null);
  const [integration, setIntegration] = useState<Integration | null>(null);
  const [loading, setLoading] = useState(true);
  const [msg, setMsg] = useState("");

  // form state (only shown when disconnected / reconnect)
  const [host, setHost] = useState("mail.aivory.uk");
  const [port, setPort] = useState("993");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [showPw, setShowPw] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testOk, setTestOk] = useState<boolean | null>(null);
  const [testMsg, setTestMsg] = useState("");
  const [saving, setSaving] = useState(false);
  const [disconnecting, setDisconnecting] = useState(false);
  const [showForm, setShowForm] = useState(false);

  async function loadMe() {
    try {
      const r = await authFetch("/v1/auth/me");
      if (r.status === 401) { window.location.href = "/login"; return null; }
      const j = await r.json();
      const d = j.data || {};
      setMe({ email: d.email, mailbox_id: d.mailbox_id, address: d.address || d.email });
      return d;
    } catch { return null; }
  }

  async function loadIntegration() {
    try {
      const r = await authFetch("/v1/integrations/email");
      const j = await r.json();
      if (j.success && j.data) {
        const d = j.data as Integration;
        setIntegration(d);
        // prefill form defaults when disconnected
        if (!d.connected) {
          setHost(d.host || "mail.aivory.uk");
          setPort(String(d.port || 993));
          setUsername(d.username || d.address || me?.address || me?.email || "");
        }
        setShowForm(!d.connected);
      }
    } catch {}
    setLoading(false);
  }

  useEffect(() => {
    (async () => {
      const d = await loadMe();
      if (d) {
        // set initial username from me
        setUsername(d.address || d.email || "");
      }
      await loadIntegration();
    })();
  }, []);

  // keep username in sync when me loads but integration hasn't
  useEffect(() => {
    if (me?.address && !username && integration && !integration.connected) {
      setUsername(me.address);
    }
  }, [me, integration]);

  async function doTest() {
    setTesting(true); setTestOk(null); setTestMsg("");
    try {
      const r = await authFetch("/v1/integrations/email/test", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ host: host.trim(), port: parseInt(port || "993", 10), username: username.trim(), password }),
      });
      const j = await r.json();
      if (j.success) { setTestOk(true); setTestMsg(j.message || "Connection test passed"); }
      else { setTestOk(false); setTestMsg(j.error || "Test failed"); }
    } catch (e: any) {
      setTestOk(false); setTestMsg(e?.message || "Test failed");
    }
    setTesting(false);
  }

  async function doSave() {
    if (!testOk) { setMsg("Test connection dulu sebelum save."); return; }
    if (password.length < 8) { setMsg("Password minimal 8 karakter"); return; }
    setSaving(true); setMsg("");
    try {
      const r = await authFetch("/v1/integrations/email", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ host: host.trim(), port: parseInt(port || "993", 10), username: username.trim(), password }),
      });
      const j = await r.json();
      if (!j.success) { setMsg(j.error || "Failed to save"); }
      else {
        setMsg("Email account connected");
        setPassword(""); setTestOk(null); setTestMsg("");
        await loadIntegration();
        setShowForm(false);
      }
    } catch (e: any) { setMsg(e?.message || "Failed to save"); }
    setSaving(false);
  }

  async function doDisconnect() {
    if (!confirm("Disconnect email account? Password IMAP akan dihapus — webmail tetap bisa login, tapi mail client (IMAP/SMTP) akan logout.")) return;
    setDisconnecting(true);
    try {
      const r = await authFetch("/v1/integrations/email", { method: "DELETE" });
      const j = await r.json();
      if (j.success) { setMsg("Disconnected"); setIntegration(prev => prev ? { ...prev, connected: false, status: "disconnected", has_password: false } : null); setShowForm(true); setTestOk(null); setPassword(""); }
      else setMsg(j.error || "Failed to disconnect");
    } catch {}
    setDisconnecting(false);
    loadIntegration();
  }

  const connected = integration?.connected === true;

  if (loading) {
    return <div className="min-h-screen bg-[#f8f6ef] flex items-center justify-center text-sm text-zinc-500">Loading…</div>;
  }

  return (
    <div className="min-h-screen bg-[#f8f6ef] font-[Manrope]">
      <div className="mx-auto max-w-5xl p-6">
        <div className="flex items-center justify-between">
          <div className="text-sm text-zinc-500"><a href="/settings" className="underline">Settings</a> / <a href="/settings/integrations" className="underline">Integrations</a> / <span className="font-semibold text-[#202124]">Email Account</span></div>
          <div className="flex gap-2">
            <a href="/settings/mail" target="_top" className="rounded-lg border border-[#e8e0c8] bg-[#fefcf6] px-3 py-1 text-xs">← Mail settings</a>
            <a href="/settings" className="rounded-lg border border-[#e8e0c8] bg-[#fefcf6] px-3 py-1 text-xs">← Overview</a>
          </div>
        </div>
        <h1 className="mt-2 text-3xl font-bold">Integrations</h1>
        <p className="mt-1 text-sm text-zinc-500">Kelola koneksi akun email untuk akses IMAP/SMTP — terpisah dari password web login.</p>

        <div className="mt-6 flex gap-6">
          <nav className="hidden w-52 shrink-0 flex-col gap-1 lg:flex">
            <button onClick={() => setSub("email")} className={`rounded-lg px-3 py-2 text-left text-sm ${sub==="email" ? "bg-[#ccc1a8] text-[#202124]" : "hover:bg-[#fefcf6] border border-transparent hover:border-[#e8e0c8]"}`}>Email Account</button>
            <div className="mt-2 text-xs text-zinc-400 px-3">Lainnya segera</div>
          </nav>
          <div className="flex-1 space-y-4">
            <div className="flex gap-2 lg:hidden overflow-x-auto pb-2">
              <button onClick={() => setSub("email")} className={`whitespace-nowrap rounded-lg px-3 py-1.5 text-xs ${sub==="email" ? "bg-[#ccc1a8] text-[#202124]" : "bg-[#fefcf6] border"}`}>Email Account</button>
            </div>

            {msg && <div className="rounded-xl bg-amber-50 px-4 py-2 text-sm text-amber-800 ring-1 ring-amber-200">{msg} <button onClick={() => setMsg("")} className="ml-2 text-xs underline">×</button></div>}

            {sub==="email" && (
              <>
                {connected && !showForm ? (
                  <div className="rounded-2xl border border-emerald-200 bg-white p-5 shadow-sm">
                    <div className="flex items-start justify-between gap-3">
                      <div>
                        <div className="flex items-center gap-2">
                          <span className="h-2.5 w-2.5 rounded-full bg-emerald-500 animate-pulse" />
                          <h3 className="font-semibold text-[#202124]">Connected</h3>
                          <span className="rounded-lg bg-emerald-50 px-2 py-0.5 text-xs font-medium text-emerald-700">IMAP ready</span>
                        </div>
                        <p className="mt-1 text-sm text-zinc-600">Connected as <span className="font-mono font-semibold text-[#202124]">{integration?.username || integration?.address}</span></p>
                        <p className="mt-1 text-xs text-zinc-500 font-mono">{integration?.host}:{integration?.port} · IMAP 993 SSL · SMTP 587 STARTTLS · username = full address</p>
                        {integration?.updated_at && <p className="mt-1 text-xs text-zinc-400">Last updated {new Date(integration.updated_at).toLocaleString()}</p>}
                      </div>
                      <div className="flex gap-2">
                        <button onClick={() => setShowForm(true)} className="rounded-lg border border-[#e8e0c8] bg-[#fefcf6] px-4 py-1.5 text-xs font-medium hover:bg-[#f8f6ef]">Reconnect</button>
                        <button onClick={doDisconnect} disabled={disconnecting} className="rounded-lg border border-red-200 bg-white px-4 py-1.5 text-xs font-medium text-red-600 hover:bg-red-50 disabled:opacity-50">{disconnecting ? "…" : "Disconnect"}</button>
                      </div>
                    </div>
                    <div className="mt-4 rounded-xl bg-[#f8f6ef] px-3 py-2 text-xs text-zinc-500">
                      Password tidak pernah ditampilkan lagi setelah save — seperti App Passwords. Jika lupa, gunakan Reconnect untuk set password baru. Admin juga bisa cek status & reset via Admin Console.
                    </div>
                  </div>
                ) : (
                  <div className="rounded-2xl border border-[#e8e0c8] bg-[#fefcf6] p-5 shadow-sm">
                    <h3 className="font-semibold text-[#202124]">Email Account · IMAP</h3>
                    <p className="mt-1 text-xs text-zinc-500">Isi host/port/username/password IMAP. Test connection dulu sebelum Save. Setelah tersimpan, password tidak akan dirender lagi — hanya status Connected.</p>

                    {!me?.mailbox_id && (
                      <div className="mt-3 rounded-xl bg-amber-50 px-3 py-2 text-xs text-amber-800">Akun mailbox belum ditemukan untuk {me?.email}. Buat mailbox dulu di Admin → Accounts.</div>
                    )}

                    <div className="mt-4 grid gap-4">
                      <div className="grid md:grid-cols-3 gap-3">
                        <label className="flex flex-col gap-1 text-sm">
                          <span className="text-xs font-medium text-zinc-600">IMAP host</span>
                          <input value={host} onChange={e=> {setHost(e.target.value); setTestOk(null);}} placeholder="mail.aivory.uk" className="rounded-lg border border-[#e8e0c8] bg-white px-3 py-2 text-sm font-mono focus:border-[#ccc1a8] focus:outline-none" />
                        </label>
                        <label className="flex flex-col gap-1 text-sm">
                          <span className="text-xs font-medium text-zinc-600">Port</span>
                          <input value={port} onChange={e=> {setPort(e.target.value); setTestOk(null);}} placeholder="993" inputMode="numeric" className="rounded-lg border border-[#e8e0c8] bg-white px-3 py-2 text-sm font-mono focus:border-[#ccc1a8] focus:outline-none" />
                        </label>
                        <label className="flex flex-col gap-1 text-sm">
                          <span className="text-xs font-medium text-zinc-600">Username</span>
                          <input value={username} onChange={e=> {setUsername(e.target.value); setTestOk(null);}} placeholder="you@domain.com" className="rounded-lg border border-[#e8e0c8] bg-white px-3 py-2 text-sm font-mono focus:border-[#ccc1a8] focus:outline-none" />
                        </label>
                      </div>

                      <label className="flex flex-col gap-1 text-sm">
                        <span className="text-xs font-medium text-zinc-600">Password</span>
                        <div className="flex gap-2">
                          <div className="relative flex-1">
                            <input
                              type={showPw ? "text" : "password"}
                              value={password}
                              onChange={e=> {setPassword(e.target.value); setTestOk(null); setTestMsg("");}}
                              placeholder="IMAP password (min 8 chars)"
                              className="w-full rounded-lg border border-[#e8e0c8] bg-white px-3 py-2 pr-10 text-sm font-mono focus:border-[#ccc1a8] focus:outline-none"
                            />
                            <button type="button" onClick={()=> setShowPw(v=>!v)} className="absolute right-2 top-1/2 -translate-y-1/2 rounded p-1 text-zinc-400 hover:bg-zinc-100" title={showPw ? "Hide" : "Show"}>
                              {showPw ? (
                                <svg className="h-4 w-4" fill="none" stroke="currentColor" strokeWidth="1.5" viewBox="0 0 24 24"><path strokeLinecap="round" strokeLinejoin="round" d="M3.98 8.223A10.477 10.477 0 0 0 1.934 12C3.226 16.338 7.244 19.5 12 19.5c.993 0 1.953-.138 2.863-.395M6.228 6.228A10.45 10.45 0 0 1 12 4.5c4.756 0 8.773 3.162 10.065 7.498a10.523 10.523 0 0 1-4.293 5.774M6.228 6.228 3 3m3.228 3.228 3.65 3.65m7.894 7.894L21 21m-3.228-3.228-3.65-3.65m0 0a3 3 0 1 0-4.243-4.243m4.243 4.243L9.88 9.88"/></svg>
                              ) : (
                                <svg className="h-4 w-4" fill="none" stroke="currentColor" strokeWidth="1.5" viewBox="0 0 24 24"><path strokeLinecap="round" strokeLinejoin="round" d="M2.036 12.322a1.012 1.012 0 0 1 0-.639C3.423 7.51 7.36 4.5 12 4.5c4.638 0 8.573 3.007 9.963 7.178.07.207.07.431 0 .639C20.577 16.49 16.64 19.5 12 19.5c-4.638 0-8.573-3.007-9.963-7.178Z"/><path strokeLinecap="round" strokeLinejoin="round" d="M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z"/></svg>
                              )}
                            </button>
                          </div>
                          <button type="button" onClick={()=> setShowPw(v=>!v)} className="rounded-lg border border-[#e8e0c8] bg-white px-3 py-2 text-xs hover:bg-[#f8f6ef]">{showPw ? "Hide" : "Show"}</button>
                        </div>
                        <span className="text-xs text-zinc-400">Password IMAP terpisah dari password web login — App-passwords parity.</span>
                      </label>

                      <div className="flex flex-wrap items-center gap-2">
                        <button onClick={doTest} disabled={testing || !host || !username || !password} className="rounded-lg border border-[#005a5e] bg-white px-4 py-2 text-sm font-medium text-[#005a5e] hover:bg-[#f0f7f7] disabled:opacity-50">
                          {testing ? "Testing…" : "Test connection"}
                        </button>
                        {testOk === true && <span className="text-xs font-medium text-emerald-700">✓ {testMsg}</span>}
                        {testOk === false && <span className="text-xs font-medium text-red-600">✗ {testMsg}</span>}
                        {testOk === null && <span className="text-xs text-zinc-400">Wajib test sebelum Save</span>}
                      </div>

                      <div className="flex justify-between gap-2 pt-2 border-t border-[#f0ece0]">
                        {connected && <button onClick={()=> {setShowForm(false); setTestOk(null);}} className="rounded-lg border border-[#e8e0c8] bg-white px-4 py-2 text-sm hover:bg-[#f8f6ef]">Cancel</button>}
                        {!connected && <div />}
                        <button onClick={doSave} disabled={saving || testOk !== true} className="rounded-lg bg-[#005a5e] px-6 py-2 text-sm font-semibold text-white hover:bg-[#00454a] disabled:opacity-40">
                          {saving ? "Saving…" : "Save & connect"}
                        </button>
                      </div>
                    </div>
                  </div>
                )}

                <div className="rounded-xl border border-dashed border-[#e8e0c8] bg-white p-4 text-xs text-zinc-500">
                  <div className="font-medium text-zinc-600">Untuk mail client (Thunderbird/Apple Mail/Outlook)</div>
                  <div className="mt-1 font-mono">IMAP: {host || "mail.aivory.uk"}:993 SSL · SMTP: 587 STARTTLS · username = full address</div>
                  <div className="mt-1">Setelah Save, kredensial ini dipakai Dovecot (993) & submission (587). Revoke via Disconnect — web login tetap jalan.</div>
                </div>
              </>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
