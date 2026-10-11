import { test } from "node:test";
import assert from "node:assert/strict";
import { tokenExpiry, tokenExpired } from "./session.ts";

function jwt(payload: object): string {
  const enc = (o: object) => Buffer.from(JSON.stringify(o)).toString("base64url");
  return `${enc({ alg: "HS256", typ: "JWT" })}.${enc(payload)}.signature`;
}

test("reads exp from the payload", () => {
  assert.equal(tokenExpiry(jwt({ exp: 1791727803, sub: "a@b.c" })), 1791727803);
});

test("expired once exp is in the past, valid before", () => {
  const exp = 1_800_000_000;
  const token = jwt({ exp });
  assert.equal(tokenExpired(token, exp * 1000 - 1), false);
  assert.equal(tokenExpired(token, exp * 1000), true);
  assert.equal(tokenExpired(token, exp * 1000 + 60_000), true);
});

test("unreadable or exp-less tokens are not treated as expired (the server decides)", () => {
  assert.equal(tokenExpired(null), false);
  assert.equal(tokenExpired(""), false);
  assert.equal(tokenExpired("not-a-jwt"), false);
  assert.equal(tokenExpired("a.%%%.c"), false);
  assert.equal(tokenExpired(jwt({ sub: "x" })), false);
  assert.equal(tokenExpiry(jwt({ exp: "soon" })), null);
});
