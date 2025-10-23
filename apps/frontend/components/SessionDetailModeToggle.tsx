"use client";

import { cn } from "@/lib/utils";
import type { SessionDetailMode } from "@/types";

interface SessionDetailModeToggleProps {
  value: SessionDetailMode;
  onChange: (mode: SessionDetailMode) => void;
  className?: string;
}

export default function SessionDetailModeToggle({
  value,
  onChange,
  className,
}: SessionDetailModeToggleProps) {
  const options: Array<{ value: SessionDetailMode; label: string }> = [
    { value: "user_only", label: "User turns" },
    { value: "conversation", label: "Conversation" },
    { value: "full", label: "Full transcript" },
  ];

  return (
    <div
      className={cn(
        "flex items-center gap-1 rounded-full border border-border bg-background/60 p-1",
        className,
      )}
    >
      {options.map((option) => {
        const isActive = option.value === value;
        return (
          <button
            key={option.value}
            type="button"
            aria-pressed={isActive}
            onClick={() => onChange(option.value)}
            className={cn(
              "rounded-full px-2.5 py-1 text-xs font-medium transition-colors",
              isActive
                ? "bg-primary text-primary-foreground shadow-sm"
                : "text-muted-foreground hover:text-foreground",
            )}
          >
            {option.label}
          </button>
        );
      })}
    </div>
  );
}
