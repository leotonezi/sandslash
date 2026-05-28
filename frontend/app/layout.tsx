import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "seo-rs",
  description: "SEO audit tool powered by seo-rs",
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en">
      <body>
        <div className="container">{children}</div>
      </body>
    </html>
  );
}
