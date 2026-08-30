"use client";
import { useEffect, useState } from "react";
import { useParams, useSearchParams } from "next/navigation";
const API = process.env.NEXT_PUBLIC_MAIL_API || "http://localhost:8095";
export default function SharePage() {
  const params = useParams() as { id: string };
  const search = useSearchParams();
  const [msg, setMsg] = useState<any>(null);
  const [err, setErr] = useState("");
  useEffect(() => {
    const t = search.get("t");
    if (!params?.id || !t) { setErr("Missing share token"); return; }
    fetch(`${API}/v1/share/${params.id}?t=${encodeURIComponent(t)}`)
      .then(r=>r.json()).then(j=> j.success ? setMsg(j.data) : setErr(j.error||"Invalid link")).catch(e=>setErr(String(e)));
  }, [params?.id, search]);
  if (err) return <div className="flex h-screen items-center justify-center p-8"><div className="rounded-xl border border-red-200 bg-red-50 px-6 py-4 text-sm text-red-700">{err}</div></div>;
  if (!msg) return <div className="flex h-screen items-center justify-center text-sm text-zinc-500">Loading shared message...</div>;
  return (
    <div className="min-h-screen bg-zinc-50 p-6">
      <div className="mx-auto max-w-2xl rounded-xl border border-zinc-200 bg-white p-6 shadow-sm">
        <div className="mb-2 text-xs font-semibold text-zinc-500">Shared via Aivory Mail — read-only</div>
        <h1 className="text-lg font-bold">{msg.subject}</h1>
        <div className="mt-2 text-xs text-zinc-500">From {msg.from} · {msg.created_at}</div>
        <div className="mt-4 whitespace-pre-wrap text-sm leading-6">{msg.body_text}</div>
        {msg.body_html && <div className="prose prose-sm mt-4 max-w-none rounded-lg border bg-zinc-50 p-4" dangerouslySetInnerHTML={{__html: msg.body_html}} />}
      </div>
    </div>
  );
}
