/* Aivory Mail service worker.
 *
 * 1. Shows new-mail notifications sent by the API through Web Push, including
 *    when no Aivory Mail tab is open. Payload shape: api/push.rs new_mail_payload().
 * 2. Shows /offline.html when a page can't be loaded because the device is
 *    offline.
 *
 * It deliberately caches nothing else. Mail is private and changes by the
 * second, and the app's own scripts are content-hashed by Next.js, so serving
 * anything from a cache risks showing someone stale or another account's data
 * for no gain. Only navigations are touched; API calls, scripts and images go
 * straight to the network.
 */

const SHELL_CACHE = "aivory-mail-shell-v1";
const OFFLINE_URL = "/offline.html";
const SHELL_FILES = [OFFLINE_URL, "/icon-192.png"];

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches.open(SHELL_CACHE).then((cache) => cache.addAll(SHELL_FILES)).then(() => self.skipWaiting()),
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    (async () => {
      const names = await caches.keys();
      await Promise.all(
        names.filter((n) => n.startsWith("aivory-mail-shell-") && n !== SHELL_CACHE).map((n) => caches.delete(n)),
      );
      await self.clients.claim();
    })(),
  );
});

self.addEventListener("fetch", (event) => {
  const request = event.request;
  if (request.mode !== "navigate" || request.method !== "GET") return;
  event.respondWith(
    fetch(request).catch(async () => (await caches.match(OFFLINE_URL)) || Response.error()),
  );
});

self.addEventListener("push", (event) => {
  let data = {};
  try {
    data = event.data ? event.data.json() : {};
  } catch (e) {
    data = {};
  }
  const title = data.title || "Aivory Mail";
  const options = {
    body: data.body || "You have new mail",
    icon: "/notification-icon.png",
    badge: "/notification-icon.png",
    // One notification per message; a repeat push for the same id replaces it.
    tag: data.message_id ? "aivory-mail-" + data.message_id : "aivory-mail",
    data: { url: data.url || "/", messageId: data.message_id || null },
  };
  // Browsers require every push to show a notification (userVisibleOnly),
  // so this always shows one, even when a tab is open. The page turns off
  // its own in-tab pop-ups while push is on, so nothing appears twice.
  event.waitUntil(self.registration.showNotification(title, options));
});

self.addEventListener("notificationclick", (event) => {
  event.notification.close();
  const { url, messageId } = event.notification.data || {};
  event.waitUntil(
    (async () => {
      const windows = await self.clients.matchAll({ type: "window", includeUncontrolled: true });
      const open = windows.find((c) => new URL(c.url).origin === self.location.origin);
      if (open) {
        await open.focus();
        if (messageId) open.postMessage({ type: "aivory:open-message", id: messageId });
        return;
      }
      await self.clients.openWindow(url || "/");
    })(),
  );
});
