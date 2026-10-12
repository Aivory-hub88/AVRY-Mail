"use client";

import { useRef, useState } from "react";
import { followOffset, lockAxis, swipeOutcome, type Axis } from "./gestures";

type Props = {
  children: React.ReactNode;
  /** Finger moved left. Reveals the right-hand label. */
  onSwipeLeft?: () => void;
  /** Finger moved right. Reveals the left-hand label. */
  onSwipeRight?: () => void;
  leftLabel?: string;
  rightLabel?: string;
};

const SLIDE_MS = 160;

/**
 * Swipe a list row on a touch screen: left to archive, right to toggle read.
 * Mouse input is ignored, so desktop behaviour is unchanged, and the actions
 * stay reachable from buttons and the command palette (a gesture is never the
 * only way). `touch-action: pan-y` leaves vertical scrolling to the browser;
 * the axis is locked after a few pixels so a diagonal scroll never moves a row.
 */
export function SwipeRow({ children, onSwipeLeft, onSwipeRight, leftLabel = "Archive", rightLabel = "Read" }: Props) {
  const [offset, setOffset] = useState(0);
  const [dragging, setDragging] = useState(false);
  const st = useRef({ id: -1, x0: 0, y0: 0, t0: 0, width: 0, axis: "pending" as Axis });
  const swiped = useRef(false);

  function reset() {
    st.current.id = -1;
    st.current.axis = "pending";
    setDragging(false);
    setOffset(0);
  }

  function onPointerDown(e: React.PointerEvent<HTMLDivElement>) {
    if (e.pointerType === "mouse" || !e.isPrimary) return;
    st.current = { id: e.pointerId, x0: e.clientX, y0: e.clientY, t0: e.timeStamp, width: e.currentTarget.offsetWidth, axis: "pending" };
  }

  function onPointerMove(e: React.PointerEvent<HTMLDivElement>) {
    const s = st.current;
    if (s.id !== e.pointerId) return;
    const dx = e.clientX - s.x0;
    const dy = e.clientY - s.y0;
    s.axis = lockAxis(dx, dy, s.axis);
    if (s.axis !== "horizontal") return;
    if (!dragging) {
      setDragging(true);
      try { e.currentTarget.setPointerCapture(e.pointerId); } catch {}
    }
    setOffset(followOffset(dx, dx < 0 ? !!onSwipeLeft : !!onSwipeRight, s.width));
  }

  function onPointerUp(e: React.PointerEvent<HTMLDivElement>) {
    const s = st.current;
    if (s.id !== e.pointerId) return;
    const wasHorizontal = s.axis === "horizontal";
    const dx = e.clientX - s.x0;
    const dt = Math.max(1, e.timeStamp - s.t0);
    const outcome = wasHorizontal ? swipeOutcome(dx, dx / dt, s.width) : null;
    if (wasHorizontal) {
      // the click that follows a drag must not open the row
      swiped.current = true;
      setTimeout(() => { swiped.current = false; }, 60);
    }
    const action = outcome === "left" ? onSwipeLeft : outcome === "right" ? onSwipeRight : undefined;
    if (!action) { reset(); return; }
    st.current.id = -1;
    setDragging(false);
    setOffset(outcome === "left" ? -s.width : s.width); // slide out…
    setTimeout(() => { action(); setOffset(0); }, SLIDE_MS); // …then act and settle
  }

  const revealing = offset !== 0;
  const toRight = offset > 0;
  return (
    <div
      className="relative overflow-hidden"
      style={{ touchAction: "pan-y" }}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={reset}
      onClickCapture={(e) => { if (swiped.current) { e.preventDefault(); e.stopPropagation(); } }}
    >
      {revealing && (
        <div
          aria-hidden
          className={`absolute inset-0 flex items-center px-5 text-xs font-semibold text-white ${
            toRight ? "justify-start bg-sky-600" : "justify-end bg-emerald-600"
          }`}
        >
          {toRight ? rightLabel : leftLabel}
        </div>
      )}
      <div
        className="relative bg-[#fefcf6] dark:bg-zinc-800"
        style={{
          transform: `translateX(${offset}px)`,
          transition: dragging ? "none" : `transform ${SLIDE_MS}ms ease-out`,
        }}
      >
        {children}
      </div>
    </div>
  );
}
