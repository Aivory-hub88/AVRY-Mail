"use client";

import { useEffect, useRef, useState, type RefObject } from "react";
import { isEdgeBack, lockAxis, PULL_THRESHOLD_PX, pullState, type Axis } from "./gestures";

/**
 * Pull down at the top of a scrolling list to refresh it. Touch events, not
 * pointer events: cancelling the browser's own pan/pull-to-refresh needs a
 * non-passive `touchmove`. Only engages at scrollTop 0 and for a vertical pull,
 * so ordinary scrolling and horizontal row swipes are untouched.
 */
export function usePullToRefresh(ref: RefObject<HTMLElement | null>, onRefresh: () => void, enabled = true) {
  const [pull, setPull] = useState({ offset: 0, ready: false });
  const [refreshing, setRefreshing] = useState(false);
  const refresh = useRef(onRefresh);
  refresh.current = onRefresh;
  const readyRef = useRef(false);

  useEffect(() => {
    const el = ref.current;
    if (!el || !enabled) return;
    let x0 = 0;
    let y0 = 0;
    let axis: Axis = "pending";
    let tracking = false;
    let pulling = false;

    const settle = () => { tracking = false; pulling = false; axis = "pending"; setPull({ offset: 0, ready: false }); };

    function onStart(e: TouchEvent) {
      if (e.touches.length !== 1 || el!.scrollTop > 0) { tracking = false; return; }
      x0 = e.touches[0].clientX;
      y0 = e.touches[0].clientY;
      axis = "pending";
      tracking = true;
      pulling = false;
    }
    function onMove(e: TouchEvent) {
      if (!tracking) return;
      const dx = e.touches[0].clientX - x0;
      const dy = e.touches[0].clientY - y0;
      axis = lockAxis(dx, dy, axis);
      if (axis === "pending") return;
      if (axis !== "vertical" || dy <= 0 || el!.scrollTop > 0) { if (pulling) settle(); else tracking = false; return; }
      pulling = true;
      if (e.cancelable) e.preventDefault(); // keep the browser from scrolling or reloading the page
      const next = pullState(dy);
      readyRef.current = next.ready;
      setPull(next);
    }
    function onEnd() {
      if (pulling && readyRef.current) {
        setRefreshing(true);
        refresh.current();
        setTimeout(() => setRefreshing(false), 900);
      }
      readyRef.current = false;
      settle();
    }
    el.addEventListener("touchstart", onStart, { passive: true });
    el.addEventListener("touchmove", onMove, { passive: false });
    el.addEventListener("touchend", onEnd, { passive: true });
    el.addEventListener("touchcancel", onEnd, { passive: true });
    return () => {
      el.removeEventListener("touchstart", onStart);
      el.removeEventListener("touchmove", onMove);
      el.removeEventListener("touchend", onEnd);
      el.removeEventListener("touchcancel", onEnd);
    };
  }, [ref, enabled]);

  return { offset: refreshing ? PULL_THRESHOLD_PX : pull.offset, ready: pull.ready, refreshing };
}

/**
 * Swipe right from the left edge to go back (close the open message), as on
 * iOS. Only on a phone-sized screen and only while something is open.
 */
export function useEdgeSwipeBack(ref: RefObject<HTMLElement | null>, onBack: () => void, enabled: boolean) {
  const back = useRef(onBack);
  back.current = onBack;

  useEffect(() => {
    const el = ref.current;
    if (!el || !enabled) return;
    let start: { x: number; y: number } | null = null;

    function onStart(e: TouchEvent) {
      const small = typeof window.matchMedia === "function" && window.matchMedia("(max-width: 767px)").matches;
      start = small && e.touches.length === 1 ? { x: e.touches[0].clientX, y: e.touches[0].clientY } : null;
    }
    function onEnd(e: TouchEvent) {
      if (!start) return;
      const t = e.changedTouches[0];
      const hit = isEdgeBack(start.x, t.clientX - start.x, t.clientY - start.y);
      start = null;
      if (hit) back.current();
    }
    el.addEventListener("touchstart", onStart, { passive: true });
    el.addEventListener("touchend", onEnd, { passive: true });
    el.addEventListener("touchcancel", () => { start = null; }, { passive: true });
    return () => {
      el.removeEventListener("touchstart", onStart);
      el.removeEventListener("touchend", onEnd);
    };
  }, [ref, enabled]);
}
