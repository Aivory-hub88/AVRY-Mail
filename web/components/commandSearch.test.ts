import { test } from "node:test";
import assert from "node:assert/strict";
import {
  advanceSequence,
  canTriggerSingleKey,
  commandScore,
  EMPTY_SEQ,
  filterCommands,
  isPaletteShortcut,
  isTypingTarget,
  parseSequence,
  tokenScore,
} from "./commandSearch.ts";

const cmd = (title: string, group = "Go to", keywords = "") => ({ title, group, keywords });

test("prefix beats word start beats substring beats loose subsequence", () => {
  const prefix = tokenScore("in", "Inbox")!;
  const wordStart = tokenScore("box", "Go to Inbox box")!;
  const substring = tokenScore("nbo", "Inbox")!;
  const loose = tokenScore("gti", "Go to Inbox")!;
  assert.ok(prefix > wordStart && wordStart > substring && substring > loose, [prefix, wordStart, substring, loose].join(" "));
  assert.equal(tokenScore("zzz", "Inbox"), null);
});

test("a single letter does not loosely match everything", () => {
  assert.equal(tokenScore("z", "Go to Inbox"), null);
  assert.ok(tokenScore("g", "Go to Inbox")! >= 60); // but a real prefix still does
});

test("every token must match; title outranks keywords and group", () => {
  const trash = cmd("Delete message", "Message", "trash remove bin");
  assert.ok(commandScore("delete", trash)! > commandScore("trash", trash)!);
  assert.equal(commandScore("delete banana", trash), null);
  assert.ok(commandScore("trash message", trash)! > 0);
});

test("filtering sorts best first and keeps original order on ties", () => {
  const list = [cmd("Go to Sent"), cmd("Compose new message", "Mail"), cmd("Go to Inbox"), cmd("Toggle dark mode", "View", "theme")];
  assert.deepEqual(filterCommands("inbox", list).map((c) => c.title), ["Go to Inbox"]);
  assert.deepEqual(filterCommands("theme", list).map((c) => c.title), ["Toggle dark mode"]);
  assert.deepEqual(filterCommands("go", list).map((c) => c.title), ["Go to Sent", "Go to Inbox"]);
  assert.equal(filterCommands("", list).length, 4);
  assert.equal(filterCommands("   ", list).length, 4);
});

test("sequences: g then i fires; wrong second key restarts; timeout resets", () => {
  const seqs = [parseSequence("g i"), parseSequence("g s")];
  let r = advanceSequence(EMPTY_SEQ, "g", 0, seqs);
  assert.equal(r.fired, null);
  r = advanceSequence(r.state, "i", 300, seqs);
  assert.deepEqual(r.fired, ["g", "i"]);
  assert.deepEqual(r.state, EMPTY_SEQ);

  // g g i: the second g restarts the sequence, then i completes it
  r = advanceSequence(EMPTY_SEQ, "g", 0, seqs);
  r = advanceSequence(r.state, "g", 100, seqs);
  r = advanceSequence(r.state, "i", 200, seqs);
  assert.deepEqual(r.fired, ["g", "i"]);

  // i alone is nothing; i after a long pause is nothing
  assert.equal(advanceSequence(EMPTY_SEQ, "i", 0, seqs).fired, null);
  r = advanceSequence(EMPTY_SEQ, "g", 0, seqs);
  assert.equal(advanceSequence(r.state, "i", 5000, seqs).fired, null);
});

test("modifier combos are not shortcuts (Ctrl+R reloads, Cmd+S saves)", () => {
  const base = { key: "r", ctrlKey: false, metaKey: false, altKey: false };
  assert.equal(canTriggerSingleKey(base, null), true);
  assert.equal(canTriggerSingleKey({ ...base, ctrlKey: true }, null), false);
  assert.equal(canTriggerSingleKey({ ...base, key: "s", metaKey: true }, null), false);
  assert.equal(canTriggerSingleKey({ ...base, altKey: true }, null), false);
  assert.equal(canTriggerSingleKey({ ...base, isComposing: true }, null), false);
  assert.equal(canTriggerSingleKey({ ...base, defaultPrevented: true }, null), false);
});

test("typing in a field blocks single keys, palette shortcut does not need that", () => {
  assert.equal(isTypingTarget({ tagName: "INPUT" }), true);
  assert.equal(isTypingTarget({ tagName: "textarea" }), true);
  assert.equal(isTypingTarget({ tagName: "DIV", isContentEditable: true }), true);
  assert.equal(isTypingTarget({ tagName: "DIV", getAttribute: (n) => (n === "role" ? "textbox" : null) }), true);
  assert.equal(isTypingTarget({ tagName: "BUTTON" }), false);
  assert.equal(isTypingTarget(null), false);
  assert.equal(canTriggerSingleKey({ key: "c", ctrlKey: false, metaKey: false, altKey: false }, { tagName: "INPUT" }), false);
  assert.equal(isPaletteShortcut({ key: "k", metaKey: true, ctrlKey: false, altKey: false }), true);
  assert.equal(isPaletteShortcut({ key: "K", metaKey: false, ctrlKey: true, altKey: false }), true);
  assert.equal(isPaletteShortcut({ key: "k", metaKey: false, ctrlKey: false, altKey: false }), false);
});
