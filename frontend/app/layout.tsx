import type { Metadata } from "next";
import { Montserrat } from "next/font/google";
import Image from "next/image";
import clawIcon from "../assets/images/claw.png";
import ThemeToggle from "./components/ThemeToggle";
import "./globals.css";

const montserrat = Montserrat({ subsets: ["latin"], variable: "--font-montserrat" });

export const metadata: Metadata = {
  title: "Sandslash",
  description: "Blazing-fast SEO audits, built with Rust.",
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en" className={montserrat.variable}>
      <head>
        <script
          dangerouslySetInnerHTML={{
            __html: `(function(){var t=localStorage.getItem('theme')||((window.matchMedia('(prefers-color-scheme:dark)').matches)?'dark':'light');document.documentElement.setAttribute('data-theme',t);})()`,
          }}
        />
      </head>
      <body>
        <nav className="navbar">
          <div className="navbar-inner">
            <Image src={clawIcon} alt="Sandslash" width={28} height={28} priority className="navbar-logo" />
            <span className="navbar-brand">Sandslash</span>
            <ThemeToggle />
          </div>
        </nav>
        <div className="container">{children}</div>
      </body>
    </html>
  );
}
