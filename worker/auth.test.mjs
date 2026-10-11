// Run: node worker/auth.test.mjs   (Node 18+, no dependencies)
// Checks that POST /send on the worker refuses callers without the internal
// token. `cloudflare:email` only exists inside Workers, so it's stubbed.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import assert from "node:assert/strict";

const src = readFileSync(fileURLToPath(new URL("./worker.js", import.meta.url)), "utf8").replace(
  'import { EmailMessage } from "cloudflare:email";',
  "class EmailMessage { constructor(from, to, raw) { this.from = from; this.to = to; this.raw = raw; } }",
);
const worker = (await import("data:text/javascript;base64," + Buffer.from(src).toString("base64"))).default;

const sent = [];
const env = (token) => ({ AIVORY_MAIL_API_TOKEN: token, SEND_EMAIL: { send: async (m) => { sent.push(m); } } });
const req = (token) =>
  new Request("https://worker.test/send", {
    method: "POST",
    headers: { "content-type": "application/json", ...(token === undefined ? {} : { "x-internal-token": token }) },
    body: JSON.stringify({ from: "a@aivory.uk", to: "b@example.com", subject: "s", text: "t" }),
  });
const status = async (token, secret) => (await worker.fetch(req(token), env(secret))).status;

assert.equal(await status(undefined, "s3cret"), 401, "no token");
assert.equal(await status("wrong", "s3cret"), 401, "wrong token");
assert.equal(await status("s3cret2", "s3cret"), 401, "near miss");
assert.equal(await status("S3CRET", "s3cret"), 401, "case differs");
assert.equal(await status("", ""), 401, "both empty");
assert.equal(await status("anything", ""), 401, "unset secret fails closed");
assert.equal(await status("${INTERNAL_TOKEN}", "${INTERNAL_TOKEN}"), 401, "unexpanded placeholder never authenticates");
assert.equal(sent.length, 0, "no rejected call sent mail");
assert.equal(await status("s3cret", "s3cret"), 200, "valid token sends");
assert.equal(sent.length, 1);
assert.equal((await worker.fetch(new Request("https://worker.test/"), env("s3cret"))).status, 200, "banner stays public");
console.log("worker auth: all checks passed");
