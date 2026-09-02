"use client";
import { useState } from "react";
const API = process.env.NEXT_PUBLIC_MAIL_API || "http://localhost:8095";

export default function LoginPage() {
  const [email, setEmail] = useState("admin@aivory.id");
  const [password, setPassword] = useState("");
  const [err, setErr] = useState("");
  const [loading, setLoading] = useState(false);

  async function doLogin(e: React.FormEvent) {
    e.preventDefault();
    setErr("");
    setLoading(true);
    try {
      const r = await fetch(`${API}/v1/auth/login`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ email, password }),
      });
      const j = await r.json();
      if (!j.success || j.error) {
        setErr(j.error || "Login failed");
        setLoading(false);
        return;
      }
      const token = j.data?.token;
      if (token) {
        localStorage.setItem("aivory_mail_token", token);
        localStorage.setItem("aivory_mail_email", j.data.email || email);
        document.cookie = `aivory_mail_token=${token}; path=/; max-age=604800`;
        window.location.href = "/";
      } else {
        setErr("No token returned");
      }
    } catch (e: any) {
      setErr(e.message || "Network error");
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="flex min-h-screen items-center justify-center bg-[#f8f6ef] p-4">
      <div className="w-full max-w-md rounded-2xl border border-[#e8e0c8] bg-[#fefcf6] p-8 shadow-lg">
        <div className="flex flex-col items-center">
          <img src="/aivory-mail-logo2.svg" alt="Aivory Mail" className="w-[220px] h-auto object-contain" />
          <h1 className="mt-6 text-2xl font-bold text-[#202124]">Sign in to Aivory Mail</h1>
          <p className="mt-1 text-sm text-zinc-500">Business email, without the email tax.</p>
        </div>

        <form onSubmit={doLogin} className="mt-8 space-y-4">
          <div>
            <label className="text-sm font-medium text-zinc-700">Email</label>
            <input
              type="email"
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              placeholder="admin@aivory.id or your mailbox"
              className="mt-1 w-full rounded-full border border-[#e8e0c8] bg-white px-4 py-2.5 text-sm focus:border-[#005a5e] focus:outline-none"
              required
            />
          </div>
          <div>
            <label className="text-sm font-medium text-zinc-700">Password</label>
            <input
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              placeholder="••••••••"
              className="mt-1 w-full rounded-full border border-[#e8e0c8] bg-white px-4 py-2.5 text-sm focus:border-[#005a5e] focus:outline-none"
              required
            />
            <p className="mt-1 text-xs text-zinc-400">Default: <span className="font-mono">aivory123</span> (set MAIL_ADMIN_PASSWORD in production)</p>
          </div>

          {err && <div className="rounded-xl bg-red-50 px-3 py-2 text-sm text-red-700 ring-1 ring-red-200">{err}</div>}

          <button
            type="submit"
            disabled={loading}
            className="w-full rounded-full bg-[#005a5e] py-3 text-sm font-semibold text-white shadow hover:bg-[#00454a] disabled:opacity-50 active:scale-[0.98] transition-transform"
          >
            {loading ? "Signing in..." : "Sign in →"}
          </button>
        </form>

        <div className="mt-6 rounded-xl bg-[#f0ece0] p-3 text-xs leading-relaxed text-zinc-600">
          <div className="font-semibold text-[#202124]">Demo access</div>
          <div>Email: <span className="font-mono">admin@aivory.id</span> · Password: <span className="font-mono">aivory123</span></div>
          <div className="mt-1">Or use any mailbox address (e.g. <span className="font-mono">hello@demo.aivory.test</span>) with the same password if the mailbox exists.</div>
        </div>

        <div className="mt-6 flex justify-center gap-4 text-xs">
          <a href="https://aivory.id" className="text-zinc-400 hover:text-[#005a5e]">Aivory</a>
          <span className="text-zinc-300">·</span>
          <a href="/admin" className="text-[#005a5e] hover:underline">Admin →</a>
        </div>
      </div>
    </div>
  );
}
