"use client";

import {
  ApiError,
  type Material,
  type Product,
  type Recipe,
} from "@takar/api-client";
import { Plus, Trash2 } from "lucide-react";
import { useRef, useState } from "react";

import { Modal } from "@/components/modal";
import { api } from "@/lib/api";
import { normalizeDecimal } from "@/lib/format";
import { useI18n, type Dictionary } from "@/lib/i18n";
import { readToken } from "@/lib/session";

type Props = {
  product: Product;
  materials: Material[];
  /** The active recipe, used to prefill a new version. */
  initial?: Recipe;
  onClose: () => void;
  onSaved: (recipe: Recipe) => void;
};

type Row = {
  key: string;
  materialId: string;
  qty: string;
  waste: string;
};

function trimDecimal(value: string): string {
  return value.includes(".")
    ? value.replace(/0+$/, "").replace(/\.$/, "")
    : value;
}

function recipeError(t: Dictionary, cause: unknown): string {
  if (cause instanceof ApiError) {
    switch (cause.code) {
      case "invalid_yield":
        return t.products.errors.invalidYield;
      case "invalid_labor_minutes":
        return t.products.errors.invalidLabor;
      case "recipe_items_required":
        return t.products.errors.itemsRequired;
      case "invalid_qty":
        return t.products.errors.invalidQty;
      case "invalid_waste":
        return t.products.errors.invalidWaste;
      case "duplicate_material":
        return t.products.errors.duplicateMaterial;
      case "invalid_material":
        return t.products.errors.invalidMaterial;
      case "permission_denied":
        return t.products.errors.forbidden;
    }
  }

  return t.products.errors.generic;
}

