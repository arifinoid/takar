"use client";

import type { Material, Supplier, Unit } from "@takar/api-client";
import { useState } from "react";

import { api } from "@/lib/api";
import { normalizeDecimal } from "@/lib/format";
import { useI18n } from "@/lib/i18n";
import { readToken } from "@/lib/session";

import { stockError } from "./error-copy";
import { Modal } from "@/components/modal";

type Props = {
  units: Unit[];
  suppliers: Supplier[];
  onClose: () => void;
  onCreated: (material: Material) => void;
};

export function MaterialDialog({ units, suppliers, onClose, onCreated }: Props) {
  const { t } = useI18n();
  const [name, setName] = useState("");
  const [unitId, setUnitId] = useState(units[0]?.id ?? "");
  const [minStock, setMinStock] = useState("0");
  const [shelfLife, setShelfLife] = useState("");
  const [supplierId, setSupplierId] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function submit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError(null);

    const token = readToken();

    if (!token) {
      onClose();
      return;
    }

    const minimum = normalizeDecimal(minStock);

    if (minimum === null) {
      setError(t.stock.errors.invalidMinStock);
      return;
    }

    let shelfLifeDays: number | null = null;

    if (shelfLife.trim() !== "") {
      shelfLifeDays = Number(shelfLife);

      if (!Number.isInteger(shelfLifeDays) || shelfLifeDays < 0) {
        setError(t.stock.errors.invalidShelfLife);
        return;
      }
    }

    setSaving(true);

    try {
      const material = await api.createMaterial(token, {
        name: name.trim(),
        base_unit_id: unitId,
        min_stock: minimum,
        shelf_life_days: shelfLifeDays,
        default_supplier_id: supplierId || null,
      });

      onCreated(material);
    } catch (cause) {
      setError(stockError(t, cause));
    } finally {
      setSaving(false);
    }
  }

  return (
    <Modal
      open
      title={t.stock.newMaterialTitle}
      closeLabel={t.stock.panel.close}
      onClose={onClose}
    >
      <form className="form" onSubmit={submit}>
        <div className="form__grid">
          <label className="field form__wide">
            <span className="field__label">{t.stock.form.name}</span>
            <input
              className="field__input"
              type="text"
              value={name}
              onChange={(event) => setName(event.target.value)}
              maxLength={120}
              required
              autoFocus
            />
          </label>

          <label className="field">
            <span className="field__label">{t.stock.form.unit}</span>
            <select
              className="field__input"
              value={unitId}
              onChange={(event) => setUnitId(event.target.value)}
              required
            >
              {units.map((unit) => (
                <option key={unit.id} value={unit.id}>
                  {unit.code} · {unit.name}
                </option>
              ))}
            </select>
          </label>

          <label className="field">
            <span className="field__label">{t.stock.form.minStock}</span>
            <input
              className="field__input"
              type="text"
              inputMode="decimal"
              value={minStock}
              onChange={(event) => setMinStock(event.target.value)}
            />
          </label>

          <label className="field">
            <span className="field__label">{t.stock.form.shelfLife}</span>
            <input
              className="field__input"
              type="number"
              min={0}
              step={1}
              value={shelfLife}
              onChange={(event) => setShelfLife(event.target.value)}
            />
            <span className="field__hint">{t.stock.form.shelfLifeHint}</span>
          </label>

          <label className="field">
            <span className="field__label">{t.stock.form.defaultSupplier}</span>
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
            {saving ? t.stock.form.creating : t.stock.form.create}
          </button>
        </div>
      </form>
    </Modal>
  );
}
