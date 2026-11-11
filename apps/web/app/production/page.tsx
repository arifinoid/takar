"use client";

import {
  ApiError,
  type Product,
  type ProductionOrder,
  type ProductionPreview,
} from "@takar/api-client";
import { AlertTriangle, CheckCircle2, ChefHat } from "lucide-react";
import Link from "next/link";
import { useCallback, useEffect, useMemo, useState } from "react";

import { api } from "@/lib/api";
import {
  formatDate,
  formatMoney,
  formatPercent,
  formatQty,
  normalizeDecimal,
  todayIso,
} from "@/lib/format";
import { fill, useI18n, type Dictionary } from "@/lib/i18n";
import { readToken } from "@/lib/session";
import { useMe } from "@/lib/use-me";

import { AppShell } from "../app-shell";

type Load =
  | { status: "loading" }
  | { status: "ready"; products: Product[] }
  | { status: "error" };

function productionError(t: Dictionary, cause: unknown): string {
  if (cause instanceof ApiError) {
    switch (cause.code) {
      case "invalid_quantity":
        return t.production.errors.invalidQuantity;
      case "recipe_required":
      case "recipe_items_required":
        return t.production.errors.recipeRequired;
      case "insufficient_stock":
        return t.production.errors.insufficientStock;
      case "product_not_found":
        return t.production.errors.productNotFound;
      case "permission_denied":
        return t.production.errors.forbidden;
    }
  }

  return t.production.errors.generic;
}

