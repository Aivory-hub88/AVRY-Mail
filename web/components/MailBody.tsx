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
    FORBID_TAGS: ["script", "style", "iframe", "object", "embed", "form"],
    FORBID_ATTR: ["srcset"],
    ADD_ATTR: ["target"],
  });

  const doc = `<!doctype html><html><head><meta charset="utf-8"><meta name="color-scheme" content="light dark">
    <base target="_blank">
    <style>
      html,body{margin:0;padding:0;}
      body{font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,Helvetica,Arial,sans-serif;font-size:14px;line-height:1.5;color:#202124;word-wrap:break-word;overflow-wrap:break-word;}
      img{max-width:100%;height:auto;}
      table{max-width:100%;}
      a{color:#005a5e;}
      pre{white-space:pre-wrap;word-wrap:break-word;}
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
