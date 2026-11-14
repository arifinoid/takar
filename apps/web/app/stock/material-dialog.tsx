"use client";

import type { Material, Supplier, Unit } from "@takar/api-client";
import { useState } from "react";

import { Modal } from "@/components/modal";
import { Button } from "@/components/ui/button";
import { Field, FormError } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { api } from "@/lib/api";
import { normalizeDecimal } from "@/lib/format";
import { useI18n } from "@/lib/i18n";
import { readToken } from "@/lib/session";

import { stockError } from "./error-copy";

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
      <form className="flex flex-col gap-4" onSubmit={submit}>
        <div className="grid gap-3 sm:grid-cols-2">
          <Field label={t.stock.form.name} className="sm:col-span-2">
            <Input
              type="text"
              value={name}
              onChange={(event) => setName(event.target.value)}
              maxLength={120}
              required
              autoFocus
            />
          </Field>

          <Field label={t.stock.form.unit}>
            <Select
              value={unitId}
              onChange={(event) => setUnitId(event.target.value)}
              required
            >
              {units.map((unit) => (
                <option key={unit.id} value={unit.id}>
                  {unit.code} · {unit.name}
                </option>
              ))}
            </Select>
          </Field>

          <Field label={t.stock.form.minStock}>
            <Input
              type="text"
              inputMode="decimal"
              value={minStock}
              onChange={(event) => setMinStock(event.target.value)}
            />
          </Field>

          <Field label={t.stock.form.shelfLife} hint={t.stock.form.shelfLifeHint}>
            <Input
              type="number"
              min={0}
              step={1}
              value={shelfLife}
              onChange={(event) => setShelfLife(event.target.value)}
            />
          </Field>

          <Field label={t.stock.form.defaultSupplier}>
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
        </div>

        {error && <FormError>{error}</FormError>}

        <div className="flex justify-end gap-2">
          <Button variant="ghost" type="button" onClick={onClose}>
            {t.stock.form.cancel}
          </Button>
          <Button type="submit" disabled={saving}>
            {saving ? t.stock.form.creating : t.stock.form.create}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
