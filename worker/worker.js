// Aivory Mail — Cloudflare Email Routing Worker shim
// Deploy: wrangler deploy
// Env: AIVORY_MAIL_API_URL, AIVORY_MAIL_API_TOKEN
import { EmailMessage } from "cloudflare:email";

/**
 * Is `x-internal-token` the configured AIVORY_MAIL_API_TOKEN? Fails closed:
 * an unset secret, or one still holding an unexpanded "${...}" placeholder
 * (wrangler never expands those, and the placeholder is public in the repo),
 * authenticates nobody. Compared via SHA-256 digests so the check takes the
 * same time wherever a mismatch occurs.
 */
export async function internalTokenOk(request, env) {
  const expected = env.AIVORY_MAIL_API_TOKEN || "";
  const got = request.headers.get("x-internal-token") || "";
  if (!expected || !got || expected.startsWith("${")) return false;
  const enc = new TextEncoder();
  const [a, b] = await Promise.all([
    crypto.subtle.digest("SHA-256", enc.encode(got)),
    crypto.subtle.digest("SHA-256", enc.encode(expected)),
  ]);
  const x = new Uint8Array(a);
  const y = new Uint8Array(b);
  let diff = 0;
  for (let i = 0; i < x.length; i++) diff |= x[i] ^ y[i];
  return diff === 0;
}

/** Recipient lists as arrays, blanks dropped (the API sends null for an absent cc/bcc). */
function addrList(v) {
  return (Array.isArray(v) ? v : v ? [v] : []).map(x => String(x).trim()).filter(Boolean);
}

/**
 * What /send must deliver: envelope recipients plus the raw message. Cc is listed in
 * the headers; Bcc never is, so Bcc recipients stay hidden from the others.
 */
export function buildSendPlan({ from, to, cc, bcc, subject, text, html }) {
  const fromAddr = from.includes("<") ? from.match(/<([^>]+)>/)[1] : from.trim();
  const toList = addrList(to);
  const ccList = addrList(cc);
  const bccList = addrList(bcc);
  const raw =
    `Subject: ${subject || "(no subject)"}\r\nFrom: ${from}\r\nTo: ${toList.join(", ")}\r\n` +
    (ccList.length ? `Cc: ${ccList.join(", ")}\r\n` : "") +
    `Content-Type: ${html ? "text/html" : "text/plain"}; charset=utf-8\r\n\r\n${html || text || ""}`;
  return { fromAddr, toList, ccList, bccList, raw };
}

