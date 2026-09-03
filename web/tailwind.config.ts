import type { Config } from "tailwindcss";
const config: Config = {
  content: ["./app/**/*.{js,ts,jsx,tsx}", "./components/**/*.{js,ts,jsx,tsx}"],
  theme: {
    extend: {
      colors: {
        background: "var(--background)",
        foreground: "var(--foreground)",
        card: "var(--card)",
        muted: "var(--muted)",
        border: "var(--border)",
        primary: { DEFAULT: "var(--primary)", foreground: "var(--primary-foreground)" },
      },
      fontFamily: { sans: ["var(--font-manrope)", "Manrope", "sans-serif"] },
      fontSize: {
        xs: ["12px", { lineHeight: "16px", letterSpacing: "0.01em" }],
        sm: ["14px", { lineHeight: "20px", letterSpacing: "-0.01em" }],
        base: ["15px", { lineHeight: "22px" }],
      },
      borderRadius: {
        lg: "12px",
        xl: "16px",
        "2xl": "20px",
      },
      transitionTimingFunction: {
        "ease-out": "cubic-bezier(0.23, 1, 0.32, 1)",
        "ease-in-out": "cubic-bezier(0.77, 0, 0.175, 1)",
      },
    },
  },
  plugins: [],
};
export default config;
