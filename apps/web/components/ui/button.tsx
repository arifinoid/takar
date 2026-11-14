import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";

import { cn } from "@/lib/utils";

const buttonVariants = cva(
  "inline-flex shrink-0 items-center justify-center gap-2 whitespace-nowrap rounded-lg text-sm font-bold transition-[background-color,border-color,color,opacity,transform] duration-150 ease-out outline-none focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand active:scale-[0.96] disabled:pointer-events-none disabled:opacity-55 disabled:active:scale-100 motion-reduce:active:scale-100 [&_svg]:pointer-events-none [&_svg]:shrink-0",
  {
    variants: {
      variant: {
        default: "bg-primary text-primary-foreground hover:bg-brand-dark",
        secondary:
          "bg-secondary text-secondary-foreground hover:bg-[#d8ebe1]",
        ghost:
          "border border-border bg-transparent text-brand hover:bg-surface-alt",
        outline:
          "border border-input bg-card text-foreground hover:bg-surface-alt",
        destructive:
          "bg-destructive text-destructive-foreground hover:bg-[#953024]",
        link: "text-brand underline-offset-4 hover:underline",
      },
      size: {
        sm: "h-8 px-3 text-xs",
        default: "h-11 px-4",
        lg: "h-12 px-5",
        icon: "size-10",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  },
);

function Button({
  className,
  variant,
  size,
  asChild = false,
  ...props
}: React.ComponentProps<"button"> &
  VariantProps<typeof buttonVariants> & { asChild?: boolean }) {
  const Comp = asChild ? Slot : "button";

  return (
    <Comp
      data-slot="button"
      className={cn(buttonVariants({ variant, size }), className)}
      {...props}
    />
  );
}

export { Button, buttonVariants };