export default function ProductionPage() {
  const { t, locale } = useI18n();
  const { state: session, reload: reloadSession, signOut } = useMe();

  const [load, setLoad] = useState<Load>({ status: "loading" });
  const [productId, setProductId] = useState("");
  const [qty, setQty] = useState("");
  const [producedOn, setProducedOn] = useState(todayIso());
  const [preview, setPreview] = useState<ProductionPreview | null>(null);
  const [order, setOrder] = useState<ProductionOrder | null>(null);
  const [busy, setBusy] = useState<"preview" | "save" | null>(null);
  const [error, setError] = useState<string | null>(null);

  const fetchProducts = useCallback(async () => {
    const token = readToken();

    if (!token) return;

    setLoad((current) =>
      current.status === "ready" ? current : { status: "loading" },
    );

    try {
      const products = await api.listProducts(token);
      setLoad({ status: "ready", products });
    } catch {
      setLoad({ status: "error" });
    }
  }, []);

  useEffect(() => {
    if (session.status === "ready") {
      void fetchProducts();
    }
  }, [session.status, fetchProducts]);

  const products = load.status === "ready" ? load.products : [];

  const producible = useMemo(
    () => products.filter((product) => product.current_recipe_id !== null),
    [products],
  );

  const selected =
    producible.find((product) => product.id === productId) ?? null;

  const permissions = session.status === "ready" ? session.me.permissions : [];
  const canWrite = permissions.includes("production_write");

  function resetRun() {
    setQty("");
    setPreview(null);
    setOrder(null);
    setError(null);
  }

  function chooseProduct(id: string) {
    setProductId(id);
    setPreview(null);
    setOrder(null);
    setError(null);
  }

  function changeQty(value: string) {
    setQty(value);
    setPreview(null);
    setOrder(null);
    setError(null);
  }

  async function runPreview(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError(null);

    const token = readToken();

    if (!token) return;

    const quantity = normalizeDecimal(qty);

    if (!quantity || Number(quantity) <= 0) {
      setError(t.production.errors.invalidQuantity);
      return;
    }

    if (!productId) {
      setError(t.production.errors.productRequired);
      return;
    }

    setBusy("preview");

    try {
      const result = await api.previewProduction(token, {
        product_id: productId,
        qty: quantity,
      });
      setPreview(result);
    } catch (cause) {
      setError(productionError(t, cause));
    } finally {
      setBusy(null);
    }
  }

  async function confirm() {
    if (!preview || !preview.can_produce || !canWrite) return;

    const token = readToken();

    if (!token) return;

    setBusy("save");
    setError(null);

    try {
      const result = await api.createProduction(token, {
        product_id: preview.product_id,
        qty: preview.qty,
        produced_on: producedOn,
      });

      setPreview(null);
      setOrder(result);
      void fetchProducts();
    } catch (cause) {
      setError(productionError(t, cause));
      // Stock changed underneath the preview; make the user look again.
      setPreview(null);
    } finally {
      setBusy(null);
    }
  }

  const cost =
    preview &&
    preview.can_produce &&
    preview.unit_cost !== null &&
    preview.material_cost !== null &&
    preview.margin !== null
      ? {
          unit: preview.unit_cost,
          material: preview.material_cost,
          margin: preview.margin,
          marginPct: preview.margin_pct,
          sellPrice: preview.sell_price,
        }
      : null;

  const resultMargin =
    selected && order ? Number(selected.sell_price) - Number(order.unit_cost) : null;

  return (
    <>
      {session.status === "loading" && (
        <main className="page">
          <p className="lede">{t.production.loading}</p>
        </main>
      )}

      {session.status === "error" && (
        <main className="page">
          <div className="card">
            <p>{t.dashboard.error}</p>
            <button
              className="btn btn--secondary"
              type="button"
              onClick={() => void reloadSession()}
            >
              {t.dashboard.retry}
            </button>
          </div>
        </main>
      )}

      {session.status === "ready" && (
        <AppShell me={session.me} active="production" onSignOut={signOut}>
          <div className="production">
            <header className="production__header">
              <div>
                <h1>{t.production.title}</h1>
                <p className="production__sub">{t.production.subtitle}</p>
              </div>
            </header>

            {load.status === "loading" && (
              <p className="lede">{t.production.loading}</p>
            )}

            {load.status === "error" && (
              <div className="card">
                <p>{t.production.error}</p>
                <button
                  className="btn btn--secondary"
                  type="button"
                  onClick={() => void fetchProducts()}
                >
                  {t.production.retry}
                </button>
              </div>
            )}

            {load.status === "ready" && producible.length === 0 && (
              <div className="production__empty">
                <ChefHat size={26} />
                <h2>{t.production.empty.title}</h2>
                <p>{t.production.empty.body}</p>
                <Link className="btn btn--secondary" href="/products">
                  {t.production.empty.action}
                </Link>
              </div>
            )}

            {load.status === "ready" && producible.length > 0 && order && (
              <section className="production__result">
                <span className="production__result-icon">
                  <CheckCircle2 size={24} />
                </span>
                <h2>{t.production.result.title}</h2>
                <p>
                  {fill(t.production.result.body, {
                    qty: formatQty(order.actual_qty, locale),
                    product: order.product_name,
                  })}
                </p>

                <div className="production__result-grid">
                  <div>
                    <span>{t.production.result.hpp}</span>
                    <strong>{formatMoney(Number(order.unit_cost), locale)}</strong>
                  </div>
                  <div>
                    <span>{t.production.result.materialCost}</span>
                    <strong>
                      {formatMoney(Number(order.material_cost), locale)}
                    </strong>
                  </div>
                  <div>
                    <span>{t.production.result.margin}</span>
                    <strong
                      className={
                        resultMargin !== null && resultMargin < 0
                          ? "is-danger"
                          : undefined
                      }
                    >
                      {resultMargin === null
                        ? t.production.result.noPrice
                        : formatMoney(resultMargin, locale)}
                    </strong>
                  </div>
                </div>

                <div className="production__result-foot">
                  <span>
                    {formatDate(order.produced_on, locale)} ·{" "}
                    {fill(t.production.result.recipe, {
                      version: order.recipe_version,
                    })}
                  </span>
                  <button
                    className="btn btn--primary"
                    type="button"
                    onClick={resetRun}
                  >
                    {t.production.result.newRun}
                  </button>
                </div>
              </section>
            )}

            {load.status === "ready" && producible.length > 0 && !order && (
              <>
                <form className="production__card" onSubmit={runPreview}>
                  <div className="production__form-grid">
                    <label className="field">
                      <span className="field__label">
                        {t.production.form.product}
                      </span>
                      <select
                        className="field__input"
                        value={productId}
                        onChange={(event) => chooseProduct(event.target.value)}
                        required
                      >
                        <option value="">
                          {t.production.form.productPlaceholder}
                        </option>
                        {producible.map((product) => (
                          <option key={product.id} value={product.id}>
                            {product.name} · v{product.current_recipe_version}
                          </option>
                        ))}
                      </select>
                    </label>

                    <label className="field">
                      <span className="field__label">{t.production.form.qty}</span>
                      <input
                        className="field__input"
                        type="text"
                        inputMode="decimal"
                        autoComplete="off"
                        placeholder="0"
                        value={qty}
                        onChange={(event) => changeQty(event.target.value)}
                        required
                      />
                      <span className="field__hint">
                        {t.production.form.qtyHint}
                      </span>
                    </label>

                    <label className="field">
                      <span className="field__label">
                        {t.production.form.producedOn}
                      </span>
                      <input
                        className="field__input"
                        type="date"
                        value={producedOn}
                        onChange={(event) => setProducedOn(event.target.value)}
                        required
                      />
                    </label>
                  </div>

                  <div className="production__form-actions">
                    <button
                      className="btn btn--primary"
                      type="submit"
                      disabled={busy !== null}
                    >
                      {busy === "preview"
                        ? t.production.form.previewing
                        : t.production.form.preview}
                    </button>
                  </div>
                </form>

                {error && (
                  <p className="form__error" role="alert">
                    {error}
                  </p>
                )}

                {!preview && !error && (
                  <p className="production__hint">{t.production.form.hint}</p>
                )}

                {preview && (
                  <section className="production__card production__plan">
                    <header className="production__plan-head">
                      <div>
                        <h2>{t.production.plan.title}</h2>
                        <p>
                          {fill(t.production.plan.subtitle, {
                            version: preview.recipe_version,
                            yield: formatQty(preview.yield_qty, locale),
                          })}
                        </p>
                      </div>
                      <span className="production__plan-qty">
                        {formatQty(preview.qty, locale)}{" "}
                        {t.production.plan.units}
                      </span>
                    </header>

                    {cost && (
                      <div className="costs">
                        <div className="cost cost--primary">
                          <span>{t.production.plan.hpp}</span>
                          <strong>{formatMoney(Number(cost.unit), locale)}</strong>
                        </div>
                        <div className="cost">
                          <span>{t.production.plan.sellPrice}</span>
                          <strong>
                            {formatMoney(Number(cost.sellPrice), locale)}
                          </strong>
                        </div>
                        <div className="cost">
                          <span>{t.production.plan.margin}</span>
                          <strong
                            className={
                              Number(cost.margin) < 0 ? "is-danger" : undefined
                            }
                          >
                            {formatMoney(Number(cost.margin), locale)}
                          </strong>
                          {cost.marginPct && (
                            <em>
                              {fill(t.production.plan.marginPct, {
                                pct: formatPercent(cost.marginPct, locale),
                              })}
                            </em>
                          )}
                        </div>
                        <div className="cost">
                          <span>{t.production.plan.materialCost}</span>
                          <strong>
                            {formatMoney(Number(cost.material), locale)}
                          </strong>
                        </div>
                      </div>
                    )}

                    <div
                      className={`production__banner ${
                        preview.can_produce
                          ? "production__banner--ready"
                          : "production__banner--danger"
                      }`}
                    >
                      {preview.can_produce ? (
                        <CheckCircle2 size={17} />
                      ) : (
                        <AlertTriangle size={17} />
                      )}
                      <div>
                        <strong>
                          {preview.can_produce
                            ? t.production.plan.readyTitle
                            : t.production.plan.shortageTitle}
                        </strong>
                        <span>
                          {preview.can_produce
                            ? t.production.plan.readyBody
                            : t.production.plan.shortageBody}
                        </span>
                      </div>
                    </div>

                    <div className="plan">
                      {preview.materials.map((material) => {
                        const short = Number(material.shortfall) > 0;

                        return (
                          <div className="plan__item" key={material.material_id}>
                            <div className="plan__head">
                              <div className="plan__name">
                                <strong>{material.material_name}</strong>
                                <span>{material.unit_code}</span>
                              </div>
                              <div className="plan__numbers">
                                <div className="plan__number">
                                  <span>{t.production.plan.required}</span>
                                  <strong>
                                    {formatQty(material.required, locale)}
                                  </strong>
                                </div>
                                <div className="plan__number">
                                  <span>{t.production.plan.available}</span>
                                  <strong>
                                    {formatQty(material.available, locale)}
                                  </strong>
                                </div>
                                <div
                                  className={`plan__number${
                                    short ? " is-danger" : ""
                                  }`}
                                >
                                  <span>{t.production.plan.shortfall}</span>
                                  <strong>
                                    {short
                                      ? formatQty(material.shortfall, locale)
                                      : "—"}
                                  </strong>
                                </div>
                              </div>
                            </div>

                            {material.batches.length > 0 && (
                              <ul className="plan__batches">
                                {material.batches.map((batch) => (
                                  <li className="plan__batch" key={batch.batch_id}>
                                    <span>
                                      {batch.expires_at
                                        ? fill(t.production.plan.expires, {
                                            date: formatDate(
                                              batch.expires_at,
                                              locale,
                                            ),
                                          })
                                        : t.production.plan.noExpiry}
                                    </span>
                                    <strong>
                                      {formatQty(batch.qty, locale)}{" "}
                                      {material.unit_code} ·{" "}
                                      {formatMoney(
                                        Number(batch.unit_cost),
                                        locale,
                                      )}
                                      /{material.unit_code}
                                    </strong>
                                  </li>
                                ))}
                              </ul>
                            )}
                          </div>
                        );
                      })}
                    </div>

                    <div className="production__confirm">
                      {!canWrite && (
                        <span>{t.production.plan.readOnly}</span>
                      )}
                      <button
                        className="btn btn--primary"
                        type="button"
                        onClick={() => void confirm()}
                        disabled={
                          !preview.can_produce || !canWrite || busy !== null
                        }
                      >
                        {busy === "save"
                          ? t.production.form.confirming
                          : t.production.form.confirm}
                      </button>
                    </div>
                  </section>
                )}
              </>
            )}
          </div>
        </AppShell>
      )}
    </>
  );
}
