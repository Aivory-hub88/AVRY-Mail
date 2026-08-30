// Aivory Mail — Cloudflare Email Routing Worker shim
// Deploy: wrangler deploy
// Env: AIVORY_MAIL_API_URL, AIVORY_MAIL_API_TOKEN

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
    return new Response("Aivory Mail Worker — email() active. POST /email for test.", { headers: { "content-type": "text/plain" }});
  }
}
