// Touch gesture maths, kept free of React and the DOM so it can be unit-tested.
// The components in SwipeRow.tsx / PullToRefresh.tsx / useEdgeSwipeBack.ts feed
// these functions pointer positions and act on what they return.

export type Axis = "pending" | "horizontal" | "vertical";

/** Finger travel before we decide whether this is a swipe or a scroll. */
export const AXIS_LOCK_PX = 10;
/** Distance that commits a swipe on its own. */
export const SWIPE_COMMIT_PX = 80;
/** A quick flick commits sooner. */
export const FLICK_MIN_PX = 40;
export const FLICK_MIN_VELOCITY = 0.5; // px per ms
/** Pull distance (after resistance) that triggers a refresh. */
export const PULL_THRESHOLD_PX = 64;
export const PULL_MAX_PX = 110;
/** A back swipe has to start this close to the left edge. */
export const EDGE_ZONE_PX = 28;
export const EDGE_COMMIT_PX = 80;

/**
 * Decide, once the finger has moved a little, whether the gesture is
 * horizontal (ours) or vertical (the page's scroll). Locked for the rest of
 * the gesture. A horizontal swipe must be clearly more sideways than down, so
 * a diagonal scroll never moves a row.
 */
export function lockAxis(dx: number, dy: number, current: Axis): Axis {
  if (current !== "pending") return current;
  const ax = Math.abs(dx);
  const ay = Math.abs(dy);
  if (Math.max(ax, ay) < AXIS_LOCK_PX) return "pending";
  return ax > ay * 1.5 ? "horizontal" : "vertical";
}

export type SwipeOutcome = "left" | "right" | null;

/** What a finished horizontal drag means. `vx` is px/ms (signed like `dx`). */
export function swipeOutcome(dx: number, vx: number, rowWidth: number): SwipeOutcome {
  const distance = Math.abs(dx);
  const far = distance >= Math.max(SWIPE_COMMIT_PX, rowWidth * 0.28);
  const flick = distance >= FLICK_MIN_PX && Math.abs(vx) >= FLICK_MIN_VELOCITY;
  if (!far && !flick) return null;
  return dx < 0 ? "left" : "right";
}

/**
 * How far the row follows the finger. Full speed where an action exists;
 * rubber-banded (a third of the travel, capped) where there is none, so the
 * user feels there is nothing on that side.
 */
export function followOffset(dx: number, hasAction: boolean, rowWidth: number): number {
  if (hasAction) return Math.max(-rowWidth, Math.min(rowWidth, dx));
  const damped = dx / 3;
  return Math.max(-24, Math.min(24, damped));
}

/** Pull-to-refresh: finger travel → indicator offset (with resistance) and readiness. */
export function pullState(dy: number): { offset: number; ready: boolean } {
  if (dy <= 0) return { offset: 0, ready: false };
  const offset = Math.min(PULL_MAX_PX, dy * 0.5);
  return { offset, ready: offset >= PULL_THRESHOLD_PX };
}

/** Swipe-right-from-the-left-edge to go back, as on iOS. */
export function isEdgeBack(startX: number, dx: number, dy: number): boolean {
  return startX <= EDGE_ZONE_PX && dx >= EDGE_COMMIT_PX && Math.abs(dy) < dx * 0.6;
}
