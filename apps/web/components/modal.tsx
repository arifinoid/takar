"use client";

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

type Props = {
  open: boolean;
  title: string;
  subtitle?: string;
  closeLabel: string;
  wide?: boolean;
  onClose: () => void;
  children: React.ReactNode;
};

/**
 * Radix dialog: focus trap, Escape, scroll lock and the aria wiring come from
 * the primitive, so a form only has to describe its own content.
 */
export function Modal({
  open,
  title,
  subtitle,
  closeLabel,
  wide,
  onClose,
  children,
}: Props) {
  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) onClose();
      }}
    >
      <DialogContent
        closeLabel={closeLabel}
        className={wide ? "w-[min(720px,calc(100vw-32px))]" : undefined}
      >
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          {subtitle && <DialogDescription>{subtitle}</DialogDescription>}
        </DialogHeader>
        {children}
      </DialogContent>
    </Dialog>
  );
}
