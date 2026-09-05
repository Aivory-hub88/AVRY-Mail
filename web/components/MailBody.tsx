"use client";
import { useEffect, useRef, useState } from "react";
import DOMPurify from "dompurify";

// Renders a received email the way Gmail/Zoho/Outlook do: the message's own
// HTML in an isolated sandboxed iframe (so its styles/tables can't bleed into
// — or be clobbered by — the app's own Tailwind CSS), sanitized so a hostile
// sender can't run script or reach outside the frame. Falls back to the
// plain-text part when there is no HTML body. Previously the page rendered
// body_text AND raw body_html stacked on top of each other via
// dangerouslySetInnerHTML with no sanitization and no style isolation.
export default function MailBody({ html, text }: { html?: string | null; text?: string | null }) {
  const iframeRef = useRef<HTMLIFrameElement>(null);
  const [height, setHeight] = useState(80);

  const hasHtml = !!html && html.trim().length > 0;

  useEffect(() => {
    if (!hasHtml) return;
    const frame = iframeRef.current;
    if (!frame) return;
    const resize = () => {
      try {
        const doc = frame.contentDocument;
        if (doc?.body) setHeight(Math.min(Math.max(doc.body.scrollHeight + 24, 80), 2000));
      } catch {}
    };
    frame.addEventListener("load", resize);
    return () => frame.removeEventListener("load", resize);
  }, [hasHtml, html]);

  if (!hasHtml) {
    return (
      <div className="whitespace-pre-wrap break-words text-[14px] leading-6 text-zinc-800">
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

  // Deliberately NOT declaring color-scheme: email HTML is authored assuming
  // a fixed white page (like every real webmail client renders it), and
  // "light dark" here made the browser paint the iframe's default background
  // black under a dark OS theme while text stayed a dark, light-background
  // color — dark-on-black, unreadable. html/body background is pinned to
  // white below so no sender/browser default can flip it.
  const doc = `<!doctype html><html><head><meta charset="utf-8">
    <base target="_blank">
    <style>
      html,body{margin:0;padding:0;background:#ffffff;color-scheme:light;max-width:100%;overflow-x:hidden;}
      body{font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,Helvetica,Arial,sans-serif;font-size:14px;line-height:1.5;color:#202124;word-wrap:break-word;overflow-wrap:anywhere;}
      img{max-width:100%;height:auto;}
      table{max-width:100%;}
      /* Some senders emit long unbroken tokens (booking links, tracking
         IDs) as plain text or inside <a> without their own wrap rule —
         force a break rather than letting them push the layout wide. */
      a,td,p,div,span{overflow-wrap:anywhere;word-break:break-word;}
      a{color:#005a5e;}
      pre{white-space:pre-wrap;word-wrap:break-word;overflow-wrap:anywhere;}
    </style>
    </head><body>${clean}</body></html>`;

  return (
    <iframe
      ref={iframeRef}
      title="Email content"
      srcDoc={doc}
      sandbox="allow-same-origin allow-popups allow-popups-to-escape-sandbox"
      style={{ width: "100%", height, border: "none", display: "block" }}
    />
  );
}
