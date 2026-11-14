import { cn } from "@/lib/utils";

function Input({ className, type, ...props }: React.ComponentProps<"input">) {
  return (
    <input
      type={type}
      data-slot="input"
      className={cn(
        "h-11 w-full min-w-0 rounded-lg border border-input bg-card px-3 text-base text-foreground transition-[border-color,box-shadow] duration-150 outline-none placeholder:text-ink-3 focus-visible:border-primary focus-visible:shadow-[0_0_0_3px_var(--brand-soft)] disabled:cursor-not-allowed disabled:opacity-55",
        className,
      )}
      {...props}
    />
  );
}

export { Input };
