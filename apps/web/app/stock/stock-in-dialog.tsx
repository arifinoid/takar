"use client";

import { ApiError, type Batch, type Material, type Supplier } from "@takar/api-client";
import { useState } from "react";

import { api } from "@/lib/api";
import { addDaysIso, normalizeDecimal, todayIso } from "@/lib/format";
import { useI18n } from "@/lib/i18n";
import { readToken } from "@/lib/session";

import { stockError } from "./error-copy";
import { Modal } from "@/components/modal";

type Props = {
  materials: Material[];
  suppliers: Supplier[];
  defaultMaterialId?: string | null;
  onClose: () => void;
  onSaved: (batch: Batch) => void;
};

export function StockInDialog({
  materials,
  suppliers,
  defaultMaterialId,
  onClose,
  onSaved,
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

    setSaving(true);

    try {
      const batch = await api.stockIn(token, {
        material_id: materialId,
        supplier_id: supplierId || null,
        received_at: receivedAt,
        expires_at: expiresAt || null,
        qty_received: quantity,
        unit_cost: cost,
        note: note.trim() || null,
      });

      onSaved(batch);
    } catch (cause) {
      setError(stockError(t, cause));

      if (cause instanceof ApiError && cause.status === 401) {
        onClose();
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
        <p className="modal__empty">{t.stock.empty.body}</p>
      ) : (
        <form className="form" onSubmit={submit}>
          <div className="form__grid">
            <label className="field">
              <span className="field__label">{t.stock.form.material}</span>
              <select
                className="field__input"
                value={materialId}
                onChange={(event) => chooseMaterial(event.target.value)}
                required
              >
                {materials.map((item) => (
                  <option key={item.id} value={item.id}>
                    {item.name} ({item.base_unit.code})
                  </option>
                ))}
              </select>
            </label>

            <label className="field">
              <span className="field__label">{t.stock.form.supplier}</span>
              <select
                className="field__input"
                value={supplierId}
                onChange={(event) => setSupplierId(event.target.value)}
              >
                <option value="">{t.stock.form.supplierNone}</option>
                {suppliers.map((supplier) => (
                  <option key={supplier.id} value={supplier.id}>
                    {supplier.name}
                  </option>
                ))}
              </select>
            </label>

            <label className="field">
              <span className="field__label">{t.stock.form.receivedAt}</span>
              <input
                className="field__input"
                type="date"
                value={receivedAt}
                onChange={(event) => chooseReceivedAt(event.target.value)}
                required
              />
            </label>

            <label className="field">
              <span className="field__label">{t.stock.form.expiresAt}</span>
              <input
                className="field__input"
                type="date"
                value={expiresAt}
                onChange={(event) => setExpiresAt(event.target.value)}
              />
              <span className="field__hint">{t.stock.form.expiresAtHint}</span>
            </label>

            <label className="field">
              <span className="field__label">
                {t.stock.form.qty}
                {unit ? ` (${unit})` : ""}
              </span>
              <input
                className="field__input"
                type="text"
                inputMode="decimal"
                autoComplete="off"
                placeholder="0"
                value={qty}
                onChange={(event) => setQty(event.target.value)}
                required
              />
            </label>

            <label className="field">
              <span className="field__label">{t.stock.form.unitCost}</span>
              <input
                className="field__input"
                type="text"
                inputMode="decimal"
                autoComplete="off"
                placeholder="0"
                value={unitCost}
                onChange={(event) => setUnitCost(event.target.value)}
                required
              />
            </label>

            <label className="field form__wide">
              <span className="field__label">{t.stock.form.note}</span>
              <input
                className="field__input"
                type="text"
                placeholder={t.stock.form.notePlaceholder}
                value={note}
                onChange={(event) => setNote(event.target.value)}
              />
            </label>
          </div>

          {error && (
            <p className="form__error" role="alert">
              {error}
            </p>
          )}

          <div className="form__actions">
            <button className="btn btn--ghost" type="button" onClick={onClose}>
              {t.stock.form.cancel}
            </button>
            <button className="btn btn--primary" type="submit" disabled={saving}>
              {saving ? t.stock.form.saving : t.stock.form.submit}
            </button>
          </div>
        </form>
      )}
    </Modal>
  );
}
