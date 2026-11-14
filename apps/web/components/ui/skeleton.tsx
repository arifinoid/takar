import { cn } from "@/lib/utils";

/**
 * Placeholder for content that has not arrived yet. Mirrors the legacy
 * `.skeleton` shimmer so a page in transition looks the same everywhere.
 */
function Skeleton({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      aria-hidden="true"
      data-slot="skeleton"
      className={cn(
        "animate-shimmer rounded-md bg-[linear-gradient(90deg,var(--surface-alt)_25%,#eef0ec_50%,var(--surface-alt)_75%)] bg-[length:200%_100%] motion-reduce:animate-none",
        className,
      )}
      {...props}
    />
  );
}

export { Skeleton };
