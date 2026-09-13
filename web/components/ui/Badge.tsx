"use client";
import type { ReactNode } from "react";

/**
 * avry-ui Badge — Skiff Chip (status pill) + MonoTag (mono label).
 */
export type ChipTone = "default" | "success" | "warning" | "info" | "danger";

const tones: Record<ChipTone, string> = {
  default: "bg-black/[0.06] text-zinc-600 ring-black/10",
  success: "bg-emerald-500/10 text-emerald-700 ring-emerald-600/20",
  warning: "bg-amber-500/10 text-amber-700 ring-amber-600/25",
  info: "bg-[#0B79AF]/10 text-[#0B79AF] ring-[#0B79AF]/25",
  danger: "bg-red-600/10 text-red-600 ring-red-600/20",
};

export function Chip({
  tone = "default",
  children,
  className = "",
}: {
  tone?: ChipTone;
  children: ReactNode;
  className?: string;
}) {
  return (
    <span
      className={`inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-semibold ring-1 ${tones[tone]} ${className}`}
    >
      {children}
    </span>
  );
}

export function MonoTag({ children, className = "" }: { children: ReactNode; className?: string }) {
  return (
    <span
      className={`inline-flex items-center rounded-md bg-black/[0.06] px-1.5 py-0.5 font-mono text-[10px] font-medium uppercase tracking-wide text-zinc-600 ${className}`}
    >
      {children}
    </span>
  );
}

export default Chip;
