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
  const ref = useRef<HTMLDivElement>(null);

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

  return (
    <div>
      <div className="flex items-center gap-1 rounded-lg border border-black/10 bg-black/[0.03] p-1 dark:border-zinc-700 dark:bg-white/5">
        <button onClick={() => fmt("bold")} className={`${tool} font-bold`} title="Bold">B</button>
        <button onClick={() => fmt("italic")} className={`${tool} italic`} title="Italic">I</button>
        <button onClick={() => fmt("underline")} className={`${tool} underline`} title="Underline">U</button>
        <span className="ml-auto px-1 text-[11px] text-zinc-400 dark:text-zinc-500">Rich text</span>
      </div>
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
        Tip: name, role & company, phone. Saved as rich text + plain-text twin for text-only mail.
      </p>
    </div>
  );
}

// Re-exported for tests/consumers that build seeded signatures.
export { escapeHtml as escapeSigHtml };
