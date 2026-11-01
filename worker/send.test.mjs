// Run: node worker/send.test.mjs   (Node 18+, no dependencies)
// POST /send must deliver to Cc and Bcc recipients too (the API falls back to this worker
// when Cloudflare's own send is refused, and the worker used to read only `to`).
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import assert from "node:assert/strict";

const src = readFileSync(fileURLToPath(new URL("./worker.js", import.meta.url)), "utf8").replace(
  'import { EmailMessage } from "cloudflare:email";',
  "class EmailMessage { constructor(from, to, raw) { this.from = from; this.to = to; this.raw = raw; } }",
);
const mod = await import("data:text/javascript;base64," + Buffer.from(src).toString("base64"));
const worker = mod.default;

const post = (env, body) =>
  worker.fetch(
    new Request("https://worker.test/send", {
      method: "POST",
      headers: { "content-type": "application/json", "x-internal-token": "tok" },
      body: JSON.stringify(body),
    }),
    env,
  );

// SEND_EMAIL binding: one envelope per recipient; Cc in the headers, Bcc nowhere in them.
{
  const sent = [];
  const env = { AIVORY_MAIL_API_TOKEN: "tok", SEND_EMAIL: { send: async (m) => { sent.push(m); } } };
  const res = await post(env, {
    from: "Lex <lex@aivory.uk>", to: ["a@x.com"], cc: ["c@x.com"], bcc: ["b@x.com"], subject: "s", text: "t",
  });
  assert.equal(res.status, 200);
  assert.deepEqual(sent.map((m) => m.to), ["a@x.com", "c@x.com", "b@x.com"]);
  assert.match(sent[0].raw, /\r\nTo: a@x\.com\r\nCc: c@x\.com\r\n/);
  assert.doesNotMatch(sent[0].raw, /b@x\.com/i);
  assert.equal(sent[0].from, "lex@aivory.uk");
}

// A request with no cc/bcc (null from the API) behaves as before.
{
  const sent = [];
  const env = { AIVORY_MAIL_API_TOKEN: "tok", SEND_EMAIL: { send: async (m) => { sent.push(m); } } };
  await post(env, { from: "lex@aivory.uk", to: ["a@x.com"], cc: null, bcc: null, subject: "s", text: "t" });
  assert.equal(sent.length, 1);
  assert.doesNotMatch(sent[0].raw, /Cc:/);
}

// MailChannels fallback carries cc/bcc inside the personalization.
{
  const calls = [];
  globalThis.fetch = async (url, init) => { calls.push(JSON.parse(init.body)); return new Response("", { status: 202 }); };
  await post({ AIVORY_MAIL_API_TOKEN: "tok" }, {
    from: "lex@aivory.uk", to: ["a@x.com"], cc: ["c@x.com"], bcc: ["b@x.com"], subject: "s", text: "t",
  });
  assert.deepEqual(calls[0].personalizations, [
    { to: [{ email: "a@x.com" }], cc: [{ email: "c@x.com" }], bcc: [{ email: "b@x.com" }] },
  ]);
}

// One recipient failing after another was delivered must still answer success (the API would
// otherwise re-send the whole message through the next provider = duplicates).
{
  const sent = [];
  const env = {
    AIVORY_MAIL_API_TOKEN: "tok",
    SEND_EMAIL: { send: async (m) => { if (m.to === "c@x.com") throw new Error("rejected"); sent.push(m.to); } },
  };
  const res = await post(env, { from: "lex@aivory.uk", to: ["a@x.com"], cc: ["c@x.com"], subject: "s", text: "t" });
  assert.equal(res.status, 200);
  assert.deepEqual(await res.json(), { success: true, via: "send_email", delivered: 1, failed: 1 });
  assert.deepEqual(sent, ["a@x.com"]);
}

// Nobody accepted: a real failure, so the API may try the next provider safely.
{
  const env = { AIVORY_MAIL_API_TOKEN: "tok", SEND_EMAIL: { send: async () => { throw new Error("down"); } } };
  const res = await post(env, { from: "lex@aivory.uk", to: ["a@x.com"], subject: "s", text: "t" });
  assert.equal(res.status, 502);
}

console.log("send ok");
