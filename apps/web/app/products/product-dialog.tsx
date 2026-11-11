"use client";

import { ApiError, type Product } from "@takar/api-client";
import { useState } from "react";

import { Modal } from "@/components/modal";
import { api } from "@/lib/api";
import { normalizeDecimal } from "@/lib/format";
import { useI18n, type Dictionary } from "@/lib/i18n";
import { readToken } from "@/lib/session";

type Props = {
  /** Present when editing an existing product. */
  product?: Product;
  onClose: () => void;
  onSaved: (product: Product) => void;
};

function productError(t: Dictionary, cause: unknown): string {
  if (cause instanceof ApiError) {
    switch (cause.code) {
      case "name_required":
        return t.products.errors.nameRequired;
      case "invalid_sell_price":
        return t.products.errors.invalidPrice;
      case "product_exists":
        return t.products.errors.productExists;
      case "permission_denied":
        return t.products.errors.forbidden;
    }
  }

  return t.products.errors.generic;
}

export function ProductDialog({ product, onClose, onSaved }: Props) {
  const { t } = useI18n();
  const [name, setName] = useState(product?.name ?? "");
  const [sku, setSku] = useState(product?.sku ?? "");
  const [price, setPrice] = useState(product?.sell_price ?? "");
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

    if (!name.trim()) {
      setError(t.products.errors.nameRequired);
      return;
    }

    const sellPrice = price.trim() === "" ? "0" : normalizeDecimal(price);

    if (sellPrice === null) {
      setError(t.products.errors.invalidPrice);
      return;
    }

    setSaving(true);

    try {
      const body = {
        name: name.trim(),
        sku: sku.trim() || null,
        sell_price: sellPrice,
      };

      const saved = product
        ? await api.updateProduct(token, product.id, body)
        : await api.createProduct(token, body);

      onSaved(saved);
    } catch (cause) {
      setError(productError(t, cause));

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
      title={product ? t.products.form.editTitle : t.products.form.createTitle}
      subtitle={t.products.form.subtitle}
      closeLabel={t.products.form.cancel}
      onClose={onClose}
    >
      <form className="form" onSubmit={submit}>
        <div className="form__grid">
          <label className="field form__wide">
            <span className="field__label">{t.products.form.name}</span>
            <input
              className="field__input"
              type="text"
              autoComplete="off"
              value={name}
              onChange={(event) => setName(event.target.value)}
              required
            />
          </label>

          <label className="field">
            <span className="field__label">{t.products.form.sku}</span>
            <input
              className="field__input"
              type="text"
              autoComplete="off"
              value={sku}
              onChange={(event) => setSku(event.target.value)}
            />
            <span className="field__hint">{t.products.form.skuHint}</span>
          </label>

          <label className="field">
            <span className="field__label">{t.products.form.price}</span>
            <input
              className="field__input"
              type="text"
              inputMode="decimal"
              autoComplete="off"
              placeholder="0"
              value={price}
              onChange={(event) => setPrice(event.target.value)}
            />
            <span className="field__hint">{t.products.form.priceHint}</span>
          </label>
        </div>

        {error && (
          <p className="form__error" role="alert">
            {error}
          </p>
        )}

        <div className="form__actions">
          <button className="btn btn--ghost" type="button" onClick={onClose}>
            {t.products.form.cancel}
          </button>
          <button className="btn btn--primary" type="submit" disabled={saving}>
            {saving
              ? t.products.form.saving
              : product
                ? t.products.form.save
                : t.products.form.create}
          </button>
        </div>
      </form>
    </Modal>
  );
}