export function RecipeDialog({
  product,
  materials,
  initial,
  onClose,
  onSaved,
}: Props) {
  const { t } = useI18n();
  const nextKey = useRef(initial?.items.length ?? 1);

  const [yieldQty, setYieldQty] = useState(
    initial ? trimDecimal(initial.yield_qty) : "",
  );
  const [laborMinutes, setLaborMinutes] = useState(
    initial ? String(initial.labor_minutes) : "0",
  );
  const [rows, setRows] = useState<Row[]>(() => {
    if (!initial || initial.items.length === 0) {
      return [{ key: "row-0", materialId: "", qty: "", waste: "" }];
    }

    return initial.items.map((item, index) => ({
      key: `row-${index}`,
      materialId: item.material_id,
      qty: trimDecimal(item.qty),
      waste: trimDecimal(item.waste_pct),
    }));
  });
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  function updateRow(key: string, patch: Partial<Row>) {
    setRows((current) =>
      current.map((row) => (row.key === key ? { ...row, ...patch } : row)),
    );
  }

  function addRow() {
    const key = `row-${nextKey.current}`;
    nextKey.current += 1;
    setRows((current) => [
      ...current,
      { key, materialId: "", qty: "", waste: "" },
    ]);
  }

  function removeRow(key: string) {
    setRows((current) => current.filter((row) => row.key !== key));
  }

  function unitOf(materialId: string): string {
    return materials.find((material) => material.id === materialId)?.base_unit
      .code ?? "";
  }

  async function submit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError(null);

    const token = readToken();

    if (!token) {
      onClose();
      return;
    }

    const yieldValue = normalizeDecimal(yieldQty);

    if (!yieldValue || Number(yieldValue) <= 0) {
      setError(t.products.errors.invalidYield);
      return;
    }

    const labor = laborMinutes.trim() === "" ? 0 : Number(laborMinutes);

    if (!Number.isInteger(labor) || labor < 0) {
      setError(t.products.errors.invalidLabor);
      return;
    }

    if (rows.length === 0) {
      setError(t.products.errors.itemsRequired);
      return;
    }

    const seen = new Set<string>();
    const items = [];

    for (const row of rows) {
      if (!row.materialId) {
        setError(t.products.errors.itemRequired);
        return;
      }

      if (!seen.add(row.materialId)) {
        setError(t.products.errors.duplicateMaterial);
        return;
      }

      const qty = normalizeDecimal(row.qty);

      if (!qty || Number(qty) <= 0) {
        setError(t.products.errors.invalidQty);
        return;
      }

      const waste =
        row.waste.trim() === "" ? "0" : normalizeDecimal(row.waste);

      if (waste === null || Number(waste) > 100) {
        setError(t.products.errors.invalidWaste);
        return;
      }

      items.push({
        material_id: row.materialId,
        qty,
        waste_pct: waste,
      });
    }

    setSaving(true);

    try {
      const saved = await api.createRecipe(token, product.id, {
        yield_qty: yieldValue,
        labor_minutes: labor,
        items,
      });

      onSaved(saved);
    } catch (cause) {
      setError(recipeError(t, cause));

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
      wide
      title={t.products.recipeForm.createTitle}
      subtitle={t.products.recipeForm.subtitle}
      closeLabel={t.products.recipeForm.cancel}
      onClose={onClose}
    >
      {materials.length === 0 ? (
        <p className="modal__empty">{t.products.recipeForm.noMaterials}</p>
      ) : (
        <form className="form" onSubmit={submit}>
          <div className="form__grid">
            <label className="field">
              <span className="field__label">
                {t.products.recipeForm.yield}
              </span>
              <input
                className="field__input"
                type="text"
                inputMode="decimal"
                autoComplete="off"
                placeholder="0"
                value={yieldQty}
                onChange={(event) => setYieldQty(event.target.value)}
                required
              />
              <span className="field__hint">
                {t.products.recipeForm.yieldHint}
              </span>
            </label>

            <label className="field">
              <span className="field__label">
                {t.products.recipeForm.labor}
              </span>
              <input
                className="field__input"
                type="number"
                min="0"
                step="1"
                value={laborMinutes}
                onChange={(event) => setLaborMinutes(event.target.value)}
              />
              <span className="field__hint">
                {t.products.recipeForm.laborHint}
              </span>
            </label>
          </div>

          <div className="recipe-form">
            <span className="field__label">
              {t.products.recipeForm.items}
            </span>

            <div className="recipe-form__head">
              <span>{t.products.recipeForm.item}</span>
              <span>{t.products.recipeForm.qty}</span>
              <span>{t.products.recipeForm.waste}</span>
              <span aria-hidden="true" />
            </div>

            {rows.map((row) => (
              <div className="recipe-form__row" key={row.key}>
                <select
                  className="field__input"
                  value={row.materialId}
                  onChange={(event) =>
                    updateRow(row.key, { materialId: event.target.value })
                  }
                  aria-label={t.products.recipeForm.item}
                  required
                >
                  <option value="">
                    {t.products.recipeForm.itemPlaceholder}
                  </option>
                  {materials.map((material) => (
                    <option key={material.id} value={material.id}>
                      {material.name} ({material.base_unit.code})
                    </option>
                  ))}
                </select>

                <input
                  className="field__input"
                  type="text"
                  inputMode="decimal"
                  autoComplete="off"
                  placeholder={unitOf(row.materialId) || "0"}
                  value={row.qty}
                  onChange={(event) =>
                    updateRow(row.key, { qty: event.target.value })
                  }
                  aria-label={t.products.recipeForm.qty}
                  required
                />

                <input
                  className="field__input"
                  type="text"
                  inputMode="decimal"
                  autoComplete="off"
                  placeholder="0"
                  value={row.waste}
                  onChange={(event) =>
                    updateRow(row.key, { waste: event.target.value })
                  }
                  aria-label={t.products.recipeForm.waste}
                />

                <button
                  className="icon-btn"
                  type="button"
                  onClick={() => removeRow(row.key)}
                  aria-label={t.products.recipeForm.removeItem}
                  disabled={rows.length === 1}
                >
                  <Trash2 size={14} />
                </button>
              </div>
            ))}

            <button
              className="btn btn--ghost recipe-form__add"
              type="button"
              onClick={addRow}
            >
              <Plus size={14} />
              {t.products.recipeForm.addItem}
            </button>
          </div>

          {error && (
            <p className="form__error" role="alert">
              {error}
            </p>
          )}

          <div className="form__actions">
            <button className="btn btn--ghost" type="button" onClick={onClose}>
              {t.products.recipeForm.cancel}
            </button>
            <button className="btn btn--primary" type="submit" disabled={saving}>
              {saving
                ? t.products.recipeForm.saving
                : t.products.recipeForm.submit}
            </button>
          </div>
        </form>
      )}
    </Modal>
  );
}
