import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";

import { cn } from "@/lib/utils";

const badgeVariants = cva(
  "inline-flex w-fit items-center gap-1 rounded-full px-2.5 py-1 text-2xs font-bold whitespace-nowrap",
  {
    variants: {
      variant: {
        neutral: "bg-surface-alt text-ink-2",
        brand: "bg-secondary text-secondary-foreground",
        warning: "bg-warning-soft text-warning",
        danger: "bg-danger-soft text-destructive",
        outline: "border border-border text-ink-2",
      },
    },
    defaultVariants: {
      variant: "neutral",
    },
  },
);

function Badge({
  className,
  variant,
  asChild = false,
  ...props
}: React.ComponentProps<"span"> &
  VariantProps<typeof badgeVariants> & { asChild?: boolean }) {
  const Comp = asChild ? Slot : "span";

  return (
    <Comp
      data-slot="badge"
      className={cn(badgeVariants({ variant }), className)}
      {...props}
    />
  );
}

export { Badge, badgeVariants };
