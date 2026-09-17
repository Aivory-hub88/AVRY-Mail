"use client";
import { useEffect, useRef, useState } from "react";
import DOMPurify from "dompurify";
import AIAssistantButton from "./AIAssistantButton";

// Renders a received email the way Gmail/Zoho/Outlook do: the message's own
// HTML in an isolated sandboxed iframe (so its styles/tables can't bleed into
// — or be clobbered by — the app's own Tailwind CSS), sanitized so a hostile
// sender can't run script or reach outside the frame. Falls back to the
// plain-text part when there is no HTML body. Previously the page rendered
// body_text AND raw body_html stacked on top of each other via
// dangerouslySetInnerHTML with no sanitization and no style isolation.
export default function MailBody({ html, text, dark, apiBase, token, onAssistant }: { html?: string | null; text?: string | null; dark?: boolean; apiBase?: string; token?: string | null; onAssistant?: () => void }) {
  const iframeRef = useRef<HTMLIFrameElement>(null);
  const [height, setHeight] = useState(80);
  // Gmail parity: remote (tracking) images are hidden until the user opts
  // in per message. cid:/data: images (inline logos, signatures) always
  // render — they came inside the message itself, no privacy leak.
  const [showRemote, setShowRemote] = useState(false);

  const hasHtml = !!html && html.trim().length > 0;

  useEffect(() => {
    if (!hasHtml) return;
    const frame = iframeRef.current;
    if (!frame) return;
    let observer: ResizeObserver | null = null;
    const resize = () => {
      try {
        const doc = frame.contentDocument;
        if (doc?.body) setHeight(Math.min(Math.max(doc.body.scrollHeight + 24, 80), 2000));
      } catch {}
    };
    const onLoad = () => {
      resize();
      // A single measurement on 'load' can happen before the surrounding
      // flex layout has settled the iframe's own width (briefly 0 in some
      // conditions), which makes every word wrap onto its own line and
      // inflates scrollHeight to thousands of pixels — a wall of blank
      // space capped at 2000px with the real content squeezed at the
      // bottom. A ResizeObserver keeps re-measuring as layout (and content,
      // e.g. late-loading images/fonts) actually changes, self-correcting
      // instead of freezing on that first bad reading.
      try {
        const doc = frame.contentDocument;
        if (doc?.body && "ResizeObserver" in window) {
          observer = new ResizeObserver(() => resize());
          observer.observe(doc.body);
        }
      } catch {}
    };
    frame.addEventListener("load", onLoad);
    return () => {
      frame.removeEventListener("load", onLoad);
      observer?.disconnect();
    };
  }, [hasHtml, html]);

  if (!hasHtml) {
    return (
      <div className="whitespace-pre-wrap break-words text-[14px] leading-6 text-zinc-800 dark:text-zinc-200">
        {text || <span className="italic text-zinc-400">No content</span>}
      </div>
    );
  }

  const clean = DOMPurify.sanitize(html as string, {
    WHOLE_DOCUMENT: false,
    // "style" tags are safe to keep — DOMPurify doesn't let them execute
    // anything — and most real HTML email templates rely on a <style> block
    // for layout/responsive rules; stripping it left templates half-styled.
    FORBID_TAGS: ["script", "iframe", "object", "embed", "form"],
    FORBID_ATTR: ["srcset"],
    ADD_ATTR: ["target"],
  });

  // Dark-mode reader: the whole message is color-inverted
  // (invert + hue-rotate) so sender ink always ends up light-on-dark —
  // Outlook's explicit color:rgb(0,0,0) becomes white, white page becomes
  // near-black. Photos/logos are counter-inverted back to original, hues
  // are approximately preserved by the hue rotation. Light mode renders
  // the message untouched. Trade-off, stated plainly: emails that were
  // DESIGNED dark (light text on dark sections) flip to dark-on-light —
  // the standard cost of automatic dark mode, same technique several
  // mobile clients use. Readability of normal mail wins.
  // Inline API attachments (/v1/messages/.../attachments/...) are pulled
  // aside before the remote-image gate: they came inside the message, so
  // they always render (Gmail parity), and <img> tags can't send an
  // Authorization header, so the session token is appended as ?token=
  // (the backend accepts it as a bearer equivalent, ownership enforced).
  const base = (apiBase || "").replace(/\/$/, "");
  const attUrls: string[] = [];
  const withPlaceholders = clean.replace(
    /<img([^>]*?)\ssrc\s*=\s*(["'])([^"']*)\2/gi,
    (m: string, attrs: string, q: string, url: string) =>
      url.includes("/v1/messages/") && url.includes("/attachments/")
        ? `<img${attrs} src=${q}__AIVORY_ATT_${attUrls.push(url) - 1}__${q}`
        : m
  );
  const shown = showRemote
    ? withPlaceholders
    : withPlaceholders.replace(/<img([^>]*)\ssrc\s*=\s*(["'])https?:[^"']*\2/gi,
        (_m: string, attrs: string) => `<span class="aivory-img-off">[image hidden]</span><img${attrs} src="" alt="remote image hidden" style="display:none">`);
  const finalHtml = shown.replace(/__AIVORY_ATT_(\d+)__/g, (_m: string, i: string) => {
    const raw = attUrls[Number(i)] || "";
    const abs = /^https?:\/\//i.test(raw) ? raw : `${base}${raw.startsWith("/") ? "" : "/"}${raw}`;
    if (!abs) return "";
    return token ? `${abs}${abs.includes("?") ? "&" : "?"}token=${encodeURIComponent(token)}` : abs;
  });
  // Banner only for true remote images — inline API attachments were
  // pulled into placeholders above, so they never trigger the gate.
  const hasRemoteImg = hasHtml && /<img[^>]*\ssrc\s*=\s*["']https?:/i.test(withPlaceholders);
  const pageBg = dark ? "#27272a" : "#ffffff";
  const pageColor = dark ? "#e4e4e7" : "#202124";
  const wrappedHtml = dark ? `<div class="aivory-dm">${finalHtml}</div>` : finalHtml;
  const doc = `<!doctype html><html><head><meta charset="utf-8">
    <base target="_blank">
    <style>
      html,body{margin:0;padding:0;background:${pageBg};color-scheme:${dark ? "dark" : "light"};max-width:100%;overflow-x:hidden;}
      body{font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,Helvetica,Arial,sans-serif;font-size:14px;line-height:1.5;color:${pageColor};word-wrap:break-word;overflow-wrap:anywhere;}
      img{max-width:100%;height:auto;}
      table{max-width:100%;}
      /* Some senders emit long unbroken tokens (booking links, tracking
         IDs) as plain text or inside <a> without their own wrap rule —
         force a break rather than letting them push the layout wide. */
      a,td,p,div,span{overflow-wrap:anywhere;word-break:break-word;}
      a{color:#005a5e;text-decoration:underline;}a:hover{color:#00454a;}
      pre{white-space:pre-wrap;word-wrap:break-word;overflow-wrap:anywhere;}
      .aivory-img-off{display:inline-block;border:1px dashed #a8a29e;background:#f5f5f4;color:#78716c;font-size:12px;padding:6px 10px;border-radius:8px;margin:4px 0;}
      ${dark ? `.aivory-dm{background:#D8D8D5;filter:invert(1) hue-rotate(180deg);}
      /* #D8D8D5 inverts to exactly #27272a (zinc-800) — the app's own dark
         panel — so the message melts into the surrounding card instead of
         flashing a pure-black rectangle. (Grays have no hue, so the
         rotation is a no-op on the base.) */
      .aivory-dm img,.aivory-dm video,.aivory-dm svg,.aivory-dm canvas{filter:invert(1) hue-rotate(180deg);}` : ``}
    </style>
    </head><body>${wrappedHtml}</body></html>`;

  return (
    <div>
      {hasRemoteImg && (
        <div className="mb-2 flex items-center gap-2 rounded-lg border border-[#e8e0c8] bg-[#f8f6ef] px-3 py-1.5 text-xs text-zinc-600 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-300">
          <span>{showRemote ? "Remote images are shown." : "Remote images are hidden for privacy."}</span>
          <button
            onClick={() => setShowRemote(v => !v)}
            className="rounded-lg border border-[#005a5e] px-2 py-0.5 font-medium text-[#005a5e] hover:bg-[#005a5e] hover:text-white dark:border-teal-400/40 dark:text-teal-300 dark:hover:bg-teal-400/10 dark:hover:text-teal-200"
          >
            {showRemote ? "Hide images" : "Show images"}
          </button>
          {onAssistant && (
            <span className="ml-auto">
              <AIAssistantButton onClick={onAssistant} />
            </span>
          )}
        </div>
      )}
      <iframe
        ref={iframeRef}
        title="Email content"
        srcDoc={doc}
        sandbox="allow-same-origin allow-popups allow-popups-to-escape-sandbox"
        style={{ width: "100%", height, border: "none", display: "block" }}
      />
    </div>
  );
}
