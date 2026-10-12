"use client";

import { useEffect, useRef } from "react";
import {
  advanceSequence,
  canTriggerSingleKey,
  EMPTY_SEQ,
  isPaletteShortcut,
  parseSequence,
  type SeqState,
} from "./commandSearch";

type Options = {
  /** Single-key and "g then i" shortcuts (Settings → Shortcuts). ⌘K is always on. */
  enabled: boolean;
  /** Something modal is open (palette, shortcuts sheet): keep out of its way. */
  blocked: boolean;
  /** `"c"`, `"#"`, `"?"`, … */
  keys: Record<string, () => void>;
  /** `"g i"`, … */
  sequences: Record<string, () => void>;
  onPalette: () => void;
};

/**
 * One keydown listener for the whole app. The handlers are read through a ref,
 * so the listener is installed once and never sees a stale closure, and nothing
 * is fetched while keys are pressed (the settings are loaded by the caller).
 * `preventDefault` runs synchronously, so the key doesn't also type a character
 * into whatever gets focus.
 */
export function useShortcuts(options: Options) {
  const latest = useRef(options);
  latest.current = options;
  const seq = useRef<SeqState>(EMPTY_SEQ);

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      const o = latest.current;
      if (isPaletteShortcut(e)) {
        e.preventDefault();
        o.onPalette();
        return;
      }
      if (!o.enabled || o.blocked) return;
      if (!canTriggerSingleKey(e, e.target as HTMLElement | null)) return;
      if (e.key.length !== 1) return; // Escape, arrows, Tab: not ours

      // sequences first, so "g" is held back instead of firing something
      const sequences = Object.keys(o.sequences).map(parseSequence);
      if (sequences.length) {
        const r = advanceSequence(seq.current, e.key.toLowerCase(), Date.now(), sequences);
        seq.current = r.state;
        if (r.fired) {
          e.preventDefault();
          o.sequences[r.fired.join(" ")]?.();
          return;
        }
        if (r.state.keys.length) return; // waiting for the second key
      }
      const handler = o.keys[e.key] ?? o.keys[e.key.toLowerCase()];
      if (handler) {
        e.preventDefault();
        handler();
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
}

const OPEN_PALETTE_MESSAGE = "aivory-open-command-palette";

/**
 * Settings, Calendar and Domains are embedded in the inbox as iframes, and a
 * key press inside an iframe never reaches the parent's listener. Embedded
 * pages call this so ⌘K still opens the palette, which lives in the parent.
 */
export function useForwardPaletteShortcut() {
  useEffect(() => {
    if (typeof window === "undefined" || window.parent === window) return;
    function onKeyDown(e: KeyboardEvent) {
      if (!isPaletteShortcut(e)) return;
      e.preventDefault();
      window.parent.postMessage({ type: OPEN_PALETTE_MESSAGE }, window.location.origin);
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
}

/** Parent side of the above: calls `open` when an embedded page asks for the palette. */
export function useOpenPaletteFromEmbeds(open: () => void) {
  const latest = useRef(open);
  latest.current = open;
  useEffect(() => {
    function onMessage(e: MessageEvent) {
      if (e.origin !== window.location.origin) return;
      if (e.data && e.data.type === OPEN_PALETTE_MESSAGE) latest.current();
    }
    window.addEventListener("message", onMessage);
    return () => window.removeEventListener("message", onMessage);
  }, []);
}
