import type { Metadata } from "next";
import { IBM_Plex_Mono, IBM_Plex_Sans, Space_Grotesk } from "next/font/google";
import { ThemeScript } from "gui-mellow/ThemeToggle";
import { GlowTracker } from "@/components/GlowTracker";
import "gui-mellow/styles.scss";
import "./app.scss";

const display = Space_Grotesk({ variable: "--font-display", subsets: ["latin"], weight: ["500", "600"] });
const body = IBM_Plex_Sans({ variable: "--font-body", subsets: ["latin"], weight: ["400", "500", "600"] });
const mono = IBM_Plex_Mono({ variable: "--font-mono", subsets: ["latin"], weight: ["400"] });

export const metadata: Metadata = {
  title: { default: "Methods", template: "%s · Methods" },
  description: "Project management for people and their bots.",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" className={`${display.variable} ${body.variable} ${mono.variable}`} suppressHydrationWarning>
      <head>
        <ThemeScript />
      </head>
      <body>
        <GlowTracker />
        {children}
      </body>
    </html>
  );
}