export default {
  async email(message, env, ctx) {
    const from = message.from || "(unknown sender)";
    // Cloudflare sends `to` as a string, but guard against arrays/objects so
    // a multi-recipient envelope can never 400 the webhook (silent loss).
    const to = Array.isArray(message.to) ? (message.to[0] || "(unknown recipient)") : (message.to || "(unknown recipient)");
    // Deduplicate forwarded loops
    if (message.headers.get("X-Aivory-Forwarded")) {
      console.log(`skip forwarded from=${from} to=${to}`);
      return;
    }
    let raw;
    try {
      raw = await new Response(message.raw).arrayBuffer();
    } catch (e) {
      console.error(`read raw failed from=${from} to=${to}: ${String(e && e.stack || e)}`);
      throw e; // fail loud: Cloudflare retries, then bounces — never silently drop
    }
    // Chunked base64: btoa(String.fromCharCode(...bytes)) blows the call
    // stack on any mail larger than ~100KB and the message is lost silently.
    const bytes = new Uint8Array(raw);
    let bin = "";
    const CHUNK = 0x8000;
    for (let i = 0; i < bytes.length; i += CHUNK) {
      bin += String.fromCharCode.apply(null, bytes.subarray(i, i + CHUNK));
    }
    const b64 = btoa(bin);
    const payload = { from, to, raw: b64 };
    const apiUrl = env.AIVORY_MAIL_API_URL || "https://mail.aivory.uk";
    console.log(`forward from=${from} to=${to} size=${bytes.length} api=${apiUrl}/v1/webhooks/cloudflare`);
    // Awaited (not waitUntil): a failed forward throws, so Cloudflare
    // retries and eventually bounces to the sender instead of losing mail.
    const resp = await fetch(`${apiUrl}/v1/webhooks/cloudflare`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "x-internal-token": env.AIVORY_MAIL_API_TOKEN || "",
        "x-aivory-forwarded": "1",
      },
      body: JSON.stringify(payload),
    });
    const body = await resp.text().catch(() => "");
    if (!resp.ok) {
      console.error(`forward FAILED status=${resp.status} from=${from} to=${to} body=${body.slice(0, 500)}`);
      throw new Error(`Aivory Mail API ${resp.status}: ${body.slice(0, 200)}`);
    }
    console.log(`forwarded to Aivory Mail status=${resp.status} from=${from} to=${to} id=${body.slice(0, 120)}`);
  },
  async fetch(request, env) {
    const url = new URL(request.url);
    // Outbound via Cloudflare Email (SEND_EMAIL binding) — bypass port 25
    if (url.pathname === "/send" && request.method === "POST") {
      // Without this the worker is an open relay: anyone could send mail as
      // @aivory.uk with the domain's own SPF/DKIM. The API already sends the
      // token (send_via_worker_http).
      if (!(await internalTokenOk(request, env))) {
        return new Response(JSON.stringify({ success: false, error: "unauthorized" }), { status: 401, headers: { "content-type": "application/json" } });
      }
      try {
        const body = await request.json();
        const { from, subject, text, html } = body;
        const { fromAddr, toList, ccList, bccList, raw } = buildSendPlan(body);
        if (env.SEND_EMAIL) {
          // One envelope per recipient: the binding takes a single address. The headers
          // list To/Cc for everyone; Bcc recipients get an envelope but no header.
          // A failure after something was already delivered must NOT reach the API as a
          // failure: it would re-send the whole message through the next provider and
          // the recipients already served would get a duplicate.
          const delivered = [];
          const failed = [];
          for (const rcpt of [...toList, ...ccList, ...bccList]) {
            try {
              await env.SEND_EMAIL.send(new EmailMessage(fromAddr, rcpt, raw));
              delivered.push(rcpt);
            } catch (e) {
              console.error(`send_email failed for one recipient: ${String(e).slice(0, 200)}`);
              failed.push(rcpt);
            }
          }
          if (delivered.length === 0) {
            return new Response(JSON.stringify({ success: false, error: "no recipient accepted", failed: failed.length }), { status: 502, headers: { "content-type": "application/json" } });
          }
          return new Response(JSON.stringify({ success: true, via: "send_email", delivered: delivered.length, failed: failed.length }), { headers: { "content-type": "application/json" } });
        } else {
          // Fallback to MailChannels if SEND_EMAIL not bound
          const fromEmail = fromAddr;
          const fromName = from.includes("<") ? from.split("<")[0].replace(/"/g, "").trim() : "";
          const personalization = { to: toList.map(email => ({ email })) };
          if (ccList.length) personalization.cc = ccList.map(email => ({ email }));
          if (bccList.length) personalization.bcc = bccList.map(email => ({ email }));
          const personalizations = [personalization];
          const content = [];
          if (html) content.push({ type: "text/html", value: html });
          if (text) content.push({ type: "text/plain", value: text });
          if (content.length === 0) content.push({ type: "text/plain", value: "" });
          const payload = {
            personalizations,
            from: { email: fromEmail, name: fromName || fromEmail },
            subject: subject || "(no subject)",
            content,
          };
          const resp = await fetch("https://api.mailchannels.net/tx/v1/send", {
            method: "POST",
            headers: { "content-type": "application/json" },
            body: JSON.stringify(payload),
          });
          const respBody = await resp.text();
          if (!resp.ok) {
            return new Response(JSON.stringify({ success: false, error: respBody }), { status: resp.status, headers: { "content-type": "application/json" } });
          }
          return new Response(JSON.stringify({ success: true, via: "mailchannels" }), { headers: { "content-type": "application/json" } });
        }
      } catch (e) {
        return new Response(JSON.stringify({ success: false, error: String(e) }), { status: 500, headers: { "content-type": "application/json" } });
      }
    }
    return new Response("Aivory Mail Worker — email() active. POST /send for outbound relay.", { headers: { "content-type": "text/plain" }});
  }
}
