// Pure logic behind the command palette and the keyboard shortcuts. Nothing
// here touches React or the DOM, so it is unit-tested directly.

export type Command = {
  id: string;
  title: string;
  /** Section heading in the palette, e.g. "Go to", "Message". */
  group: string;
  /** Extra words that should find this command ("trash" finds "Delete"). */
  keywords?: string;
  /** How the shortcut is shown, e.g. "g i" or "e". Purely a label. */
  shortcut?: string;
  run: () => void;
};

// ───────────────────────── fuzzy matching ─────────────────────────

const words = (text: string) => text.split(/[^a-z0-9]+/).filter(Boolean);

/** How well one query token matches one piece of text; null = no match. */
export function tokenScore(token: string, text: string): number | null {
  if (!token) return 0;
  const t = text.toLowerCase();
  if (t.startsWith(token)) return 100;
  const ws = words(t);
  const wi = ws.findIndex((w) => w.startsWith(token));
  if (wi >= 0) return 80 - Math.min(wi, 9);
  const at = t.indexOf(token);
  if (at >= 0) return 60 - Math.min(at, 30) / 2;
  // Loose subsequence ("gti" finds "Go to Inbox"), only for real tokens so a
  // single letter doesn't match everything.
  if (token.length >= 3) {
    let from = 0;
    let span = 0;
    for (const ch of token) {
      const i = t.indexOf(ch, from);
      if (i < 0) return null;
      span += i - from;
      from = i + 1;
    }
    return Math.max(5, 30 - span);
  }
  return null;
}

/**
 * Every whitespace-separated token must match the title, keywords or group;
 * title matches count most. Null when something doesn't match.
 */
export function commandScore(query: string, cmd: Pick<Command, "title" | "keywords" | "group">): number | null {
  const tokens = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (tokens.length === 0) return 0;
  let total = 0;
  for (const token of tokens) {
    const candidates = [
      tokenScore(token, cmd.title),
      scale(tokenScore(token, cmd.keywords || ""), 0.7),
      scale(tokenScore(token, cmd.group), 0.4),
    ].filter((s): s is number => s !== null);
    if (candidates.length === 0) return null;
    total += Math.max(...candidates);
  }
  return total / tokens.length;
}

const scale = (s: number | null, k: number) => (s === null ? null : s * k);

/** Commands matching `query`, best first (ties keep their original order). */
export function filterCommands<T extends Pick<Command, "title" | "keywords" | "group">>(query: string, commands: T[]): T[] {
  if (!query.trim()) return commands;
  return commands
    .map((c, i) => ({ c, i, s: commandScore(query, c) }))
    .filter((x): x is { c: T; i: number; s: number } => x.s !== null)
    .sort((a, b) => b.s - a.s || a.i - b.i)
    .map((x) => x.c);
}

// ───────────────────────── key sequences ("g" then "i") ─────────────────────────

export type SeqState = { keys: string[]; at: number };
export const EMPTY_SEQ: SeqState = { keys: [], at: 0 };

/**
 * Feed one key into a sequence matcher. `sequences` is e.g. [["g","i"],["g","s"]].
 * Returns the new state and, when a sequence just completed, which one.
 */
export function advanceSequence(
  prev: SeqState,
  key: string,
  now: number,
  sequences: string[][],
  timeoutMs = 1000,
): { state: SeqState; fired: string[] | null } {
  const buffer = now - prev.at > timeoutMs ? [] : prev.keys;
  const isPrefix = (b: string[]) => sequences.some((s) => s.length > b.length && b.every((k, i) => s[i] === k));
  let next = [...buffer, key];
  const done = sequences.find((s) => s.length === next.length && s.every((k, i) => k === next[i]));
  if (done) return { state: EMPTY_SEQ, fired: done };
  if (isPrefix(next)) return { state: { keys: next, at: now }, fired: null };
  // not a continuation: maybe it starts a new sequence on its own
  next = [key];
  if (isPrefix(next)) return { state: { keys: next, at: now }, fired: null };
  return { state: EMPTY_SEQ, fired: null };
}

/** `"g i"` → `["g","i"]` */
export const parseSequence = (s: string) => s.trim().split(/\s+/);

// ───────────────────────── event filtering ─────────────────────────

type Targetish = { tagName?: string; isContentEditable?: boolean; getAttribute?: (n: string) => string | null } | null | undefined;

/** Is the user typing somewhere? Single-key shortcuts must stay out of the way. */
export function isTypingTarget(el: Targetish): boolean {
  if (!el) return false;
  const tag = (el.tagName || "").toUpperCase();
  if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return true;
  if (el.isContentEditable) return true;
  const role = el.getAttribute?.("role");
  return role === "textbox" || role === "combobox" || role === "searchbox";
}

type KeyEventLike = { key: string; ctrlKey: boolean; metaKey: boolean; altKey: boolean; isComposing?: boolean; defaultPrevented?: boolean };

/** ⌘K / Ctrl+K */
export function isPaletteShortcut(e: KeyEventLike): boolean {
  return e.key.toLowerCase() === "k" && (e.metaKey || e.ctrlKey) && !e.altKey;
}

/**
 * Whether a plain key press may trigger a single-key shortcut: no modifier
 * (Ctrl+R is reload, Cmd+S is save, not "reply" and "star"), not mid-IME
 * composition, not already handled, and not while typing.
 */
export function canTriggerSingleKey(e: KeyEventLike, target: Targetish): boolean {
  if (e.defaultPrevented || e.isComposing) return false;
  if (e.ctrlKey || e.metaKey || e.altKey) return false;
  return !isTypingTarget(target);
}
