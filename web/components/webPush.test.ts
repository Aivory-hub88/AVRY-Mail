import { test } from "node:test";
import assert from "node:assert/strict";
import { urlBase64ToUint8Array } from "./webPush.ts";

test("VAPID public key decodes to an uncompressed P-256 point (65 bytes, 0x04)", () => {
  // RFC 8291 Appendix A application-server public key (base64url, unpadded).
  const key = urlBase64ToUint8Array("BP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8");
  assert.equal(key.length, 65);
  assert.equal(key[0], 0x04);
});

test("url-safe characters and missing padding are handled", () => {
  assert.deepEqual([...urlBase64ToUint8Array("-_8")], [0xfb, 0xff]);
  assert.deepEqual([...urlBase64ToUint8Array("AQ")], [0x01]);
});
