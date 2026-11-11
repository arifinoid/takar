"use client";

import { useEffect, useRef } from "react";
import { X } from "lucide-react";

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
 * Native <dialog> so the browser handles focus trapping and Escape. The card
 * is a child, so a click on the backdrop is a click on the dialog itself.
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
  const ref = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const dialog = ref.current;

    if (!dialog) return;

    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
  }, [open]);

  return (
    <dialog
      ref={ref}
      className={`modal${wide ? " modal--wide" : ""}`}
      onClose={onClose}
      onClick={(event) => {
        if (event.target === ref.current) onClose();
      }}
    >
      <div className="modal__card">
        <header className="modal__head">
          <div>
            <h2>{title}</h2>
            {subtitle && <p>{subtitle}</p>}
          </div>
          <button
            className="icon-btn"
            type="button"
            onClick={onClose}
            aria-label={closeLabel}
          >
            <X size={16} />
          </button>
        </header>
        {open && children}
      </div>
    </dialog>
  );
}
