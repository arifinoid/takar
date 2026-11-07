import type { Metadata } from "next";

import { I18nProvider } from "@/lib/i18n";

import "./globals.css";

export const metadata: Metadata = {
  title: "Takar",
  description: "Production planning and cost control for small F&B businesses",
};

export default function RootLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="id">
      <body>
        <I18nProvider>{children}</I18nProvider>
      </body>
    </html>
  );
}
