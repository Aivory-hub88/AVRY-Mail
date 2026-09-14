"use client";
import { useEffect, useRef, useState } from "react";
import DOMPurify from "dompurify";

/** Plain-text twin of signature HTML (kept in sync on every save). */
export function sigToText(html: string): string {
  return html
    .replace(/<br\s*\/?>/gi, "\n")
    .replace(/<\/(p|div|li|h[1-6])>\s*<(p|div|li|h[1-6])[^>]*>/gi, "\n\n")
    .replace(/<[^>]+>/g, "")
    .replace(/&nbsp;/g, " ")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}

function escapeHtml(s: string) {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

/**
 * SignatureEditor — Skiff-style inline rich editor: type with B/I/U, see the
 * live rendered preview, one Save. No raw HTML, no names, no default flags.
 * Reused by Settings and the sidebar quick-edit so there is exactly one
 * signature UX in the product.
 */
export default function SignatureEditor({
  initialHtml = "",
  saveLabel = "Save signature",
  onSave,
}: {
  initialHtml?: string;
  saveLabel?: string;
  onSave: (html: string, text: string) => Promise<void> | void;
}) {
  const [html, setHtml] = useState(initialHtml);
  const [ekey, setEkey] = useState(0);
  const [saving, setSaving] = useState(false);
  const [err, setErr] = useState("");
  const ref = useRef<HTMLDivElement>(null);
  const imgRef = useRef<HTMLInputElement>(null);

  // Keep the contentEditable uncontrolled — React re-applying innerHTML on
  // every keystroke moves the caret to the start, so the next character is
  // inserted at the front and typing appears reversed ("tseB").
  useEffect(() => {
    setHtml(initialHtml);
    setEkey((k) => k + 1);
  }, [initialHtml]);

  useEffect(() => {
    if (ref.current) ref.current.innerHTML = initialHtml;
  }, [ekey, initialHtml]);

  function fmt(cmd: "bold" | "italic" | "underline") {
    ref.current?.focus();
    document.execCommand(cmd, false);
    if (ref.current) setHtml(ref.current.innerHTML);
  }

  function insertLink() {
    const url = prompt("URL — use https://, mailto:, or tel: (e.g. https://aivory.uk or mailto:hello@aivory.uk)");
    if (!url) return;
    const t = url.trim();
    if (!/^(https?:\/\/|mailto:|tel:)/i.test(t)) { setErr("URL must start with https://, mailto: or tel:"); return; }
    setErr("");
    const label = prompt("Link text (leave empty to use URL)", t) || t;
    const tag = `<a href="${escapeHtml(t)}" target="_blank" rel="noopener">${escapeHtml(label)}</a>`;
    ref.current?.focus();
    document.execCommand("insertHTML", false, tag);
    if (ref.current) setHtml(ref.current.innerHTML);
  }

  async function insertImage(files: FileList | null) {
    const f = files?.[0];
    if (!f) return;
    if (!f.type.startsWith("image/")) { setErr("Select an image file (png/jpg/webp/svg)"); return; }
    if (f.size > 800 * 1024) { setErr("Logo must be under 800 KB — please compress"); return; }
    setErr("");
    const dataUrl: string = await new Promise((res, rej) => {
      const r = new FileReader();
      r.onload = () => res(r.result as string);
      r.onerror = () => rej(new Error("read failed"));
      r.readAsDataURL(f);
    });
    const tag = `<img src="${dataUrl}" alt="${escapeHtml(f.name)}" style="max-width:220px;max-height:80px;height:auto;display:inline-block;vertical-align:middle" />`;
    ref.current?.focus();
    document.execCommand("insertHTML", false, tag);
    if (ref.current) setHtml(ref.current.innerHTML);
    if (imgRef.current) imgRef.current.value = "";
  }

  const empty = sigToText(html) === "";

  async function save() {
    if (empty || saving) return;
    setSaving(true);
    try {
      await onSave(html, sigToText(html));
    } finally {
      setSaving(false);
    }
  }

  const tool =
    "rounded px-1.5 py-1 text-sm text-zinc-700 hover:bg-black/[0.04] dark:text-zinc-300 dark:hover:bg-white/10";

  function Ico({ d, size = 14 }: { d: string; size?: number }) {
    return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.7} strokeLinecap="round" strokeLinejoin="round" aria-hidden><path d={d} /></svg>;
  }
  const P = {
    link: "M10 13a5 5 0 0 1 0-7l1-1a5 5 0 0 1 7 7l-1 1 M14 11a5 5 0 0 1 0 7l-1 1a5 5 0 0 1-7-7l1-1",
    image: "M3 5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5z M8.5 10a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3z M21 15l-5-5L5 21",
  };

  return (
    <div>
      <div className="flex flex-wrap items-center gap-1 rounded-lg border border-black/10 bg-black/[0.03] p-1 dark:border-zinc-700 dark:bg-white/5">
        <button onClick={() => fmt("bold")} className={`${tool} font-bold`} title="Bold">B</button>
        <button onClick={() => fmt("italic")} className={`${tool} italic`} title="Italic">I</button>
        <button onClick={() => fmt("underline")} className={`${tool} underline`} title="Underline">U</button>
        <span className="mx-1 h-4 w-px bg-black/10 dark:bg-white/10" />
        <button onClick={insertLink} className={`${tool} inline-flex items-center gap-1`} title="Insert link — email or website"><Ico d={P.link} size={12} /> Link</button>
        <button onClick={() => imgRef.current?.click()} className={`${tool} inline-flex items-center gap-1`} title="Insert logo image"><Ico d={P.image} size={12} /> Logo</button>
        <span className="ml-auto px-1 text-[11px] text-zinc-400 dark:text-zinc-500">Rich text</span>
        <input ref={imgRef} type="file" accept="image/png,image/jpeg,image/webp,image/svg+xml,image/gif" hidden onChange={(e) => insertImage(e.target.files)} />
      </div>
      {err && <div className="mt-2 rounded-lg bg-amber-50 px-3 py-1.5 text-xs text-amber-800 ring-1 ring-amber-200 dark:bg-amber-900/30 dark:text-amber-200">{err}</div>}
      <div
        key={ekey}
        ref={ref}
        contentEditable
        suppressContentEditableWarning
        onInput={(e) => setHtml(e.currentTarget.innerHTML)}
        data-placeholder="Best,"
        className="compose-rich mt-2 min-h-[90px] w-full overflow-y-auto rounded-lg border border-zinc-200 bg-white p-3 text-sm leading-6 focus:outline-none dark:border-zinc-600 dark:bg-zinc-900 dark:text-zinc-100"
      />
      <div className="mt-2 text-xs font-medium text-zinc-500 dark:text-zinc-400">Live preview — exactly how recipients see it</div>
      <div
        className="mt-1 rounded-lg border border-black/10 bg-white p-3 text-sm dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-100"
        dangerouslySetInnerHTML={{
          __html: DOMPurify.sanitize(html) || '<span style="color:#a1a1aa">Nothing yet — type above</span>',
        }}
      />
      <button
        onClick={save}
        disabled={saving || empty}
        className="mt-3 rounded-lg bg-[#ff6d00] px-4 py-1.5 text-xs font-semibold text-white hover:bg-[#e65e00] disabled:opacity-40"
      >
        {saving ? "Saving..." : saveLabel}
      </button>
      <p className="mt-2 text-[11px] leading-relaxed text-zinc-400 dark:text-zinc-500">
        Tip: name, role & company, phone — add logo via Logo and email/website via Link. Saved as rich text + plain twin.
      </p>
    </div>
  );
}

// Re-exported for tests/consumers that build seeded signatures.
export { escapeHtml as escapeSigHtml };
