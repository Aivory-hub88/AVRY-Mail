"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import { filterCommands, type Command } from "./commandSearch";

const MAX_RESULTS = 60;

/** ⌘ on Apple platforms, Ctrl elsewhere. */
export function modKey(): string {
  if (typeof navigator === "undefined") return "Ctrl";
  return /Mac|iPhone|iPad/i.test(navigator.platform || navigator.userAgent) ? "⌘" : "Ctrl";
}

function Kbd({ children, onActiveRow = false }: { children: React.ReactNode; onActiveRow?: boolean }) {
  // The highlighted row inverts (dark in light mode, white in dark mode), so
  // its keys need the opposite treatment to stay readable.
  const tone = onActiveRow
    ? "border-white/30 bg-white/15 text-white dark:border-black/20 dark:bg-black/10 dark:text-zinc-700"
    : "border-black/10 bg-black/[0.04] text-zinc-600 dark:border-white/15 dark:bg-white/10 dark:text-zinc-300";
  return (
    <kbd className={`rounded border px-1.5 py-0.5 font-sans text-[11px] font-medium ${tone}`}>
      {children}
    </kbd>
  );
}

function ShortcutKeys({ shortcut, onActiveRow }: { shortcut?: string; onActiveRow?: boolean }) {
  if (!shortcut) return null;
  return (
    <span className="ml-3 flex shrink-0 items-center gap-1">
      {shortcut.split(" ").map((k, i) => <Kbd key={i} onActiveRow={onActiveRow}>{k}</Kbd>)}
    </span>
  );
}

export function CommandPalette({ open, commands, onClose, onSearch }: {
  open: boolean;
  commands: Command[];
  onClose: () => void;
  /** Free text that matches no command can be searched for. */
  onSearch?: (query: string) => void;
}) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const returnFocus = useRef<HTMLElement | null>(null);

  const results: Command[] = useMemo(() => {
    const found = filterCommands(query, commands).slice(0, MAX_RESULTS);
    const q = query.trim();
    if (q && onSearch) {
      found.push({ id: "search-free-text", title: `Search mail for “${q}”`, group: "Search", run: () => onSearch(q) });
    }
    return found;
  }, [query, commands, onSearch]);

  // open: remember where focus was, start clean; close: give focus back.
  useEffect(() => {
    if (open) {
      returnFocus.current = document.activeElement as HTMLElement | null;
      setQuery("");
      setActive(0);
      requestAnimationFrame(() => inputRef.current?.focus());
    } else if (returnFocus.current) {
      try { returnFocus.current.focus(); } catch {}
      returnFocus.current = null;
    }
  }, [open]);

  useEffect(() => { setActive(0); }, [query]);
  useEffect(() => {
    listRef.current?.querySelector<HTMLElement>(`[data-index="${active}"]`)?.scrollIntoView({ block: "nearest" });
  }, [active, results]);

  if (!open) return null;

  function run(cmd: Command | undefined) {
    if (!cmd) return;
    onClose();
    // after the palette has closed and focus has returned, so a command that
    // opens a composer or focuses search keeps the focus it asks for
    setTimeout(cmd.run, 0);
  }
  function onKeyDown(e: React.KeyboardEvent) {
    if (e.nativeEvent.isComposing) return;
    if (e.key === "Escape") { e.preventDefault(); onClose(); }
    else if (e.key === "ArrowDown" || (e.ctrlKey && e.key === "n")) { e.preventDefault(); setActive((i) => (results.length ? (i + 1) % results.length : 0)); }
    else if (e.key === "ArrowUp" || (e.ctrlKey && e.key === "p")) { e.preventDefault(); setActive((i) => (results.length ? (i - 1 + results.length) % results.length : 0)); }
    else if (e.key === "Enter") { e.preventDefault(); run(results[active]); }
  }

  let lastGroup = "";
  return (
    <div
      className="dialog-overlay fixed inset-0 z-[70] flex items-start justify-center bg-black/40 px-4 pt-[12vh]"
      data-state="open"
      onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Command palette"
        data-state="open"
        className="dialog-content w-full max-w-xl overflow-hidden rounded-2xl border border-[#e8e0c8] bg-[#fefcf6] shadow-2xl dark:border-zinc-700 dark:bg-zinc-900"
        onKeyDown={onKeyDown}
      >
        <div className="border-b border-[#e8e0c8] px-4 py-3 dark:border-zinc-700">
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            role="combobox"
            aria-expanded="true"
            aria-controls="command-palette-list"
            aria-activedescendant={results[active] ? `cmd-${results[active].id}` : undefined}
            aria-label="Type a command or search"
            placeholder="Type a command or search…"
            autoComplete="off"
            spellCheck={false}
            className="w-full bg-transparent text-[15px] text-zinc-900 outline-none placeholder:text-zinc-400 dark:text-white"
          />
        </div>
        <div ref={listRef} id="command-palette-list" role="listbox" className="max-h-[50vh] overflow-y-auto p-2">
          {results.length === 0 && (
            <div className="px-3 py-8 text-center text-sm text-zinc-500 dark:text-zinc-400">No matching command</div>
          )}
          {results.map((c, i) => {
            const header = c.group !== lastGroup && !query.trim() ? c.group : "";
            lastGroup = c.group;
            return (
              <div key={c.id}>
                {header && <div className="px-3 pb-1 pt-2 text-[11px] font-semibold uppercase tracking-wide text-zinc-400 dark:text-zinc-500">{header}</div>}
                <div
                  id={`cmd-${c.id}`}
                  role="option"
                  aria-selected={i === active}
                  data-index={i}
                  onMouseMove={() => i !== active && setActive(i)}
                  onClick={() => run(c)}
                  className={`flex cursor-pointer items-center justify-between rounded-lg px-3 py-2 text-sm ${
                    i === active ? "bg-zinc-900 text-white dark:bg-white dark:text-zinc-900" : "text-zinc-800 dark:text-zinc-200"
                  }`}
                >
                  <span className="truncate">{c.title}</span>
                  {query.trim() && <span className={`ml-3 shrink-0 text-xs ${i === active ? "opacity-70" : "text-zinc-400"}`}>{c.group}</span>}
                  {!query.trim() && <ShortcutKeys shortcut={c.shortcut} onActiveRow={i === active} />}
                </div>
              </div>
            );
          })}
        </div>
        <div className="flex items-center justify-between border-t border-[#e8e0c8] px-4 py-2 text-[11px] text-zinc-500 dark:border-zinc-700 dark:text-zinc-400">
          <span className="flex items-center gap-3">
            <span className="flex items-center gap-1"><Kbd>↑</Kbd><Kbd>↓</Kbd> move</span>
            <span className="flex items-center gap-1"><Kbd>↵</Kbd> run</span>
            <span className="flex items-center gap-1"><Kbd>esc</Kbd> close</span>
          </span>
          <span className="flex items-center gap-1">Press <Kbd>?</Kbd> for all shortcuts</span>
        </div>
      </div>
    </div>
  );
}

