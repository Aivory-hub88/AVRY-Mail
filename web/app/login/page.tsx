"use client";
import { useState } from "react";
const API = process.env.NEXT_PUBLIC_MAIL_API || "http://localhost:8095";

function Ico({ d, size = 16, cls = "" }: { d: string; size?: number; cls?: string }) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.6} vectorEffect="non-scaling-stroke" shapeRendering="geometricPrecision" strokeLinecap="round" strokeLinejoin="round" className={cls} aria-hidden><path d={d} /></svg>;
}
const P = {
  lock: "M8 11V7a4 4 0 118 0v4 M5 11h14a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2z",
  eye: "M2 12s3-7 10-7 10 7 10 7-3 7-10 7S2 12 2 12z M12 15a3 3 0 100-6 3 3 0 000 6z",
  eyeOff: "M17.94 17.94A10.07 10.07 0 0112 20c-7 0-11-8-11-8a18.45 18.45 0 015.06-5.94 M9.53 9.53a3 3 0 104.95 4.95 M1 1l22 22",
  google: "M12 2a10 10 0 0110 10 10 10 0 01-2.93 7.07A10 10 0 012 12a10 10 0 0110-10z M12 8v4h5.66A4 4 0 0012 8z",
  apple: "M12 2a4.5 4.5 0 013 1.2A4 4 0 0012 7a4 4 0 00-3-1.2A4.5 4.5 0 0012 2z M9 12c0 2 1.5 4 3 4s3-2 3-4-1.5-4-3-4-3 2-3 4z",
  github: "M9 19c-4 1.5-4-1-4-1 0-1 1-2 1-2l2 1c1 1 2 1 3 0l2-1s1 1 1 2c0 0 0 2-4 1z M12 2a10 10 0 00-3.2 19.5c.5.1.7-.2.7-.5v-1.7c-2.7.6-3.3-1.2-3.3-1.2-.4-1-1-1.3-1-1.3-.8-.6.1-.6.1-.6 1 0 1.5 1 1.5 1 .9 1.5 2.3 1 2.9.8.1-.6.4-1 .7-1.3-2.2-.2-4.5-1.1-4.5-4.9 0-1.1.4-2 1-2.7 0-.2-.4-1.2.1-2.6 0 0 .8-.3 2.7 1 .8-.2 1.6-.3 2.5-.3s1.7.1 2.5.3c1.9-1.3 2.7-1 2.7-1 .5 1.4.2 2.4.1 2.6.6.7 1 1.6 1 2.7 0 3.8-2.3 4.7-4.6 4.9.4.3.7.9.7 1.8v2.7c0 .3.2.6.7.5A10 10 0 0012 2z",
};

export default function LoginPage() {
  const [email, setEmail] = useState("admin@aivory.id");
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
        <div className="flex flex-1 items-center justify-center px-6 py-8">
          <div className="w-full max-w-[380px]">
            <div className="flex justify-center">
              <img src="/aivory-mail-logo3.svg" alt="Aivory Mail" className="w-[214px] h-auto object-contain" />
            </div>

            <form onSubmit={doLogin} className="mt-8 space-y-4">
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
                  <button type="button" onClick={() => setShowPass(!showPass)} className="absolute right-2 top-1/2 -translate-y-1/2 rounded p-1 text-zinc-500 hover:bg-zinc-100" aria-label={showPass ? "Hide password" : "Show password"}>
                    <Ico d={showPass ? P.eyeOff : P.eye} size={16} cls="text-zinc-500" />
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
            </div>
          </div>
        </div>
      </div>

      {/* Right — Aivory palette dot map (Cloudflare orange → Aivory teal) */}
      <div className="relative hidden w-[48%] flex-col overflow-hidden bg-gradient-to-br from-[#005a5e] via-[#0a4a4d] to-[#083a3d] p-8 text-white lg:flex">
        <div className="flex items-center justify-end gap-3 text-sm">
          <button className="flex items-center gap-1 rounded-lg bg-white/10 px-3 py-1.5 backdrop-blur hover:bg-white/20">
            <Ico d="M12 2a10 10 0 1010 10A10 10 0 0012 2z M2 12h20 M12 2a15 15 0 010 20 M12 2a15 15 0 000 20" size={14} cls="text-white" /> English <span className="text-xs">▾</span>
          </button>
          <a href="#" onClick={(e) => { e.preventDefault(); }} className="rounded-lg bg-white px-4 py-1.5 text-sm font-semibold text-zinc-900 hover:bg-zinc-100">Sign up</a>
        </div>

        <div className="pointer-events-none absolute inset-0 opacity-20">
          <div className="absolute inset-0" style={{
            backgroundImage: `radial-gradient(circle, #f8f6ef 1.2px, transparent 1.2px)`,
            backgroundSize: `14px 14px`,
            maskImage: `radial-gradient(ellipse 80% 60% at 70% 50%, black 40%, transparent 75%)`
          }} />
          <div className="absolute right-[8%] top-[18%] h-64 w-64 rounded-lg bg-white/10 blur-2xl" />
          <div className="absolute bottom-[12%] right-[22%] h-48 w-48 rounded-lg bg-[#f0ece0]/10 blur-xl" />
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
            className="mt-4 inline-flex w-fit items-center gap-2 rounded-lg bg-white px-4 py-2 text-sm font-semibold text-[#005a5e] hover:bg-zinc-100"
          >
            <Ico d="M10 13a5 5 0 010-7l1-1a5 5 0 017 7l-1 1 M14 11a5 5 0 010 7l-1 1a5 5 0 01-7-7l1-1" size={12} cls="text-[#005a5e]" /> Register now
          </a>
        </div>

        <div className="absolute bottom-0 right-0 h-2 w-full bg-white/10" />
      </div>
    </div>
  );
}
