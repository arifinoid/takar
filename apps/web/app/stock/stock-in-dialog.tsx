"use client";

import { ApiError, type Batch, type Material, type Supplier } from "@takar/api-client";
import { useRef, useState } from "react";

import { Modal } from "@/components/modal";
import { Button } from "@/components/ui/button";
import { Field, FormError } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { api } from "@/lib/api";
import { addDaysIso, normalizeDecimal, todayIso } from "@/lib/format";
import { useI18n } from "@/lib/i18n";
import {
  createClientRef,
  enqueueStockIn,
  offlineQueueAvailable,
} from "@/lib/outbox";
import { readToken } from "@/lib/session";

import { stockError } from "./error-copy";

type Props = {
  materials: Material[];
  suppliers: Supplier[];
  defaultMaterialId?: string | null;
  onClose: () => void;
  onSaved: (batch: Batch) => void;
  /** The receipt was written to the local queue instead of the server. */
  onQueued: () => void;
};

export function StockInDialog({
  materials,
  suppliers,
  defaultMaterialId,
  onClose,
  onSaved,
  onQueued,
}: Props) {
  const { t } = useI18n();
  const initial =
    materials.find((material) => material.id === defaultMaterialId) ?? materials[0];
  const today = todayIso();

  const [materialId, setMaterialId] = useState(initial?.id ?? "");
  const [supplierId, setSupplierId] = useState(initial?.default_supplier_id ?? "");
  const [receivedAt, setReceivedAt] = useState(today);
  const [expiresAt, setExpiresAt] = useState(
    initial?.shelf_life_days != null
      ? addDaysIso(today, initial.shelf_life_days)
      : "",
  );
  const [qty, setQty] = useState("");
  const [unitCost, setUnitCost] = useState("");
  const [note, setNote] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // One reference per dialog, so a double submit or a retry cannot duplicate
  // the receipt on the server.
  const clientRef = useRef(createClientRef());

  const material = materials.find((item) => item.id === materialId);
  const unit = material?.base_unit.code ?? "";

  function chooseMaterial(id: string) {
    setMaterialId(id);

    const next = materials.find((item) => item.id === id);
    setSupplierId(next?.default_supplier_id ?? "");
    setExpiresAt(
      next?.shelf_life_days != null
        ? addDaysIso(receivedAt, next.shelf_life_days)
        : "",
    );
  }

  function chooseReceivedAt(value: string) {
    setReceivedAt(value);

    if (material?.shelf_life_days != null) {
      setExpiresAt(addDaysIso(value, material.shelf_life_days));
    }
  }

  async function submit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError(null);

    const token = readToken();

    if (!token) {
      onClose();
      return;
    }

    const quantity = normalizeDecimal(qty);
    const cost = normalizeDecimal(unitCost);

    if (!quantity || Number(quantity) <= 0) {
      setError(t.stock.errors.invalidQuantity);
      return;
    }

    if (!cost) {
      setError(t.stock.errors.invalidNumber);
      return;
    }

    const body = {
      material_id: materialId,
      supplier_id: supplierId || null,
      received_at: receivedAt,
      expires_at: expiresAt || null,
      qty_received: quantity,
      unit_cost: cost,
      note: note.trim() || null,
      client_ref: clientRef.current,
    };

    setSaving(true);

    // With no connection the receipt goes straight to the local queue; the
    // stock page flushes it when the device comes back online.
    if (offlineQueueAvailable() && typeof navigator !== "undefined" && !navigator.onLine) {
      const queued = await enqueueStockIn(body);

      setSaving(false);

      if (queued) {
        onQueued();
      } else {
        setError(t.stock.offline.saveFailed);
      }

      return;
    }

    try {
      const batch = await api.stockIn(token, body);

      onSaved(batch);
    } catch (cause) {
      if (cause instanceof ApiError) {
        setError(stockError(t, cause));

        if (cause.status === 401) {
          onClose();
        }
      } else if (offlineQueueAvailable() && (await enqueueStockIn(body))) {
        // The request never reached the server: keep it locally instead of
        // losing what the user typed.
        onQueued();
      } else {
        setError(t.stock.offline.saveFailed);
      }
    } finally {
      setSaving(false);
    }
  }

  return (
    <Modal
      open
      title={t.stock.stockInTitle}
      subtitle={t.stock.stockInSubtitle}
      closeLabel={t.stock.panel.close}
      onClose={onClose}
    >
      {materials.length === 0 ? (
        <p className="text-sm text-muted-foreground">{t.stock.empty.body}</p>
      ) : (
        <form className="flex flex-col gap-4" onSubmit={submit}>
          <div className="grid gap-3 sm:grid-cols-2">
            <Field label={t.stock.form.material}>
              <Select
                value={materialId}
                onChange={(event) => chooseMaterial(event.target.value)}
                required
              >
                {materials.map((item) => (
                  <option key={item.id} value={item.id}>
                    {item.name} ({item.base_unit.code})
                  </option>
                ))}
              </Select>
            </Field>

            <Field label={t.stock.form.supplier}>
              <Select
                value={supplierId}
                onChange={(event) => setSupplierId(event.target.value)}
              >
                <option value="">{t.stock.form.supplierNone}</option>
                {suppliers.map((supplier) => (
                  <option key={supplier.id} value={supplier.id}>
                    {supplier.name}
                  </option>
                ))}
              </Select>
            </Field>

            <Field label={t.stock.form.receivedAt}>
              <Input
                type="date"
                value={receivedAt}
                onChange={(event) => chooseReceivedAt(event.target.value)}
                required
              />
            </Field>

            <Field
              label={t.stock.form.expiresAt}
              hint={t.stock.form.expiresAtHint}
            >
              <Input
                type="date"
                value={expiresAt}
                onChange={(event) => setExpiresAt(event.target.value)}
              />
            </Field>

            <Field label={`${t.stock.form.qty}${unit ? ` (${unit})` : ""}`}>
              <Input
                type="text"
                inputMode="decimal"
                autoComplete="off"
                placeholder="0"
                value={qty}
                onChange={(event) => setQty(event.target.value)}
                required
              />
            </Field>

            <Field label={t.stock.form.unitCost}>
              <Input
                type="text"
                inputMode="decimal"
                autoComplete="off"
                placeholder="0"
                value={unitCost}
                onChange={(event) => setUnitCost(event.target.value)}
                required
              />
            </Field>

            <Field label={t.stock.form.note} className="sm:col-span-2">
              <Input
                type="text"
                placeholder={t.stock.form.notePlaceholder}
                value={note}
                onChange={(event) => setNote(event.target.value)}
              />
            </Field>
          </div>

          {error && <FormError>{error}</FormError>}

          <div className="flex justify-end gap-2">
            <Button variant="ghost" type="button" onClick={onClose}>
              {t.stock.form.cancel}
            </Button>
            <Button type="submit" disabled={saving}>
              {saving ? t.stock.form.saving : t.stock.form.submit}
            </Button>
          </div>
        </form>
      )}
    </Modal>
  );
}
