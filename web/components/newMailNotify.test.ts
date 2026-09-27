// Run: npm test  (Node's built-in runner; Node strips the TS types itself)
import { test } from "node:test";
import assert from "node:assert/strict";
import { buildPopUps, pickNewMessages, MAX_INDIVIDUAL, type InboxItem } from "./newMailNotify.ts";

const msg = (id: string, extra: Partial<InboxItem> = {}): InboxItem => ({ id, from: `${id}@x.id`, subject: `Subject ${id}`, ...extra });

test("nothing is new before a baseline exists", () => {
  assert.deepEqual(pickNewMessages([msg("a")], null), []);
});

test("every message above the last seen one is new, newest first", () => {
  const list = [msg("c"), msg("b"), msg("a")];
  assert.deepEqual(pickNewMessages(list, "a").map((m) => m.id), ["c", "b"]);
  assert.deepEqual(pickNewMessages(list, "c"), []);
});

test("last seen scrolled off the page: the whole page is new", () => {
  assert.equal(pickNewMessages([msg("z"), msg("y")], "gone").length, 2);
});

test("one pop-up per message, oldest first so the newest ends on top, each opens its message", () => {
  const popUps = buildPopUps([msg("c"), msg("b")]);
  assert.deepEqual(popUps.map((p) => p.messageId), ["b", "c"]);
  assert.equal(popUps[1].title, "c@x.id");
  assert.equal(popUps[1].body, "Subject c");
  assert.notEqual(popUps[0].tag, popUps[1].tag); // distinct tags: they don't replace each other
});

test("beyond the cap, the rest collapse into one summary", () => {
  const fresh = ["f", "e", "d", "c", "b"].map((id) => msg(id));
  const popUps = buildPopUps(fresh);
  assert.equal(popUps.length, MAX_INDIVIDUAL + 1);
  assert.equal(popUps[0].messageId, undefined);
  assert.match(popUps[0].body, /2 more new messages/);
  assert.equal(popUps.at(-1)!.messageId, "f");
});

test("sender name beats address; empty subject falls back to snippet, then a default", () => {
  assert.equal(buildPopUps([msg("a", { from_name: "Alvin" })])[0].title, "Alvin");
  assert.equal(buildPopUps([msg("a", { subject: "", snippet: "hi there" })])[0].body, "hi there");
  assert.equal(buildPopUps([msg("a", { subject: null, snippet: null, from: null })])[0].title, "New message");
});
