import { cn } from "@/lib/utils";

import { Skeleton } from "@/components/ui/skeleton";

/**
 * Announces that a region is loading while the blocks stand in for the content
 * that is on its way. The label is the only thing a screen reader gets.
 */
function LoadingRegion({
  label,
  className,
  children,
}: {
  label: string;
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <div role="status" aria-busy="true" className={className}>
      <span className="sr-only">{label}</span>
      {children}
    </div>
  );
}

/** Row of stat tiles, matching the metric cards on the overview. */
function SkeletonStats({ count = 4 }: { count?: number }) {
  return (
    <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
      {Array.from({ length: count }).map((_, index) => (
        <div
          key={index}
          className="flex flex-col gap-3 rounded-xl border border-border bg-card p-4"
        >
          <Skeleton className="h-2.5 w-16" />
          <Skeleton className="h-5 w-24" />
          <Skeleton className="h-2 w-12" />
        </div>
      ))}
    </div>
  );
}

/** Header plus rows, for a table that has not loaded yet. */
function SkeletonTable({
  rows = 6,
  columns = 5,
  className,
}: {
  rows?: number;
  columns?: number;
  className?: string;
}) {
  return (
    <div className={cn("flex flex-col", className)}>
      <div className="flex gap-4 border-b border-border pb-3">
        {Array.from({ length: columns }).map((_, column) => (
          <Skeleton
            key={column}
            className={cn("h-2.5", column === 0 ? "w-32" : "w-16")}
          />
        ))}
      </div>
      {Array.from({ length: rows }).map((_, row) => (
        <div key={row} className="flex items-center gap-4 border-b border-border py-3.5 last:border-b-0">
          {Array.from({ length: columns }).map((_, column) => (
            <Skeleton
              key={column}
              className={cn(
                "h-3",
                column === 0 ? "w-40" : column === columns - 1 ? "w-14" : "w-20",
              )}
            />
          ))}
        </div>
      ))}
    </div>
  );
}

/** Stacked cards, for the list pages (purchasing, sales history, products). */
function SkeletonList({ rows = 4 }: { rows?: number }) {
  return (
    <div className="flex flex-col gap-2.5">
      {Array.from({ length: rows }).map((_, index) => (
        <div
          key={index}
          className="flex flex-col gap-2.5 rounded-xl border border-border bg-card px-4 py-3.5"
        >
          <div className="flex items-center justify-between gap-4">
            <Skeleton className="h-3.5 w-44" />
            <Skeleton className="h-3.5 w-16" />
          </div>
          <div className="flex gap-4">
            <Skeleton className="h-2.5 w-24" />
            <Skeleton className="h-2.5 w-20" />
            <Skeleton className="h-2.5 w-28" />
          </div>
        </div>
      ))}
    </div>
  );
}

/** Labelled fields plus an action, for the form-driven screens. */
function SkeletonForm({ fields = 4 }: { fields?: number }) {
  return (
    <div className="rounded-xl border border-border bg-card p-5">
      <div className="grid gap-4 sm:grid-cols-2">
        {Array.from({ length: fields }).map((_, index) => (
          <div key={index} className="flex flex-col gap-2">
            <Skeleton className="h-2.5 w-20" />
            <Skeleton className="h-11 w-full rounded-lg" />
          </div>
        ))}
      </div>
      <div className="mt-5 flex gap-2">
        <Skeleton className="h-11 w-32 rounded-lg" />
        <Skeleton className="h-11 w-24 rounded-lg" />
      </div>
    </div>
  );
}

export {
  LoadingRegion,
  SkeletonStats,
  SkeletonTable,
  SkeletonList,
  SkeletonForm,
};
