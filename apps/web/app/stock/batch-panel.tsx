"use client";

import type { Batch, Movement, StockLevel, Supplier } from "@takar/api-client";
import { CalendarClock, Plus, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";

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
  const ref = useRef<HTMLDialogElement>(null);
  const [movements, setMovements] = useState<Movement[] | null>(null);

  useEffect(() => {
    const dialog = ref.current;
    if (dialog && !dialog.open) dialog.showModal();
  }, []);

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
    <dialog
      ref={ref}
      className="drawer"
      onClose={onClose}
      onClick={(event) => {
        if (event.target === ref.current) onClose();
      }}
    >
      <div className="drawer__panel">
        <header className="drawer__head">
          <div>
            <p className="eyebrow">{t.stock.panel.title}</p>
            <h2>{material.name}</h2>
            <p className="drawer__sub">
              {t.stock.panel.onHand}:{" "}
              <strong>
                {formatQty(material.on_hand, locale)} {material.base_unit.code}
              </strong>
            </p>
          </div>
          <button
            className="icon-btn"
            type="button"
            onClick={onClose}
            aria-label={t.stock.panel.close}
          >
            <X size={16} />
          </button>
        </header>

        <div className="drawer__body">
          <section className="drawer__section">
            <div className="drawer__section-head">
              <h3>{t.stock.panel.batches}</h3>
              {canStockIn && (
                <button className="btn btn--secondary" type="button" onClick={onStockIn}>
                  <Plus size={14} />
                  {t.stock.panel.stockIn}
                </button>
              )}
            </div>

            {batches.length === 0 ? (
              <p className="drawer__empty">{t.stock.panel.noBatches}</p>
            ) : (
              <ul className="batch-list">
                {batches.map((batch, index) => (
                  <li className="batch" key={batch.id}>
                    <div className="batch__top">
                      <span className="batch__rank">#{index + 1}</span>
                      <strong className="batch__qty">
                        {formatQty(batch.qty_remaining, locale)} {material.base_unit.code}
                      </strong>
                      <span className="batch__cost">
                        {formatMoney(Number(batch.unit_cost), locale)}
                      </span>
                    </div>
                    <dl className="batch__facts">
                      <div>
                        <dt>{t.stock.panel.received}</dt>
                        <dd>{formatDate(batch.received_at, locale)}</dd>
                      </div>
                      <div>
                        <dt>{t.stock.panel.expires}</dt>
                        <dd>{expiryText(batch.expires_at)}</dd>
                      </div>
                      <div>
                        <dt>{t.stock.panel.supplier}</dt>
                        <dd>{supplierName(batch.supplier_id)}</dd>
                      </div>
                    </dl>
                  </li>
                ))}
              </ul>
            )}
          </section>

          <section className="drawer__section">
            <h3>{t.stock.panel.movements}</h3>

            {movements === null && <p className="drawer__empty">…</p>}

            {movements !== null && movements.length === 0 && (
              <p className="drawer__empty">{t.stock.panel.noMovements}</p>
            )}

            {movements !== null && movements.length > 0 && (
              <ul className="movement-list">
                {movements.map((movement) => (
                  <li className="movement" key={movement.id}>
                    <span
                      className={`movement__qty movement__qty--${
                        Number(movement.qty) > 0 ? "in" : "out"
                      }`}
                    >
                      {movementQty(movement.qty)}
                    </span>
                    <span className="movement__type">
                      {t.stock.panel.types[movement.type]}
                    </span>
                    <span className="movement__date">
                      {formatDateTime(movement.created_at, locale)}
                    </span>
                    {movement.note && (
                      <span className="movement__note">{movement.note}</span>
                    )}
                  </li>
                ))}
              </ul>
            )}
          </section>

          <p className="drawer__foot">
            <CalendarClock size={13} />
            {fill(t.stock.meta.batches, { count: batches.length })}
          </p>
        </div>
      </div>
    </dialog>
  );
}
