import { test } from "node:test";
import assert from "node:assert/strict";
import { followOffset, isEdgeBack, lockAxis, pullState, swipeOutcome } from "./gestures.ts";

test("axis stays pending until the finger has moved, then locks and never changes", () => {
  assert.equal(lockAxis(3, 2, "pending"), "pending");
  assert.equal(lockAxis(30, 4, "pending"), "horizontal");
  assert.equal(lockAxis(4, 30, "pending"), "vertical");
  // a diagonal scroll is a scroll, not a swipe
  assert.equal(lockAxis(20, 18, "pending"), "vertical");
  assert.equal(lockAxis(24, 12, "pending"), "horizontal");
  // once locked, later movement can't flip it
  assert.equal(lockAxis(2, 90, "horizontal"), "horizontal");
  assert.equal(lockAxis(90, 2, "vertical"), "vertical");
});

test("a swipe commits by distance or by a quick flick, in the right direction", () => {
  assert.equal(swipeOutcome(-120, 0.1, 360), "left");
  assert.equal(swipeOutcome(130, 0.1, 360), "right");
  assert.equal(swipeOutcome(-50, 0.1, 360), null); // short and slow: springs back
  assert.equal(swipeOutcome(-50, -0.9, 360), "left"); // short but a flick
  assert.equal(swipeOutcome(30, 0.9, 360), null); // a flick still needs some travel
  // wide rows need proportionally more than 80px
  assert.equal(swipeOutcome(-90, 0.1, 800), null);
  assert.equal(swipeOutcome(-230, 0.1, 800), "left");
});

test("the row follows the finger where there is an action and resists where there isn't", () => {
  assert.equal(followOffset(-100, true, 360), -100);
  assert.equal(followOffset(-900, true, 360), -360);
  assert.equal(followOffset(-100, false, 360), -24); // capped rubber band
  assert.equal(followOffset(30, false, 360), 10);
});

test("pull to refresh: resistance, cap and threshold", () => {
  assert.deepEqual(pullState(-10), { offset: 0, ready: false });
  assert.deepEqual(pullState(60), { offset: 30, ready: false });
  assert.equal(pullState(140).ready, true);
  assert.equal(pullState(1000).offset, 110);
});

test("back swipe must start at the edge and be mostly horizontal", () => {
  assert.equal(isEdgeBack(10, 100, 5), true);
  assert.equal(isEdgeBack(120, 100, 5), false); // not from the edge
  assert.equal(isEdgeBack(10, 40, 0), false); // too short
  assert.equal(isEdgeBack(10, 100, 90), false); // that's a scroll
});