export type ShortcutGroup = { title: string; items: { keys: string; label: string }[] };

export function ShortcutsSheet({ open, groups, enabled, onClose }: {
  open: boolean;
  groups: ShortcutGroup[];
  /** Single-key shortcuts switched off in Settings. */
  enabled: boolean;
  onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => { if (open) ref.current?.focus(); }, [open]);
  if (!open) return null;
  return (
    <div
      className="dialog-overlay fixed inset-0 z-[70] flex items-start justify-center bg-black/40 px-4 pt-[8vh]"
      data-state="open"
      onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }}
    >
      <div
        ref={ref}
        tabIndex={-1}
        role="dialog"
        aria-modal="true"
        aria-label="Keyboard shortcuts"
        onKeyDown={(e) => { if (e.key === "Escape" || e.key === "?") { e.preventDefault(); onClose(); } }}
        data-state="open"
        className="dialog-content max-h-[80vh] w-full max-w-2xl overflow-y-auto rounded-2xl border border-[#e8e0c8] bg-[#fefcf6] p-5 shadow-2xl outline-none dark:border-zinc-700 dark:bg-zinc-900"
      >
        <div className="flex items-center justify-between">
          <h2 className="text-base font-semibold text-zinc-900 dark:text-white">Keyboard shortcuts</h2>
          <button onClick={onClose} className="rounded-lg px-2 py-1 text-xs text-zinc-500 hover:bg-black/[0.05] dark:text-zinc-400 dark:hover:bg-white/10" aria-label="Close">Esc</button>
        </div>
        {!enabled && (
          <p className="mt-2 rounded-lg bg-amber-50 px-3 py-2 text-xs text-amber-800 ring-1 ring-amber-200">
            Single-key shortcuts are switched off. Turn them on under Settings → Shortcuts. The command palette ({modKey()}+K) always works.
          </p>
        )}
        <div className="mt-4 grid gap-6 sm:grid-cols-2">
          {groups.map((g) => (
            <section key={g.title}>
              <h3 className="mb-2 text-[11px] font-semibold uppercase tracking-wide text-zinc-400 dark:text-zinc-500">{g.title}</h3>
              <ul className="space-y-1.5">
                {g.items.map((it) => (
                  <li key={it.label} className="flex items-center justify-between text-sm text-zinc-700 dark:text-zinc-200">
                    <span>{it.label}</span>
                    <span className="ml-3 flex shrink-0 items-center gap-1">
                      {it.keys.split(" ").map((k, i) => <Kbd key={i}>{k}</Kbd>)}
                    </span>
                  </li>
                ))}
              </ul>
            </section>
          ))}
        </div>
      </div>
    </div>
  );
}
