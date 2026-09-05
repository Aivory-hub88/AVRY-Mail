import "./globals.css";
import type { Metadata } from "next";

export const metadata: Metadata = {
  title: "Aivory Mail",
  icons: {
    icon: "/Favicon_Aivory-Mail.svg",
    shortcut: "/Favicon_Aivory-Mail.svg",
    apple: "/Favicon_Aivory-Mail.svg",
  },
};
import { Manrope } from "next/font/google";
const manrope = Manrope({ subsets: ["latin"], weight: ["300","400","500","600","700"], variable: "--font-manrope", display: "swap" });
export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" className={manrope.variable}>
      <body className="min-h-screen bg-zinc-50 text-zinc-900 antialiased font-[Manrope]">{children}</body>
    </html>
  );
}