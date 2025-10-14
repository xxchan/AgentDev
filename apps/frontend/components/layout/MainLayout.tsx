import { ReactNode } from 'react';

interface MainLayoutProps {
  sidebar: ReactNode;
  main: ReactNode;
  bottom: ReactNode;
}

export default function MainLayout({ sidebar, main, bottom }: MainLayoutProps) {
  return (
    <div className="h-screen flex flex-col bg-gray-50">
      {/* Header */}
      <header className="bg-white border-b border-gray-200 px-4 py-3 shadow-sm">
        <div className="flex items-center justify-between">
          <div className="flex items-center space-x-2">
            <div className="w-8 h-8 bg-blue-600 rounded-lg flex items-center justify-center">
              <span className="text-white font-bold text-sm">A</span>
            </div>
            <h1 className="text-xl font-semibold text-gray-900">AgentDev UI</h1>
          </div>
          <div className="text-sm text-gray-500">
            Multi-Agent Development Environment
          </div>
        </div>
      </header>

      {/* Main content area */}
      <div className="flex-1 flex overflow-hidden">
        {/* Left sidebar */}
        <aside className="w-80 bg-white border-r border-gray-200 overflow-y-auto">
          {sidebar}
        </aside>

        {/* Right content area (main + bottom) */}
        <div className="flex-1 flex flex-col">
          {/* Main diff viewer */}
          <main className="flex-1 bg-white overflow-y-auto">
            {main}
          </main>

          {/* Bottom processes panel */}
          <div className="h-80 border-t border-gray-200 bg-white">
            {bottom}
          </div>
        </div>
      </div>
    </div>
  );
}
