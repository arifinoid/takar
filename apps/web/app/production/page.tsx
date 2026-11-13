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

import {
  LoadingRegion,
  SkeletonForm,
  SkeletonStats,
} from "@/components/skeletons";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select } from "@/components/ui/select";
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
import { cn } from "@/lib/utils";

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
          labor: preview.labor_cost,
          laborMinutes: preview.labor_minutes,
          laborRate: preview.labor_rate_per_hour,
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
        <main className="mx-auto max-w-[1100px] p-6">
          <LoadingRegion
            label={t.production.loading}
            className="flex flex-col gap-4"
          >
            <SkeletonForm fields={3} />
            <SkeletonStats count={3} />
          </LoadingRegion>
        </main>
      )}

      {session.status === "error" && (
        <main className="mx-auto max-w-[1100px] p-6">
          <div className="flex flex-col gap-3 rounded-xl border border-border bg-card p-5">
            <p className="text-sm text-muted-foreground">{t.dashboard.error}</p>
            <Button
              variant="secondary"
              size="sm"
              className="self-start"
              type="button"
              onClick={() => void reloadSession()}
            >
              {t.dashboard.retry}
            </Button>
          </div>
        </main>
      )}

      {session.status === "ready" && (
        <AppShell me={session.me} active="production" onSignOut={signOut}>
          <div className="flex max-w-[1100px] flex-col gap-4">
            <header>
              <h1 className="text-xl font-extrabold tracking-tight sm:text-3xl">
                {t.production.title}
              </h1>
              <p className="mt-1 text-sm text-muted-foreground">
                {t.production.subtitle}
              </p>
            </header>

            {load.status === "loading" && (
              <LoadingRegion
                label={t.production.loading}
                className="flex flex-col gap-4"
              >
                <SkeletonForm fields={3} />
                <SkeletonStats count={3} />
              </LoadingRegion>
            )}

            {load.status === "error" && (
              <div className="flex flex-col gap-3 rounded-xl border border-border bg-card p-5">
                <p className="text-sm text-muted-foreground">
                  {t.production.error}
                </p>
                <Button
                  variant="secondary"
                  size="sm"
                  className="self-start"
                  type="button"
                  onClick={() => void fetchProducts()}
                >
                  {t.production.retry}
                </Button>
              </div>
            )}

            {load.status === "ready" && producible.length === 0 && (
              <div className="flex flex-col items-center gap-2 rounded-xl border border-border bg-card px-5 py-10 text-center">
                <ChefHat size={26} className="text-brand" />
                <h2 className="text-lg font-bold">{t.production.empty.title}</h2>
                <p className="max-w-[46ch] text-sm leading-relaxed text-muted-foreground">
                  {t.production.empty.body}
                </p>
                <Button variant="secondary" asChild className="mt-1.5">
                  <Link href="/products">{t.production.empty.action}</Link>
                </Button>
              </div>
            )}

            {load.status === "ready" && producible.length > 0 && order && (
              <section className="flex flex-col items-center gap-2 rounded-xl border border-border bg-card px-5 py-9 text-center">
                <span className="flex size-12 items-center justify-center rounded-full bg-brand-soft text-brand">
                  <CheckCircle2 size={24} />
                </span>
                <h2 className="mt-1 text-xl font-bold">
                  {t.production.result.title}
                </h2>
                <p className="text-sm text-muted-foreground">
                  {fill(t.production.result.body, {
                    qty: formatQty(order.actual_qty, locale),
                    product: order.product_name,
                  })}
                </p>

                <div className="mt-3.5 mb-1 grid w-full max-w-[620px] gap-2.5 sm:grid-cols-3">
                  <ResultStat label={t.production.result.hpp}>
                    {formatMoney(Number(order.unit_cost), locale)}
                  </ResultStat>
                  <ResultStat label={t.production.result.materialCost}>
                    {formatMoney(Number(order.material_cost), locale)}
                  </ResultStat>
                  <ResultStat
                    label={t.production.result.margin}
                    tone={resultMargin !== null && resultMargin < 0 ? "danger" : undefined}
                  >
                    {resultMargin === null
                      ? t.production.result.noPrice
                      : formatMoney(resultMargin, locale)}
                  </ResultStat>
                </div>

                <div className="mt-2.5 flex w-full max-w-[620px] flex-col items-center gap-3.5 sm:flex-row sm:justify-between">
                  <span className="text-2xs text-ink-3">
                    {formatDate(order.produced_on, locale)} ·{" "}
                    {fill(t.production.result.recipe, {
                      version: order.recipe_version,
                    })}
                  </span>
                  <Button type="button" onClick={resetRun}>
                    {t.production.result.newRun}
                  </Button>
                </div>
              </section>
            )}

            {load.status === "ready" && producible.length > 0 && !order && (
              <>
                <form
                  className="flex flex-col gap-3.5 rounded-xl border border-border bg-card p-4.5"
                  onSubmit={runPreview}
                >
                  <div className="grid gap-3.5 sm:grid-cols-[1.2fr_0.8fr_0.8fr]">
                    <div className="flex flex-col gap-1.5">
                      <Label htmlFor="production-product">
                        {t.production.form.product}
                      </Label>
                      <Select
                        id="production-product"
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
                      </Select>
                    </div>

                    <div className="flex flex-col gap-1.5">
                      <Label htmlFor="production-qty">
                        {t.production.form.qty}
                      </Label>
                      <Input
                        id="production-qty"
                        type="text"
                        inputMode="decimal"
                        autoComplete="off"
                        placeholder="0"
                        value={qty}
                        onChange={(event) => changeQty(event.target.value)}
                        required
                      />
                      <span className="text-2xs leading-snug text-ink-3">
                        {t.production.form.qtyHint}
                      </span>
                    </div>

                    <div className="flex flex-col gap-1.5">
                      <Label htmlFor="production-date">
                        {t.production.form.producedOn}
                      </Label>
                      <Input
                        id="production-date"
                        type="date"
                        value={producedOn}
                        onChange={(event) => setProducedOn(event.target.value)}
                        required
                      />
                    </div>
                  </div>

                  <div className="flex justify-end">
                    <Button type="submit" disabled={busy !== null}>
                      {busy === "preview"
                        ? t.production.form.previewing
                        : t.production.form.preview}
                    </Button>
                  </div>
                </form>

                {error && (
                  <p
                    role="alert"
                    className="rounded-lg bg-danger-soft px-3 py-2.5 text-xs text-destructive"
                  >
                    {error}
                  </p>
                )}

                {!preview && !error && (
                  <p className="text-xs text-ink-3">{t.production.form.hint}</p>
                )}

                {preview && (
                  <section className="flex flex-col gap-4 rounded-xl border border-border bg-card p-4.5">
                    <header className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
                      <div>
                        <h2 className="text-lg font-bold">
                          {t.production.plan.title}
                        </h2>
                        <p className="mt-0.5 text-xs text-ink-3">
                          {fill(t.production.plan.subtitle, {
                            version: preview.recipe_version,
                            yield: formatQty(preview.yield_qty, locale),
                          })}
                        </p>
                      </div>
                      <Badge variant="brand" className="px-2.5 py-1 text-xs">
                        {formatQty(preview.qty, locale)}{" "}
                        {t.production.plan.units}
                      </Badge>
                    </header>

                    {cost && (
                      <div className="grid grid-cols-2 gap-2.5 sm:grid-cols-3 lg:grid-cols-5">
                        <CostTile label={t.production.plan.hpp} primary>
                          {formatMoney(Number(cost.unit), locale)}
                        </CostTile>
                        <CostTile label={t.production.plan.sellPrice}>
                          {formatMoney(Number(cost.sellPrice), locale)}
                        </CostTile>
                        <CostTile
                          label={t.production.plan.margin}
                          tone={Number(cost.margin) < 0 ? "danger" : undefined}
                          note={
                            cost.marginPct
                              ? fill(t.production.plan.marginPct, {
                                  pct: formatPercent(cost.marginPct, locale),
                                })
                              : undefined
                          }
                        >
                          {formatMoney(Number(cost.margin), locale)}
                        </CostTile>
                        <CostTile label={t.production.plan.materialCost}>
                          {formatMoney(Number(cost.material), locale)}
                        </CostTile>
                        <CostTile
                          label={t.production.plan.laborCost}
                          note={
                            Number(cost.labor) > 0
                              ? fill(t.production.plan.laborDetail, {
                                  minutes: cost.laborMinutes,
                                  rate: formatMoney(
                                    Number(cost.laborRate),
                                    locale,
                                  ),
                                })
                              : t.production.plan.noLaborRate
                          }
                        >
                          {formatMoney(Number(cost.labor), locale)}
                        </CostTile>
                      </div>
                    )}

                    <div
                      className={cn(
                        "flex items-start gap-2.5 rounded-lg px-3 py-2.5 text-xs",
                        preview.can_produce
                          ? "bg-brand-soft text-brand"
                          : "bg-danger-soft text-destructive",
                      )}
                    >
                      {preview.can_produce ? (
                        <CheckCircle2 size={17} className="mt-px shrink-0" />
                      ) : (
                        <AlertTriangle size={17} className="mt-px shrink-0" />
                      )}
                      <div>
                        <strong className="block text-xs font-bold">
                          {preview.can_produce
                            ? t.production.plan.readyTitle
                            : t.production.plan.shortageTitle}
                        </strong>
                        <span className="text-muted-foreground">
                          {preview.can_produce
                            ? t.production.plan.readyBody
                            : t.production.plan.shortageBody}
                        </span>
                      </div>
                    </div>

                    <div className="flex flex-col gap-2.5">
                      {preview.materials.map((material) => {
                        const short = Number(material.shortfall) > 0;

                        return (
                          <div
                            className="overflow-hidden rounded-lg border border-border"
                            key={material.material_id}
                          >
                            <div className="flex flex-col items-start gap-3 px-3 py-2.5 sm:flex-row sm:items-center sm:justify-between">
                              <div className="flex items-baseline gap-1.5">
                                <strong className="text-sm font-semibold">
                                  {material.material_name}
                                </strong>
                                <span className="text-2xs text-ink-3">
                                  {material.unit_code}
                                </span>
                              </div>
                              <div className="flex w-full flex-wrap gap-3.5 sm:w-auto sm:justify-end sm:gap-4.5">
                                <PlanNumber
                                  label={t.production.plan.required}
                                  value={formatQty(material.required, locale)}
                                />
                                <PlanNumber
                                  label={t.production.plan.available}
                                  value={formatQty(material.available, locale)}
                                />
                                <PlanNumber
                                  label={t.production.plan.shortfall}
                                  value={
                                    short
                                      ? formatQty(material.shortfall, locale)
                                      : "—"
                                  }
                                  tone={short ? "danger" : undefined}
                                />
                              </div>
                            </div>

                            {material.batches.length > 0 && (
                              <ul className="list-none border-t border-dashed border-border px-3 py-2">
                                {material.batches.map((batch) => (
                                  <li
                                    className="flex items-center justify-between gap-2.5 py-0.5 text-2xs text-ink-3"
                                    key={batch.batch_id}
                                  >
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
                                    <strong className="font-semibold text-ink-2">
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

                    <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-end">
                      {!canWrite && (
                        <span className="text-xs text-ink-3 sm:mr-auto">
                          {t.production.plan.readOnly}
                        </span>
                      )}
                      <Button
                        type="button"
                        onClick={() => void confirm()}
                        disabled={
                          !preview.can_produce || !canWrite || busy !== null
                        }
                      >
                        {busy === "save"
                          ? t.production.form.confirming
                          : t.production.form.confirm}
                      </Button>
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

function CostTile({
  label,
  note,
  tone,
  primary,
  children,
}: {
  label: string;
  note?: string;
  tone?: "danger";
  primary?: boolean;
  children: React.ReactNode;
}) {
  return (
    <div
      className={cn(
        "flex flex-col gap-0.5 rounded-lg border px-3 py-3",
        primary
          ? "border-[#c8e2d6] bg-brand-soft"
          : "border-border bg-surface-alt",
      )}
    >
      <span className="text-2xs font-semibold tracking-wide text-ink-3 uppercase">
        {label}
      </span>
      <strong
        className={cn(
          "text-base font-bold tabular-nums",
          primary && "text-brand",
          tone === "danger" && "text-destructive",
        )}
      >
        {children}
      </strong>
      {note && <em className="text-2xs font-normal text-ink-2">{note}</em>}
    </div>
  );
}

function PlanNumber({
  label,
  value,
  tone,
}: {
  label: string;
  value: string;
  tone?: "danger";
}) {
  return (
    <div className="flex flex-col gap-px text-left sm:text-right">
      <span className="text-2xs font-semibold tracking-wide text-ink-3">
        {label}
      </span>
      <strong
        className={cn(
          "text-xs font-bold tabular-nums",
          tone === "danger" && "text-destructive",
        )}
      >
        {value}
      </strong>
    </div>
  );
}

function ResultStat({
  label,
  tone,
  children,
}: {
  label: string;
  tone?: "danger";
  children: React.ReactNode;
}) {
  return (
    <div className="flex flex-col gap-0.5 rounded-lg border border-border bg-surface-alt p-3">
      <span className="text-2xs font-semibold text-ink-3 uppercase">
        {label}
      </span>
      <strong
        className={cn(
          "text-base font-bold tabular-nums",
          tone === "danger" && "text-destructive",
        )}
      >
        {children}
      </strong>
    </div>
  );
}
