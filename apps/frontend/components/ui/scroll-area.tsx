"use client";

import * as React from "react";
import * as ScrollAreaPrimitive from "@radix-ui/react-scroll-area";

import { cn } from "@/lib/utils";

interface ScrollAreaProps
  extends React.ComponentPropsWithoutRef<typeof ScrollAreaPrimitive.Root> {
  showHorizontalScrollbar?: boolean;
  viewportClassName?: string;
}

const ScrollArea = React.forwardRef<
  React.ElementRef<typeof ScrollAreaPrimitive.Root>,
  ScrollAreaProps
>(({ className, children, showHorizontalScrollbar = false, viewportClassName, ...props }, ref) => (
  <ScrollAreaPrimitive.Root
    ref={ref}
    className={cn("relative h-full overflow-hidden", className)}
    {...props}
  >
    <ScrollAreaPrimitive.Viewport
      className={cn("h-full w-full rounded-[inherit]", viewportClassName)}
    >
      {children}
    </ScrollAreaPrimitive.Viewport>
    <ScrollBar />
    {showHorizontalScrollbar ? <ScrollBar orientation="horizontal" /> : null}
    {showHorizontalScrollbar ? <ScrollAreaPrimitive.Corner /> : null}
  </ScrollAreaPrimitive.Root>
));
ScrollArea.displayName = ScrollAreaPrimitive.Root.displayName;

const ScrollBar = React.forwardRef<
  React.ElementRef<typeof ScrollAreaPrimitive.Scrollbar>,
  React.ComponentPropsWithoutRef<typeof ScrollAreaPrimitive.Scrollbar>
>(({ className, orientation = "vertical", ...props }, ref) => (
  <ScrollAreaPrimitive.Scrollbar
    ref={ref}
    orientation={orientation}
    className={cn(
      "flex touch-none select-none bg-border/15 transition-colors",
      "data-[orientation=vertical]:h-full data-[orientation=vertical]:w-2.5",
      "data-[orientation=horizontal]:h-2.5 data-[orientation=horizontal]:w-full",
      className,
    )}
    {...props}
  >
    <ScrollAreaPrimitive.Thumb className="relative flex-1 rounded-full bg-border" />
  </ScrollAreaPrimitive.Scrollbar>
));
ScrollBar.displayName = ScrollAreaPrimitive.Scrollbar.displayName;

export { ScrollArea, ScrollBar };
