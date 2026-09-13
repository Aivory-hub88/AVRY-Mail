"use client";
import { forwardRef, type InputHTMLAttributes } from "react";

/**
 * avry-ui Input — Skiff field style: borderless, bg black/6, hover black/8.
 * (Skiff `--bg-field-default` / `--bg-field-hover`.)
 */
export interface InputProps extends InputHTMLAttributes<HTMLInputElement> {
  label?: string;
  hint?: string;
  error?: string;
}

export const inputCls =
  "h-9 w-full rounded-lg border-0 bg-black/[0.06] px-3 text-sm text-zinc-900 " +
  "placeholder:text-zinc-400 hover:bg-black/[0.08] focus:bg-black/[0.08] focus:outline-none " +
  "dark:bg-white/10 dark:text-zinc-100 dark:placeholder:text-zinc-500 dark:hover:bg-white/[0.14] dark:focus:bg-white/[0.14]";

export const Input = forwardRef<HTMLInputElement, InputProps>(function Input(
  { label, hint, error, id, className = "", ...rest },
  ref
) {
  return (
    <label htmlFor={id} className="block min-w-0">
      {label && <span className="mb-1 block text-xs font-medium text-zinc-500">{label}</span>}
      <input ref={ref} id={id} className={`${inputCls} ${className}`} {...rest} />
      {error ? (
        <span className="mt-1 block text-xs text-red-600">{error}</span>
      ) : hint ? (
        <span className="mt-1 block text-xs text-zinc-400">{hint}</span>
      ) : null}
    </label>
  );
});

export default Input;
