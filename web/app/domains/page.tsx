"use client";
import { useEffect, useState } from "react";
const API = process.env.NEXT_PUBLIC_MAIL_API || "http://localhost:8095";

type Domain = { id: string; domain: string; status: string; created_at?: string };
type DnsRecord = {
  record_type: string;
  host: string;
  purpose: string;
  expected_value: string;
  priority: number | null;
  status: "Missing" | "Correct" | "Mismatch";
  found_values: string[];
};

const STATUS_STYLE: Record<string, string> = {
  Active: "bg-emerald-100 text-emerald-700",
  Pending: "bg-amber-100 text-amber-700",
  Verifying: "bg-amber-100 text-amber-700",
  Failed: "bg-red-100 text-red-700",
};

const RECORD_ICON: Record<string, string> = {
  Correct: "✅",
  Missing: "⏳",
  Mismatch: "❌",
};

const PURPOSE_LABEL: Record<string, string> = {
  verification: "Domain ownership",
  mx: "Inbound mail (MX)",
  spf: "SPF (sender authorization)",
  dkim: "DKIM (signing key)",
  dmarc: "DMARC (policy)",
};

export default function DomainsPage() {
  const [domains, setDomains] = useState<Domain[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [records, setRecords] = useState<DnsRecord[]>([]);
  const [failureReason, setFailureReason] = useState<string | null>(null);
  const [newDomain, setNewDomain] = useState("");
  const [loadingDns, setLoadingDns] = useState(false);
  const [verifying, setVerifying] = useState(false);
  const [copied, setCopied] = useState<string | null>(null);

  async function loadDomains() {
    const r = await fetch(`${API}/v1/domains`);
    const j = await r.json();
    setDomains(j.data || []);
  }
  useEffect(() => { loadDomains(); }, []);

  async function loadDetail(id: string) {
    setSelected(id);
    setLoadingDns(true);
    const [dnsRes, detailRes] = await Promise.all([
      fetch(`${API}/v1/domains/${id}/dns`),
      fetch(`${API}/v1/domains/${id}`),
    ]);
    const dnsJson = await dnsRes.json();
    const detailJson = await detailRes.json();
    setRecords(dnsJson.data?.records || []);
    setFailureReason(detailJson.data?.failure_reason || null);
    setLoadingDns(false);
  }

  async function addDomain() {
    if (!newDomain.trim()) return;
    const r = await fetch(`${API}/v1/domains`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ domain: newDomain.trim() }),
    });
    const j = await r.json();
    setNewDomain("");
    await loadDomains();
    if (j.data?.id) loadDetail(j.data.id);
  }

  async function verify(id: string) {
    setVerifying(true);
    const r = await fetch(`${API}/v1/domains/${id}/verify`, { method: "POST" });
    const j = await r.json();
    setVerifying(false);
    if (!j.success) setFailureReason(j.error || "Verification failed");
    await loadDomains();
    await loadDetail(id);
  }

  function copy(value: string) {
    navigator.clipboard?.writeText(value);
    setCopied(value);
    setTimeout(() => setCopied((c) => (c === value ? null : c)), 1500);
  }

  const selectedDomain = domains.find((d) => d.id === selected);

  return (
    <div className="min-h-screen bg-zinc-50 font-[Manrope]">
      <div className="mx-auto max-w-5xl p-6">
        <div className="text-sm text-zinc-500">
          <a href="/settings" className="underline">Settings</a> / <span className="font-semibold text-zinc-900">Domains</span>
        </div>
        <h1 className="mt-2 text-3xl font-bold">Domains</h1>
        <p className="mt-1 text-sm text-zinc-500">
          Add your own domain and send/receive mail from it — no nameserver migration needed, just add a few DNS records at your existing registrar.
        </p>

        <div className="mt-6 flex gap-6">
          {/* Domain list */}
          <div className="w-72 shrink-0 space-y-3">
            <div className="rounded-2xl border border-zinc-200 bg-white p-4">
              <div className="text-sm font-semibold">Add a domain</div>
              <div className="mt-2 flex gap-2">
                <input
                  value={newDomain}
                  onChange={(e) => setNewDomain(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && addDomain()}
                  placeholder="example.com"
                  className="flex-1 rounded-lg border border-zinc-200 px-3 py-1.5 text-sm"
                />
                <button onClick={addDomain} className="rounded-lg bg-zinc-900 px-3 py-1.5 text-sm font-medium text-white hover:bg-black">
                  Add
                </button>
              </div>
            </div>

            <div className="space-y-2">
              {domains.map((d) => (
                <button
                  key={d.id}
                  onClick={() => loadDetail(d.id)}
                  className={`w-full rounded-xl border p-3 text-left text-sm transition ${selected === d.id ? "border-zinc-900 bg-white shadow-sm" : "border-zinc-200 bg-white hover:border-zinc-300"}`}
                >
                  <div className="flex items-center justify-between gap-2">
                    <span className="truncate font-medium">{d.domain}</span>
                    <span className={`shrink-0 rounded-full px-2 py-0.5 text-xs ${STATUS_STYLE[d.status] || "bg-zinc-100 text-zinc-600"}`}>{d.status}</span>
                  </div>
                </button>
              ))}
              {domains.length === 0 && <div className="rounded-xl border border-dashed border-zinc-200 p-4 text-center text-xs text-zinc-400">No domains yet</div>}
            </div>
          </div>

          {/* Detail */}
          <div className="flex-1">
            {!selectedDomain && (
              <div className="rounded-2xl border border-dashed border-zinc-200 bg-white p-10 text-center text-sm text-zinc-400">
                Select a domain, or add one, to see its DNS setup checklist.
              </div>
            )}

            {selectedDomain && (
              <div className="space-y-4">
                <div className="rounded-2xl border border-zinc-200 bg-white p-5">
                  <div className="flex items-center justify-between">
                    <div>
                      <div className="text-lg font-semibold">{selectedDomain.domain}</div>
                      <span className={`mt-1 inline-block rounded-full px-2 py-0.5 text-xs ${STATUS_STYLE[selectedDomain.status] || "bg-zinc-100 text-zinc-600"}`}>{selectedDomain.status}</span>
                    </div>
                    <button
                      onClick={() => verify(selectedDomain.id)}
                      disabled={verifying}
                      className="rounded-full bg-zinc-900 px-5 py-2 text-sm font-semibold text-white hover:bg-black disabled:opacity-50"
                    >
                      {verifying ? "Checking…" : "Verify"}
                    </button>
                  </div>
                  {failureReason && selectedDomain.status !== "Active" && (
                    <div className="mt-3 rounded-lg bg-amber-50 px-3 py-2 text-xs text-amber-800">{failureReason}</div>
                  )}
                  {selectedDomain.status === "Active" && (
                    <div className="mt-3 rounded-lg bg-emerald-50 px-3 py-2 text-xs text-emerald-800">
                      Verified — hello@{selectedDomain.domain} mailboxes can send and receive mail.
                    </div>
                  )}
                </div>

                <div className="rounded-2xl border border-zinc-200 bg-white p-5">
                  <h3 className="font-semibold">DNS records</h3>
                  <p className="mt-1 text-xs text-zinc-500">
                    Add these at your domain's DNS provider (registrar, Cloudflare, etc). Changes can take a few minutes to a few hours to propagate.
                  </p>
                  <div className="mt-4 space-y-3">
                    {loadingDns && <div className="text-xs text-zinc-400">Checking DNS…</div>}
                    {!loadingDns && records.map((r, i) => (
                      <div key={i} className="rounded-xl border border-zinc-200 p-3">
                        <div className="flex items-center justify-between gap-2">
                          <div className="flex items-center gap-2 text-sm font-medium">
                            <span>{RECORD_ICON[r.status]}</span>
                            <span>{PURPOSE_LABEL[r.purpose] || r.purpose}</span>
                          </div>
                          <span className="rounded bg-zinc-100 px-2 py-0.5 font-mono text-xs">{r.record_type}{r.priority != null ? ` (priority ${r.priority})` : ""}</span>
                        </div>
                        <div className="mt-2 grid grid-cols-[3rem_1fr_auto] items-start gap-x-3 gap-y-1 text-xs">
                          <span className="text-zinc-400">Host</span>
                          <code className="break-all rounded bg-zinc-50 px-2 py-1">{r.host}</code>
                          <button onClick={() => copy(r.host)} className="rounded border border-zinc-200 px-2 py-1 text-zinc-500 hover:bg-zinc-50">
                            {copied === r.host ? "Copied" : "Copy"}
                          </button>
                          <span className="text-zinc-400">Value</span>
                          <code className="break-all rounded bg-zinc-50 px-2 py-1">{r.expected_value}</code>
                          <button onClick={() => copy(r.expected_value)} className="rounded border border-zinc-200 px-2 py-1 text-zinc-500 hover:bg-zinc-50">
                            {copied === r.expected_value ? "Copied" : "Copy"}
                          </button>
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
