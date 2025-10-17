import { ReactNode } from 'react';
import Link from 'next/link';

import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';

interface MainLayoutProps {
  sidebar: ReactNode;
  main: ReactNode;
  bottom?: ReactNode;
}

export default function MainLayout({ sidebar, main, bottom }: MainLayoutProps) {
  return (
    <div className="flex h-full min-h-0 flex-col bg-background">
      {/* Header */}
      <header className="border-b border-border bg-card px-4 py-3 shadow-sm">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex flex-wrap items-center gap-2">
            <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-primary text-primary-foreground">
              <span className="text-sm font-bold">A</span>
            </div>
            <h1 className="text-xl font-semibold text-foreground">AgentDev UI</h1>
            <nav aria-label="Primary navigation" className="flex items-center gap-2">
              <Button asChild size="sm" variant="ghost">
                <Link href="/worktrees">Worktrees</Link>
              </Button>
              <Button asChild size="sm" variant="ghost">
                <Link href="/sessions">Sessions</Link>
              </Button>
            </nav>
          </div>
          <div className="flex items-center gap-4">
            <span className="hidden text-sm text-muted-foreground sm:inline">
              Multi-Agent Development Environment
            </span>
          </div>
        </div>
      </header>

      {/* Main content area */}
      <div className="flex-1 flex min-h-0 overflow-hidden">
        {/* Left sidebar */}
        <aside className="w-80 min-h-0 border-r border-border bg-card">
          <ScrollArea className="h-full min-h-0" viewportClassName="pr-3">
            {sidebar}
          </ScrollArea>
        </aside>

        {/* Right content area (main + bottom) */}
        <div className="flex-1 flex flex-col">
          {/* Main diff viewer */}
          <main className="flex flex-1 min-h-0 flex-col overflow-hidden bg-card">
            {main}
          </main>

          {/* Bottom processes panel */}
          {bottom ? (
            <div className="h-80 border-t border-border bg-card">
              {bottom}
            </div>
          ) : null}
        </div>
      </div>
    </div>
  );
}
