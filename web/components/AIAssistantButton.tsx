"use client";
import { useId } from "react";

// Brand gradient from the Aivory Mail assistant button artwork
// (frontend-nextjs/public/aivory-mail-ASSISTANT-BUTTON.svg).
export const AI_GRADIENT_FROM = "#d80000";
export const AI_GRADIENT_TO = "#f7c34d";
const SPARKLE = "M12 2.5l2.35 6.15L20.5 11l-6.15 2.35L12 19.5l-2.35-6.15L3.5 11l6.15-2.35L12 2.5z";

/** Four-point AI sparkle filled with the brand red→gold gradient. */
export function AISparkle({ size = 16, mono = false, className = "" }: { size?: number; mono?: boolean; className?: string }) {
  const id = useId().replace(/:/g, "");
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" className={className} aria-hidden>
      {!mono && (
        <defs>
          <linearGradient id={`aig${id}`} x1="3" y1="3" x2="21" y2="21" gradientUnits="userSpaceOnUse">
            <stop offset="0" stopColor={AI_GRADIENT_FROM} />
            <stop offset="1" stopColor={AI_GRADIENT_TO} />
          </linearGradient>
        </defs>
      )}
      <path d={SPARKLE} fill={mono ? "currentColor" : `url(#aig${id})`} />
    </svg>
  );
}

export default function AIAssistantButton({
  onClick,
  label = "AI Assistant",
  variant = "pill",
  className = "",
}: {
  onClick: () => void;
  label?: string;
  variant?: "pill" | "icon";
  className?: string;
}) {
  if (variant === "icon") {
    return (
      <button
        onClick={onClick}
        title="AI Assistant"
        aria-label="AI Assistant"
        className={`flex h-8 w-8 items-center justify-center rounded-full hover:bg-black/[0.05] dark:hover:bg-white/10 ${className}`}
      >
        <AISparkle size={16} />
      </button>
    );
  }
  // Pill mirrors the artwork: dark fill inside a red→gold gradient ring.
  return (
    <button
      onClick={onClick}
      title="Open AI Assistant for this email"
      className={`inline-flex shrink-0 rounded-full p-px shadow-sm transition-[filter] hover:brightness-110 ${className}`}
      style={{ backgroundImage: `linear-gradient(90deg, ${AI_GRADIENT_FROM}, ${AI_GRADIENT_TO})` }}
    >
      <span className="inline-flex items-center gap-1.5 rounded-full bg-zinc-950 px-3 py-1 text-xs font-semibold text-white">
        <AISparkle size={14} />
        {label}
      </span>
    </button>
  );
}
