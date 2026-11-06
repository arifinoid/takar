import type { Metadata } from "next";

import "./globals.css";

export const metadata: Metadata = {
  title: "Takar",
  description: "Production planning and cost control for small F&B businesses",
};

export default function RootLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
