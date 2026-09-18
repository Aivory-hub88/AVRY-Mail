"use client";
import { useEffect, useRef, useState } from "react";
import DOMPurify from "dompurify";
import AIAssistantButton from "./AIAssistantButton";

// Removes background/background-color/background-image paint from inline
// style="" attributes, legacy bgcolor="" attributes, and <style> block rules
// — the three ways an HTML email actually paints a colored box. Leaves every
// other declaration (color, font, padding, borders...) untouched.
function stripPaintedBackgrounds(htmlStr: string): string {
  const stripFromCss = (css: string) => css.replace(/background(-color|-image)?\s*:[^;}"']+;?/gi, "");
  return htmlStr
    .replace(/style\s*=\s*"([^"]*)"/gi, (_m, css: string) => `style="${stripFromCss(css)}"`)
    .replace(/style\s*=\s*'([^']*)'/gi, (_m, css: string) => `style='${stripFromCss(css)}'`)
    .replace(/\sbgcolor\s*=\s*(["']).*?\1/gi, "")
    .replace(/<style([^>]*)>([\s\S]*?)<\/style>/gi, (_m, attrs: string, css: string) => `<style${attrs}>${stripFromCss(css)}</style>`);
}

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

  const dirty = DOMPurify.sanitize(html as string, {
    WHOLE_DOCUMENT: false,
    // "style" tags are safe to keep — DOMPurify doesn't let them execute
    // anything — and most real HTML email templates rely on a <style> block
    // for layout/responsive rules; stripping it left templates half-styled.
    FORBID_TAGS: ["script", "iframe", "object", "embed", "form"],
    FORBID_ATTR: ["srcset"],
    ADD_ATTR: ["target"],
  });

  // Dark mode: strip any background paint the sender applied (inline
  // style="background...", legacy bgcolor="...", and <style> block rules) so
  // the message can never land on its own colored panel — signature blocks
  // and marketing footers almost always paint an explicit white box, and
  // that box has to go, not just get inverted into an equally jarring black
  // one. Text color is left alone here; the invert filter below flips it
  // (and images are counter-inverted back to normal) so the now-background-
  // less content reads light-on-dark, melted straight into the app's own
  // card. Light mode renders the message completely untouched.
  const clean = dark ? stripPaintedBackgrounds(dirty) : dirty;

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
  const pageBg = dark ? "transparent" : "#ffffff";
  // This is the PRE-invert color for text that has no explicit color of its
  // own (plain <p> text inheriting from body — common in simple signature
  // emails with no inline styling). It gets flipped by .aivory-dm's filter
  // below, so a light final color needs a dark value here — #1a1a1a inverts
  // to #e5e5e5. Senders with their own explicit color (most template-built
  // HTML mail) invert from whatever they set, this is only the fallback.
  const pageColor = dark ? "#1a1a1a" : "#202124";
  // Background paint is already stripped above (dark mode only) — the
  // invert filter here only ever has to flip text/border colors, so a
  // formerly-black-on-white message reads white-on-nothing and melts into
  // whatever card the iframe sits on. Photos/logos are counter-inverted
  // back to their original colors.
  const wrappedHtml = dark ? `<div class="aivory-dm">${finalHtml}</div>` : finalHtml;
  const doc = `<!doctype html><html><head><meta charset="utf-8">
    <base target="_blank">
    <style>
      /* color-scheme stays "light" even in dark mode: setting it to "dark"
         makes the browser paint its own UA canvas fill behind a transparent
         background instead of leaving it truly see-through, so "transparent"
         silently becomes an opaque near-black rectangle — exactly the boxed
         look this is meant to avoid. We control every color ourselves via
         the invert filter, so the UA default is never wanted here. */
      html,body{margin:0;padding:0;background:${pageBg};color-scheme:light;max-width:100%;overflow-x:hidden;}
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
      ${dark ? `.aivory-dm{filter:invert(1) hue-rotate(180deg);}
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
