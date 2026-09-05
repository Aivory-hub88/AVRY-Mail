// Aivory Mail — Cloudflare Email Routing Worker shim
// Deploy: wrangler deploy
// Env: AIVORY_MAIL_API_URL, AIVORY_MAIL_API_TOKEN
import { EmailMessage } from "cloudflare:email";

export default {
  async email(message, env, ctx) {
    // Deduplicate forwarded loops
    if (message.headers.get("X-Aivory-Forwarded")) {
      console.log("skip forwarded");
      return;
    }
    const raw = await new Response(message.raw).arrayBuffer();
    const b64 = btoa(String.fromCharCode(...new Uint8Array(raw)));
    const payload = { from: message.from, to: message.to, raw: b64 };
    const apiUrl = env.AIVORY_MAIL_API_URL || "https://mail.aivory.id";
    ctx.waitUntil(
      fetch(`${apiUrl}/v1/webhooks/cloudflare`, {
        method: "POST",
        headers: {
          "content-type": "application/json",
          "x-internal-token": env.AIVORY_MAIL_API_TOKEN || "",
          "x-aivory-forwarded": "1",
        },
        body: JSON.stringify(payload),
      }).then(r => console.log("forwarded to Aivory Mail", r.status)).catch(e => console.error(e))
    );
  },
  async fetch(request, env) {
    const url = new URL(request.url);
    // Outbound via Cloudflare Email (SEND_EMAIL binding) — bypass port 25
    if (url.pathname === "/send" && request.method === "POST") {
      try {
        const { from, to, subject, text, html } = await request.json();
        const fromAddr = from.includes("<") ? from.match(/<([^>]+)>/)[1] : from.trim();
        const toList = Array.isArray(to) ? to : [to];
        // Build EmailMessage via Cloudflare Email
        const msg = new EmailMessage(fromAddr, toList.join(", "), `Subject: ${subject || "(no subject)"}\r\nFrom: ${from}\r\nTo: ${toList.join(", ")}\r\nContent-Type: ${html ? "text/html" : "text/plain"}; charset=utf-8\r\n\r\n${html || text || ""}`);
        if (env.SEND_EMAIL) {
          await env.SEND_EMAIL.send(msg);
          return new Response(JSON.stringify({ success: true, via: "send_email" }), { headers: { "content-type": "application/json" } });
        } else {
          // Fallback to MailChannels if SEND_EMAIL not bound
          const fromEmail = fromAddr;
          const fromName = from.includes("<") ? from.split("<")[0].replace(/"/g, "").trim() : "";
          const personalizations = toList.map(email => ({ to: [{ email }] }));
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
          const body = await resp.text();
          if (!resp.ok) {
            return new Response(JSON.stringify({ success: false, error: body }), { status: resp.status, headers: { "content-type": "application/json" } });
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
