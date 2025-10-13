'use client';

import { useEffect, useRef } from 'react';
import type * as Diff2Html from 'diff2html';

interface DiffViewerProps {
  diffText: string;
  title?: string;
}

export default function DiffViewer({ diffText, title = 'Diff View' }: DiffViewerProps) {
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!containerRef.current) return;

    let isMounted = true;

    const renderDiff = async () => {
      if (!containerRef.current || !isMounted) return;

      // Clear previous content
      containerRef.current.innerHTML = '';

      if (!diffText.trim()) {
        containerRef.current.innerHTML = `
          <div class="flex items-center justify-center h-64 text-gray-500">
            <div class="text-center">
              <svg class="mx-auto h-12 w-12 text-gray-400 mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
              </svg>
              <p class="text-lg">No changes to display</p>
              <p class="text-sm text-gray-400 mt-1">Select a task or agent to view code changes</p>
            </div>
          </div>
        `;
        return;
      }

      try {
        // Dynamically import diff2html
        const Diff2Html = await import('diff2html');
        
        if (!isMounted || !containerRef.current) return;

        // Generate diff HTML using diff2html
        const diffHtml = Diff2Html.html(diffText, {
          drawFileList: false,
          matching: 'lines',
          outputFormat: 'side-by-side',
          renderNothingWhenEmpty: false,
        });

        containerRef.current.innerHTML = diffHtml;
      } catch (error) {
        console.error('Error rendering diff:', error);
        if (containerRef.current && isMounted) {
          containerRef.current.innerHTML = `
            <div class="p-4 bg-red-50 border border-red-200 rounded-md">
              <div class="flex">
                <svg class="h-5 w-5 text-red-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                </svg>
                <div class="ml-3">
                  <h3 class="text-sm font-medium text-red-800">Error rendering diff</h3>
                  <div class="mt-2 text-sm text-red-700">
                    <pre class="whitespace-pre-wrap font-mono text-xs">${error}</pre>
                  </div>
                </div>
              </div>
            </div>
          `;
        }
      }
    };

    renderDiff();

    return () => {
      isMounted = false;
    };
  }, [diffText]);

  return (
    <div className="h-full flex flex-col">
      {/* Header */}
      <div className="flex-none px-6 py-4 bg-gray-50 border-b border-gray-200">
        <div className="flex items-center justify-between">
          <h2 className="text-lg font-medium text-gray-900">{title}</h2>
          <div className="flex items-center space-x-2 text-sm text-gray-500">
            <span>Last updated: {new Date().toLocaleTimeString()}</span>
          </div>
        </div>
      </div>

      {/* Diff content */}
      <div className="flex-1 overflow-auto">
        <div 
          ref={containerRef} 
          className="h-full min-h-0" 
        />
      </div>
    </div>
  );
}
