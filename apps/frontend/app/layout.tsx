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
    <html lang="en" className="h-full">
      <body className="h-full bg-gray-50">
        {children}
      </body>
    </html>
  );
}