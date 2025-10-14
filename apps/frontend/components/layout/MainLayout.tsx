import { ReactNode } from 'react';

interface MainLayoutProps {
  sidebar: ReactNode;
  main: ReactNode;
  bottom: ReactNode;
}

export default function MainLayout({ sidebar, main, bottom }: MainLayoutProps) {
  return (
    <div className="flex min-h-screen flex-col bg-background">
      {/* Header */}
      <header className="border-b border-border bg-card px-4 py-3 shadow-sm">
        <div className="flex items-center justify-between">
          <div className="flex items-center space-x-2">
            <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-primary text-primary-foreground">
              <span className="text-sm font-bold">A</span>
            </div>
            <h1 className="text-xl font-semibold text-foreground">AgentDev UI</h1>
          </div>
          <div className="text-sm text-muted-foreground">
            Multi-Agent Development Environment
          </div>
        </div>
      </header>

      {/* Main content area */}
      <div className="flex-1 flex overflow-hidden">
        {/* Left sidebar */}
        <aside className="w-80 overflow-y-auto border-r border-border bg-card">
          {sidebar}
        </aside>

        {/* Right content area (main + bottom) */}
        <div className="flex-1 flex flex-col">
          {/* Main diff viewer */}
          <main className="flex-1 overflow-y-auto bg-card">
            {main}
          </main>

          {/* Bottom processes panel */}
          <div className="h-80 border-t border-border bg-card">
            {bottom}
          </div>
        </div>
      </div>
    </div>
  );
}
