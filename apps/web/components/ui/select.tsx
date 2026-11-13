import { ChevronDown } from "lucide-react";

import { cn } from "@/lib/utils";

/**
 * Native select, styled to sit next to `Input`. The platform picker is better
 * than a custom listbox on the cheap Android WebView the shell runs on.
 */
function Select({ className, children, ...props }: React.ComponentProps<"select">) {
  return (
    <div className="relative">
      <select
        data-slot="select"
        className={cn(
          "h-11 w-full appearance-none rounded-lg border border-input bg-card pr-9 pl-3 text-base text-foreground transition-[border-color,box-shadow] duration-150 outline-none focus-visible:border-primary focus-visible:shadow-[0_0_0_3px_var(--brand-soft)] disabled:cursor-not-allowed disabled:opacity-55",
          className,
        )}
        {...props}
      >
        {children}
      </select>
      <ChevronDown
        size={15}
        aria-hidden="true"
        className="pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 text-ink-3"
      />
    </div>
  );
}

export { Select };
