import type { Config } from "tailwindcss";
const config: Config = {
  content: ["./app/**/*.{js,ts,jsx,tsx}", "./components/**/*.{js,ts,jsx,tsx}"],
  theme: { extend: { fontFamily: { sans: ["var(--font-manrope)", "Manrope", "sans-serif"] } } },
  plugins: [],
};
export default config;
