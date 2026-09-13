"use client";
import { useEffect } from "react";

/** localStorage bridge that carries the inbox appearance.theme into the
 *  embedded iframe pages (/settings, /settings/mail, /calendar, /domains,
 *  /admin). Each document toggles its own `dark` class, so Tailwind `dark:`
 *  variants + the html.dark shell layer in globals.css apply per page. */
export const THEME_KEY = "aivory_mail_theme";

export function readTheme(): "light" | "dark" {
  try {
    return localStorage.getItem(THEME_KEY) === "light" ? "light" : "dark";
  } catch {
    return "dark";
  }
}

export function useThemeSync() {
  useEffect(() => {
    const apply = () => {
      document.documentElement.classList.toggle("dark", readTheme() === "dark");
    };
    apply();
    // Fires across documents (including iframes) on the same origin whenever
    // the inbox writes a theme change — no iframe reload, no API dependency.
    window.addEventListener("storage", apply);
    return () => window.removeEventListener("storage", apply);
  }, []);
}

export default useThemeSync;
