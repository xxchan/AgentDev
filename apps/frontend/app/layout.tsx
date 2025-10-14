import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "AgentDev UI",
  description: "Web interface for managing AI agents with git worktrees",
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en" className="h-full" suppressHydrationWarning>
      <body className="min-h-screen bg-background font-sans text-foreground antialiased">
        {children}
      </body>
    </html>
  );
}
