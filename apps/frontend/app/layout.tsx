import type { Metadata } from 'next';
import type { ReactNode } from 'react';
import QueryProvider from './query-provider';
import './globals.css';
import '@git-diff-view/react/styles/diff-view.css';

export const metadata: Metadata = {
  title: "AgentDev UI",
  description: "Web interface for managing AI agents with git worktrees",
};

export default function RootLayout({
  children,
}: {
  children: ReactNode;
}) {
  return (
    <html lang="en" className="h-full" suppressHydrationWarning>
      <body className="flex h-full min-h-0 flex-col overflow-hidden bg-background font-sans text-foreground antialiased">
        <QueryProvider>{children}</QueryProvider>
      </body>
    </html>
  );
}
