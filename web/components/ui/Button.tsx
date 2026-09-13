"use client";
import { forwardRef, type ButtonHTMLAttributes } from "react";

/**
 * avry-ui Button — Skiff CTA hierarchy, Tailwind-native.
 * - primary: near-black CTA (Skiff `--cta-primary-default`)
 * - secondary: white + hairline border (Skiff `--cta-secondary-default`)
 * - tertiary: transparent ghost (Skiff `--cta-tertiary-default`)
 * - destructive: transparent red ghost (Skiff `--cta-destructive-*`)
 * Press feedback comes from the global `button:active { scale(.97) }` rule.
 */
export type ButtonVariant = "primary" | "secondary" | "tertiary" | "destructive";
export type ButtonSize = "sm" | "md" | "icon";

const base =
  "inline-flex select-none items-center justify-center gap-1.5 font-medium transition-colors duration-150 ease-out " +
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-zinc-900 " +
  "disabled:cursor-not-allowed disabled:opacity-40";

const variants: Record<ButtonVariant, string> = {
  primary:
    "bg-zinc-900 text-white shadow-sm hover:bg-zinc-800 " +
    "dark:bg-white dark:text-zinc-900 dark:hover:bg-zinc-200",
  secondary:
    "border border-black/10 bg-white text-zinc-700 shadow-sm hover:bg-black/[0.04] " +
    "dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-200 dark:hover:bg-zinc-700",
  tertiary: "text-zinc-600 hover:bg-black/[0.06] dark:text-zinc-300 dark:hover:bg-white/10",
  destructive: "text-red-600 hover:bg-red-600/10 dark:text-red-400 dark:hover:bg-red-400/10",
};

const sizes: Record<ButtonSize, string> = {
  sm: "h-7 rounded-lg px-2.5 text-xs",
  md: "h-9 rounded-lg px-4 text-sm font-semibold",
  icon: "h-7 w-7 rounded-full",
};

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  { variant = "secondary", size = "md", className = "", type = "button", ...rest },
  ref
) {
  return (
    <button ref={ref} type={type} className={`${base} ${variants[variant]} ${sizes[size]} ${className}`} {...rest} />
  );
});

export default Button;
