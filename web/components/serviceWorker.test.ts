// Runs public/sw.js unchanged under a mocked service-worker global scope, so
// its install / activate / fetch behaviour is covered without a browser.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const SOURCE = readFileSync(fileURLToPath(new URL("../public/sw.js", import.meta.url)), "utf8");

type Handler = (event: any) => void;

function boot(network: (req: any) => Promise<Response>) {
  const handlers: Record<string, Handler> = {};
  const stores = new Map<string, Map<string, Response>>();
  const calls = { skipWaiting: 0, claim: 0 };

  const cacheApi = {
    async open(name: string) {
      if (!stores.has(name)) stores.set(name, new Map());
      const store = stores.get(name)!;
      return {
        async addAll(urls: string[]) { for (const u of urls) store.set(u, new Response(`cached:${u}`)); },
        async put(u: string, r: Response) { store.set(u, r); },
      };
    },
    async keys() { return [...stores.keys()]; },
    async delete(name: string) { return stores.delete(name); },
    async match(url: string) {
      for (const store of stores.values()) {
        const hit = store.get(url);
        if (hit) return hit.clone();
      }
      return undefined;
    },
  };
  const self = {
    addEventListener: (type: string, fn: Handler) => { handlers[type] = fn; },
    skipWaiting: async () => { calls.skipWaiting++; },
    clients: { claim: async () => { calls.claim++; }, matchAll: async () => [], openWindow: async () => {} },
    registration: { showNotification: async () => {} },
    location: { origin: "https://mail.example" },
  };
  new Function("self", "caches", "fetch", SOURCE)(self, cacheApi, network);
  return { handlers, stores, calls };
}

async function run(handler: Handler, extra: Record<string, unknown> = {}) {
  let pending: Promise<unknown> | undefined;
  let responded: Promise<Response> | undefined;
  const event = {
    ...extra,
    waitUntil: (p: Promise<unknown>) => { pending = p; },
    respondWith: (p: Promise<Response>) => { responded = p; },
  };
  handler(event);
  if (pending) await pending;
  return { responded: responded ? await responded : undefined, wasHandled: responded !== undefined };
}

const nav = (method = "GET") => ({ request: { mode: "navigate", method, url: "https://mail.example/" } });

test("install precaches only the offline page and its icon, then takes over", async () => {
  const { handlers, stores, calls } = boot(async () => new Response("net"));
  await run(handlers.install);
  assert.deepEqual([...stores.get("aivory-mail-shell-v1")!.keys()].sort(), ["/icon-192.png", "/offline.html"]);
  assert.equal(calls.skipWaiting, 1);
});

test("activate removes old shell caches and leaves everything else alone", async () => {
  const { handlers, stores, calls } = boot(async () => new Response("net"));
  stores.set("aivory-mail-shell-v0", new Map());
  stores.set("unrelated-cache", new Map());
  await run(handlers.activate);
  assert.equal(stores.has("aivory-mail-shell-v0"), false);
  assert.equal(stores.has("unrelated-cache"), true);
  assert.equal(calls.claim, 1);
});

test("a navigation goes to the network, and falls back to the offline page only when that fails", async () => {
  let online = true;
  const { handlers } = boot(async () => { if (!online) throw new TypeError("offline"); return new Response("fresh page"); });
  await run(handlers.install);

  const onlineResult = await run(handlers.fetch, nav());
  assert.equal(await onlineResult.responded!.text(), "fresh page");

  online = false;
  const offlineResult = await run(handlers.fetch, nav());
  assert.equal(await offlineResult.responded!.text(), "cached:/offline.html");
});

test("nothing else is intercepted: API calls, scripts, images, form posts", async () => {
  const { handlers } = boot(async () => new Response("net"));
  await run(handlers.install);
  for (const request of [
    { mode: "cors", method: "GET", url: "https://mail.example/v1/messages" },
    { mode: "no-cors", method: "GET", url: "https://mail.example/_next/static/app.js" },
    { mode: "same-origin", method: "GET", url: "https://mail.example/icon-192.png" },
    { mode: "navigate", method: "POST", url: "https://mail.example/login" },
  ]) {
    const r = await run(handlers.fetch, { request });
    assert.equal(r.wasHandled, false, `${request.method} ${request.mode} ${request.url}`);
  }
});

test("offline with no cached shell yields an error response, not a hang", async () => {
  const { handlers } = boot(async () => { throw new TypeError("offline"); });
  const r = await run(handlers.fetch, nav());
  assert.equal(r.responded!.type, "error");
});

test("push handling is still registered", () => {
  const { handlers } = boot(async () => new Response("net"));
  assert.equal(typeof handlers.push, "function");
  assert.equal(typeof handlers.notificationclick, "function");
});
