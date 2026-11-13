"use client";

import type { Batch, Movement, StockLevel, Supplier } from "@takar/api-client";
import { CalendarClock, Plus, X } from "lucide-react";
import { useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogSheet,
  DialogTitle,
} from "@/components/ui/dialog";
import { api } from "@/lib/api";
import { daysUntil, formatDate, formatDateTime, formatMoney, formatQty } from "@/lib/format";
import { fill, useI18n } from "@/lib/i18n";
import { readToken } from "@/lib/session";

type Props = {
  material: StockLevel;
  batches: Batch[];
  suppliers: Supplier[];
  canStockIn: boolean;
  onClose: () => void;
  onStockIn: () => void;
};

export function BatchPanel({
  material,
  batches,
  suppliers,
  canStockIn,
  onClose,
  onStockIn,
}: Props) {
  const { t, locale } = useI18n();
  const [movements, setMovements] = useState<Movement[] | null>(null);

  useEffect(() => {
    const token = readToken();

    if (!token) return;

    let cancelled = false;

    api
      .listMovements(token, { materialId: material.material_id, limit: 100 })
      .then((rows) => {
        if (!cancelled) setMovements(rows);
      })
      .catch(() => {
        if (!cancelled) setMovements([]);
      });

    return () => {
      cancelled = true;
    };
  }, [material.material_id]);

  function supplierName(id: string | null): string {
    if (!id) return t.stock.meta.noSupplier;
    return suppliers.find((supplier) => supplier.id === id)?.name ?? t.stock.meta.noSupplier;
  }

  function expiryText(expiresAt: string | null): string {
    if (!expiresAt) return t.stock.meta.noExpiry;

    const days = daysUntil(expiresAt);

    if (days < 0) {
      return `${formatDate(expiresAt, locale)} · ${fill(t.stock.meta.daysOverdue, { days: Math.abs(days) })}`;
    }

    if (days === 0) {
      return `${formatDate(expiresAt, locale)} · ${t.stock.meta.today}`;
    }

    return `${formatDate(expiresAt, locale)} · ${fill(t.stock.meta.daysLeft, { days })}`;
  }

  function movementQty(qty: string): string {
    const formatted = formatQty(qty, locale);
    return Number(qty) > 0 ? `+${formatted}` : formatted;
  }

  return (
    <Dialog
      open
      onOpenChange={(next) => {
        if (!next) onClose();
      }}
    >
      <DialogSheet>
        <header className="flex items-start justify-between gap-3 border-b border-border px-5 pt-5 pb-3.5">
          <div>
            <p className="text-2xs font-bold tracking-widest text-ink-3 uppercase">
              {t.stock.panel.title}
            </p>
            <DialogTitle className="mt-0.5 text-xl font-extrabold">
              {material.name}
            </DialogTitle>
            <p className="mt-1 text-xs text-ink-2">
              {t.stock.panel.onHand}:{" "}
              <strong>
                {formatQty(material.on_hand, locale)} {material.base_unit.code}
              </strong>
            </p>
          </div>
          <DialogClose
            aria-label={t.stock.panel.close}
            className="-mt-1 rounded-md p-1 text-ink-3 transition-colors duration-150 hover:bg-surface-alt hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand"
          >
            <X size={16} />
          </DialogClose>
        </header>

        <div className="flex flex-1 flex-col gap-5 overflow-y-auto px-5 pt-4 pb-6">
          <section>
            <div className="mb-2 flex items-center justify-between gap-3">
              <h3 className="text-2xs font-bold tracking-widest text-ink-3 uppercase">
                {t.stock.panel.batches}
              </h3>
              {canStockIn && (
                <Button
                  variant="secondary"
                  size="sm"
                  type="button"
                  onClick={onStockIn}
                >
                  <Plus size={14} />
                  {t.stock.panel.stockIn}
                </Button>
              )}
            </div>

            {batches.length === 0 ? (
              <p className="text-xs text-ink-3">{t.stock.panel.noBatches}</p>
            ) : (
              <ul className="flex list-none flex-col gap-2 p-0">
                {batches.map((batch, index) => (
                  <li
                    className="flex flex-col gap-1.5 rounded-lg border border-border px-3 py-2.5"
                    key={batch.id}
                  >
                    <div className="flex items-baseline gap-2">
                      <span className="text-2xs font-bold text-ink-3">
                        #{index + 1}
                      </span>
                      <strong className="text-sm font-bold tabular-nums">
                        {formatQty(batch.qty_remaining, locale)}{" "}
                        {material.base_unit.code}
                      </strong>
                      <span className="ml-auto text-xs font-semibold text-ink-2 tabular-nums">
                        {formatMoney(Number(batch.unit_cost), locale)}
                      </span>
                    </div>
                    <dl className="grid grid-cols-3 gap-2">
                      <div>
                        <dt className="text-2xs font-semibold tracking-wide text-ink-3 uppercase">
                          {t.stock.panel.received}
                        </dt>
                        <dd className="mt-0.5 text-2xs text-ink-2">
                          {formatDate(batch.received_at, locale)}
                        </dd>
                      </div>
                      <div>
                        <dt className="text-2xs font-semibold tracking-wide text-ink-3 uppercase">
                          {t.stock.panel.expires}
                        </dt>
                        <dd className="mt-0.5 text-2xs text-ink-2">
                          {expiryText(batch.expires_at)}
                        </dd>
                      </div>
                      <div>
                        <dt className="text-2xs font-semibold tracking-wide text-ink-3 uppercase">
                          {t.stock.panel.supplier}
                        </dt>
                        <dd className="mt-0.5 text-2xs text-ink-2">
                          {supplierName(batch.supplier_id)}
                        </dd>
                      </div>
                    </dl>
                  </li>
                ))}
              </ul>
            )}
          </section>

          <section>
            <h3 className="mb-2 text-2xs font-bold tracking-widest text-ink-3 uppercase">
              {t.stock.panel.movements}
            </h3>

            {movements === null && (
              <p className="text-xs text-ink-3" aria-busy="true">
                …
              </p>
            )}

            {movements !== null && movements.length === 0 && (
              <p className="text-xs text-ink-3">{t.stock.panel.noMovements}</p>
            )}

            {movements !== null && movements.length > 0 && (
              <ul className="flex list-none flex-col gap-2 p-0">
                {movements.map((movement) => (
                  <li
                    className="grid grid-cols-[64px_minmax(0,1fr)_auto] items-baseline gap-2 border-b border-border pb-1.5 text-xs last:border-b-0"
                    key={movement.id}
                  >
                    <span
                      className={`font-bold tabular-nums ${
                        Number(movement.qty) > 0
                          ? "text-brand"
                          : "text-destructive"
                      }`}
                    >
                      {movementQty(movement.qty)}
                    </span>
                    <span className="text-ink-2">
                      {t.stock.panel.types[movement.type]}
                    </span>
                    <span className="text-2xs whitespace-nowrap text-ink-3">
                      {formatDateTime(movement.created_at, locale)}
                    </span>
                    {movement.note && (
                      <span className="col-start-2 col-end-4 text-2xs text-ink-3">
                        {movement.note}
                      </span>
                    )}
                  </li>
                ))}
              </ul>
            )}
          </section>

          <p className="flex items-center gap-1.5 text-2xs text-ink-3">
            <CalendarClock size={13} />
            {fill(t.stock.meta.batches, { count: batches.length })}
          </p>
        </div>
      </DialogSheet>
    </Dialog>
  );
}
