import { cn } from "@/lib/utils";

/**
 * Label, control and hint as one unit. The wrapper is the `label` element, so
 * clicking the text focuses the control without threading ids through every
 * form.
 */
function Field({
  label,
  hint,
  className,
  children,
}: {
  label: string;
  hint?: string;
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <label className={cn("flex flex-col gap-1.5", className)}>
      <span className="text-xs font-semibold text-muted-foreground select-none">
        {label}
      </span>
      {children}
      {hint && (
        <span className="text-2xs leading-snug text-ink-3">{hint}</span>
      )}
    </label>
  );
}

function FormError({ children }: { children: React.ReactNode }) {
  return (
    <p
      role="alert"
      className="rounded-lg bg-danger-soft px-3 py-2.5 text-xs text-destructive"
    >
      {children}
    </p>
  );
}

export { Field, FormError };
