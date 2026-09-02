"use client";
import { useState } from "react";
const API = process.env.NEXT_PUBLIC_MAIL_API || "http://localhost:8095";

export default function LoginPage() {
  const [email, setEmail] = useState("irfan.reichmann@aivory.id");
  const [password, setPassword] = useState("");
  const [showPass, setShowPass] = useState(false);
  const [save, setSave] = useState(true);
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
        if (save) localStorage.setItem("aivory_mail_saved_email", email);
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
    <div className="flex min-h-screen bg-white">
      {/* Left — Cloudflare-style form, Aivory palette */}
      <div className="flex w-full flex-col lg:w-[52%]">
        <div className="flex items-center gap-2 px-6 py-4">
          <img src="/aivory-mail-logo2.svg" alt="Aivory Mail" className="w-[260px] h-auto object-contain object-left" />
        </div>

        <div className="flex flex-1 items-center justify-center px-6 py-8">
          <div className="w-full max-w-[380px]">
            <h1 className="text-2xl font-bold text-zinc-900">Sign in to Aivory Mail</h1>

            {/* SSO */}
            <div className="mt-6 grid grid-cols-3 gap-2">
              <button onClick={() => setErr("Google SSO coming soon — use email/password")} className="flex items-center justify-center gap-1.5 rounded-lg border border-zinc-200 bg-white px-3 py-2 text-sm font-medium hover:bg-zinc-50">
                <span className="flex h-4 w-4 items-center justify-center rounded-full bg-white text-xs">G</span> Google
              </button>
              <button onClick={() => setErr("Apple SSO coming soon")} className="flex items-center justify-center gap-1.5 rounded-lg border border-zinc-200 bg-white px-3 py-2 text-sm font-medium hover:bg-zinc-50">
                <span className="text-sm"></span> Apple
              </button>
              <button onClick={() => setErr("GitHub SSO coming soon")} className="flex items-center justify-center gap-1.5 rounded-lg border border-zinc-200 bg-white px-3 py-2 text-sm font-medium hover:bg-zinc-50">
                <span className="text-xs">⬡</span> GitHub
              </button>
            </div>
            <button onClick={() => setErr("SSO via SAML coming soon")} className="mt-2 flex w-full items-center justify-center gap-2 rounded-lg border border-zinc-200 bg-white px-3 py-2 text-sm font-medium hover:bg-zinc-50">
              <span className="text-zinc-500">🔒</span> Continue with SSO
            </button>

            <div className="my-5 flex items-center gap-3">
              <div className="h-px flex-1 bg-zinc-200" />
              <span className="text-xs tracking-widest text-zinc-500">OR</span>
              <div className="h-px flex-1 bg-zinc-200" />
            </div>

            <form onSubmit={doLogin} className="space-y-4">
              <div>
                <label className="text-sm font-medium text-zinc-900">Email</label>
                <input
                  type="email"
                  value={email}
                  onChange={(e) => setEmail(e.target.value)}
                  className="mt-1 w-full rounded-lg border border-zinc-200 bg-white px-3 py-2.5 text-sm focus:border-[#005a5e] focus:outline-none focus:ring-1 focus:ring-[#005a5e]"
                  required
                />
              </div>
              <div>
                <label className="text-sm font-medium text-zinc-900">Password</label>
                <div className="relative mt-1">
                  <input
                    type={showPass ? "text" : "password"}
                    value={password}
                    onChange={(e) => setPassword(e.target.value)}
                    className="w-full rounded-lg border border-zinc-300 bg-white px-3 py-2.5 pr-10 text-sm focus:border-[#005a5e] focus:outline-none focus:ring-1 focus:ring-[#005a5e]"
                    required
                  />
                  <button type="button" onClick={() => setShowPass(!showPass)} className="absolute right-2 top-1/2 -translate-y-1/2 rounded p-1 text-zinc-500 hover:bg-zinc-100">
                    {showPass ? "🙈" : "👁"}
                  </button>
                </div>
              </div>

              <label className="flex items-center gap-2 text-sm">
                <input type="checkbox" checked={save} onChange={(e) => setSave(e.target.checked)} className="h-4 w-4 rounded border-zinc-300 bg-black text-white accent-black" />
                <span className="text-zinc-700">Save email and login method on this device</span>
              </label>

              {err && <div className="rounded-lg bg-red-50 px-3 py-2 text-sm text-red-700 ring-1 ring-red-200">{err}</div>}

              <button
                type="submit"
                disabled={loading}
                className="w-full rounded-lg bg-[#005a5e] py-2.5 text-sm font-semibold text-white shadow hover:bg-[#00454a] disabled:opacity-50 active:scale-[0.98] transition-transform"
              >
                {loading ? "Signing in..." : "Sign in"}
              </button>
            </form>

            <button className="mt-4 w-full text-center text-sm font-medium text-zinc-900 hover:underline">View saved profiles</button>

            <div className="mt-6 text-center text-sm">
              <span className="text-zinc-600">Don&apos;t have an account? </span>
              <a href="#" onClick={(e) => { e.preventDefault(); setErr("Contact admin to create account — admin@aivory.id"); }} className="font-medium text-[#005a5e] hover:underline">Sign up</a>
            </div>
            <div className="text-center text-sm">
              <span className="text-zinc-600">Forgot your </span>
              <a href="#" className="font-medium text-[#005a5e] hover:underline">email</a>
              <span className="text-zinc-600"> or </span>
              <a href="#" className="font-medium text-[#005a5e] hover:underline">password</a>
              <span className="text-zinc-600">?</span>
            </div>

            <div className="mt-8 text-xs leading-relaxed text-zinc-500">
              By continuing, I agree to Aivory&apos;s <a href="#" className="underline">terms</a>, <a href="#" className="underline">privacy policy</a>, and <a href="#" className="underline">cookie policy</a>.
              <div className="mt-2 rounded-lg bg-[#f8f6ef] px-3 py-2 text-xs">
                <span className="font-semibold">Demo:</span> <span className="font-mono">admin@aivory.id / aivory123</span> or any mailbox with same password.
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* Right — Aivory palette dot map (Cloudflare orange → Aivory teal) */}
      <div className="relative hidden w-[48%] flex-col overflow-hidden bg-gradient-to-br from-[#005a5e] via-[#0a4a4d] to-[#083a3d] p-8 text-white lg:flex">
        <div className="flex items-center justify-end gap-3 text-sm">
          <button className="flex items-center gap-1 rounded-full bg-white/10 px-3 py-1.5 backdrop-blur hover:bg-white/20">
            <span>🌐</span> English <span className="text-xs">▾</span>
          </button>
          <a href="#" onClick={(e) => { e.preventDefault(); }} className="rounded-full bg-white px-4 py-1.5 text-sm font-semibold text-zinc-900 hover:bg-zinc-100">Sign up</a>
        </div>

        {/* dotted world map — CSS pattern */}
        <div className="pointer-events-none absolute inset-0 opacity-20">
          <div className="absolute inset-0" style={{
            backgroundImage: `radial-gradient(circle, #f8f6ef 1.2px, transparent 1.2px)`,
            backgroundSize: `14px 14px`,
            maskImage: `radial-gradient(ellipse 80% 60% at 70% 50%, black 40%, transparent 75%)`
          }} />
          <div className="absolute right-[8%] top-[18%] h-64 w-64 rounded-full bg-white/10 blur-2xl" />
          <div className="absolute bottom-[12%] right-[22%] h-48 w-48 rounded-full bg-[#f0ece0]/10 blur-xl" />
        </div>

        <div className="relative z-10 mt-auto flex flex-col justify-end pb-12">
          <div className="font-mono text-sm tracking-widest text-white/80">Aivory Connect 2026</div>
          <h2 className="mt-2 text-3xl font-bold leading-tight">
            Where teams build<br />together.
          </h2>
          <p className="mt-3 text-sm text-white/80">October 19–21, 2026 · Jakarta Convention Center, Jakarta</p>
          <a
            href="https://aivory.id"
            target="_blank"
            className="mt-4 inline-flex w-fit items-center gap-2 rounded-full bg-white px-4 py-2 text-sm font-semibold text-[#005a5e] hover:bg-zinc-100"
          >
            <span className="text-xs">↗</span> Register now
          </a>
        </div>

        <div className="absolute bottom-0 right-0 h-2 w-full bg-white/10" />
      </div>
    </div>
  );
}
