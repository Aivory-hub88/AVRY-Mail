/* Aivory Mail service worker: shows new-mail notifications sent by the API
 * through Web Push, including when no Aivory Mail tab is open.
 * Payload shape: api/push.rs new_mail_payload(). */

self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (event) => event.waitUntil(self.clients.claim()));

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
