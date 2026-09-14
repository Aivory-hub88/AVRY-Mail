"use client";
import { useState } from "react";

/**
 * AI Assistant entry button. Uses the brand SVG
 * (/aivory-mail-ASSISTANT-BUTTON.svg in web/public) with a ✦ fallback that
 * shows automatically until the file is added — no code change needed then.
 */
export const ASSISTANT_ICON_SRC = "/aivory-mail-ASSISTANT-BUTTON.svg";

function Mark({ size = 16 }: { size?: number }) {
  const [imgOk, setImgOk] = useState(true);
  if (imgOk) {
    return (
      <img
        src={ASSISTANT_ICON_SRC}
        alt=""
        aria-hidden
        width={size}
        height={size}
        style={{ width: size, height: size, objectFit: "contain" }}
        onError={() => setImgOk(false)}
      />
    );
  }
  return (
    <span aria-hidden style={{ fontSize: size }} className="leading-none">
      ✦
    </span>
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
        className={`flex h-7 w-7 items-center justify-center rounded-full hover:bg-black/[0.05] dark:hover:bg-white/10 ${className}`}
      >
        <Mark size={15} />
      </button>
    );
  }
  return (
    <button
      onClick={onClick}
      title="Open AI Assistant for this email"
      className={`inline-flex shrink-0 items-center gap-1.5 rounded-full border border-black/10 bg-white px-3 py-1 text-xs font-semibold text-zinc-800 shadow-sm hover:bg-black/[0.03] dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-100 dark:hover:bg-zinc-700 ${className}`}
    >
      <Mark size={14} />
      {label}
    </button>
  );
}
