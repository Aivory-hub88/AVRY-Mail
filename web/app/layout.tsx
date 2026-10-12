import "./globals.css";
import type { Metadata, Viewport } from "next";

export const metadata: Metadata = {
  title: "Aivory Mail",
  manifest: "/manifest.webmanifest",
  icons: {
    icon: "/Favicon_Aivory-Mail.svg",
    shortcut: "/Favicon_Aivory-Mail.svg",
    apple: "/apple-touch-icon.png",
  },
  // iOS: open from the home screen without browser chrome.
  appleWebApp: { capable: true, title: "Aivory Mail", statusBarStyle: "default" },
  formatDetection: { telephone: false },
};

export const viewport: Viewport = {
  width: "device-width",
  initialScale: 1,
  // lets fixed controls respect the notch / home indicator via env(safe-area-inset-*)
  viewportFit: "cover",
  themeColor: [
    { media: "(prefers-color-scheme: light)", color: "#fefcf6" },
    { media: "(prefers-color-scheme: dark)", color: "#18181b" },
  ],
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