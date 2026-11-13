"use client";

import type { Material, Product, Recipe } from "@takar/api-client";
import { Boxes, ChevronDown, ChevronUp, Pencil, Plus } from "lucide-react";
import { useCallback, useEffect, useState } from "react";

import { LoadingRegion, SkeletonList } from "@/components/skeletons";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { api } from "@/lib/api";
import { formatMoney, formatQty } from "@/lib/format";
import { fill, useI18n } from "@/lib/i18n";
import { readToken } from "@/lib/session";
import { useMe } from "@/lib/use-me";
import { cn } from "@/lib/utils";

import { AppShell } from "../app-shell";
import { ProductDialog } from "./product-dialog";
import { RecipeDialog } from "./recipe-dialog";

type Load =
  | { status: "loading" }
  | { status: "ready"; products: Product[]; materials: Material[] }
  | { status: "error" };

type ProductDialogState =
  | { mode: "create" }
  | { mode: "edit"; product: Product }
  | null;

type RecipeTarget = { product: Product; initial?: Recipe } | null;

export default function ProductsPage() {
  const { t, locale } = useI18n();
  const { state: session, reload: reloadSession, signOut } = useMe();

  const [load, setLoad] = useState<Load>({ status: "loading" });
  const [expanded, setExpanded] = useState<string | null>(null);
  const [recipes, setRecipes] = useState<Record<string, Recipe[]>>({});
  const [recipesLoading, setRecipesLoading] = useState<string | null>(null);
  const [productDialog, setProductDialog] = useState<ProductDialogState>(null);
  const [recipeTarget, setRecipeTarget] = useState<RecipeTarget>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const fetchData = useCallback(async () => {
    const token = readToken();

    if (!token) return;

    setLoad((current) =>
      current.status === "ready" ? current : { status: "loading" },
    );

    try {
      const [products, materials] = await Promise.all([
        api.listProducts(token),
        api.listMaterials(token),
      ]);

      setLoad({ status: "ready", products, materials });
    } catch {
      setLoad({ status: "error" });
    }
  }, []);

  useEffect(() => {
    if (session.status === "ready") {
      void fetchData();
    }
  }, [session.status, fetchData]);

  useEffect(() => {
    if (!notice) return;

    const timer = window.setTimeout(() => setNotice(null), 5_000);
    return () => window.clearTimeout(timer);
  }, [notice]);

  const products = load.status === "ready" ? load.products : [];
  const materials = load.status === "ready" ? load.materials : [];
  const permissions = session.status === "ready" ? session.me.permissions : [];
  const canWrite = permissions.includes("recipe_write");

  const loadRecipes = useCallback(
    async (productId: string): Promise<Recipe[] | null> => {
      const token = readToken();

      if (!token) return null;

      setRecipesLoading(productId);

      try {
        const list = await api.listRecipes(token, productId);
        setRecipes((current) => ({ ...current, [productId]: list }));
        return list;
      } catch {
        setNotice(t.products.errors.generic);
        return null;
      } finally {
        setRecipesLoading(null);
      }
    },
    [t],
  );

  async function toggle(product: Product) {
    if (expanded === product.id) {
      setExpanded(null);
      return;
    }

    setExpanded(product.id);

    if (!recipes[product.id]) {
      await loadRecipes(product.id);
    }
  }

  async function openRecipe(product: Product) {
    const list = recipes[product.id] ?? (await loadRecipes(product.id)) ?? [];
    setRecipeTarget({
      product,
      initial: list.find((recipe) => recipe.is_active),
    });
  }

  async function handleProductSaved(product: Product) {
    setProductDialog(null);
    setNotice(t.products.saved);
    setExpanded(product.id);
    await fetchData();
  }

  async function handleRecipeSaved(recipe: Recipe) {
    setRecipeTarget(null);
    setNotice(t.products.recipe.saved);
    setExpanded(recipe.product_id);
    await fetchData();
    await loadRecipes(recipe.product_id);
  }

  return (
    <>
      {session.status === "loading" && (
        <main className="mx-auto max-w-[1100px] p-6">
          <LoadingRegion label={t.products.loading}>
            <SkeletonList rows={4} />
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
        <AppShell me={session.me} active="products" onSignOut={signOut}>
          <div className="flex max-w-[1100px] flex-col gap-4">
            <header className="flex flex-col items-start gap-4 sm:flex-row sm:items-end sm:justify-between">
              <div>
                <h1 className="text-xl font-extrabold tracking-tight sm:text-3xl">
                  {t.products.title}
                </h1>
                <p className="mt-1 text-sm text-muted-foreground">
                  {fill(t.products.subtitle, { count: products.length })}
                </p>
              </div>
              {canWrite && (
                <Button
                  type="button"
                  onClick={() => setProductDialog({ mode: "create" })}
                >
                  <Plus size={15} />
                  {t.products.newProduct}
                </Button>
              )}
            </header>

            {notice && (
              <p
                role="status"
                className="rounded-lg bg-brand-soft px-3 py-2.5 text-xs font-medium text-brand"
              >
                {notice}
              </p>
            )}

            {load.status === "loading" && (
              <LoadingRegion label={t.products.loading}>
                <SkeletonList rows={4} />
              </LoadingRegion>
            )}

            {load.status === "error" && (
              <div className="flex flex-col gap-3 rounded-xl border border-border bg-card p-5">
                <p className="text-sm text-muted-foreground">
                  {t.products.error}
                </p>
                <Button
                  variant="secondary"
                  size="sm"
                  className="self-start"
                  type="button"
                  onClick={() => void fetchData()}
                >
                  {t.products.retry}
                </Button>
              </div>
            )}

            {load.status === "ready" && products.length === 0 && (
              <div className="flex flex-col items-center gap-2 rounded-xl border border-border bg-card px-5 py-10 text-center">
                <Boxes size={26} className="text-brand" />
                <h2 className="text-lg font-bold">{t.products.empty.title}</h2>
                <p className="max-w-[46ch] text-sm leading-relaxed text-muted-foreground">
                  {canWrite
                    ? t.products.empty.body
                    : t.products.empty.readOnly}
                </p>
                {canWrite && (
                  <Button
                    className="mt-1.5"
                    type="button"
                    onClick={() => setProductDialog({ mode: "create" })}
                  >
                    {t.products.empty.cta}
                  </Button>
                )}
              </div>
            )}

            {load.status === "ready" && products.length > 0 && (
              <div className="flex flex-col gap-2.5">
                {products.map((product) => {
                  const open = expanded === product.id;
                  const versions = recipes[product.id] ?? [];

                  return (
                    <article
                      className={cn(
                        "overflow-hidden rounded-xl border bg-card",
                        open ? "border-brand" : "border-border",
                      )}
                      key={product.id}
                    >
                      <div className="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto] items-start gap-x-3.5 gap-y-2.5 px-4 py-3.5 sm:grid-cols-[minmax(0,1.6fr)_repeat(3,minmax(0,0.8fr))_auto] sm:items-center">
                        <button
                          className="col-span-2 flex min-w-0 cursor-pointer flex-col items-start gap-0.5 border-0 bg-transparent p-0 text-left sm:col-span-1"
                          type="button"
                          onClick={() => void toggle(product)}
                          aria-expanded={open}
                        >
                          <strong className="truncate text-sm font-bold text-ink">
                            {product.name}
                          </strong>
                          <span className="text-xs text-ink-3">
                            {product.sku ?? t.products.card.noSku}
                          </span>
                        </button>

                        <div className="flex min-w-0 flex-col gap-0.5">
                          <span className="text-2xs tracking-wide text-ink-3 uppercase">
                            {t.products.card.price}
                          </span>
                          <strong className="truncate text-sm font-semibold tabular-nums">
                            {formatMoney(Number(product.sell_price), locale)}
                          </strong>
                        </div>

                        <div className="flex min-w-0 flex-col gap-0.5">
                          <span className="text-2xs tracking-wide text-ink-3 uppercase">
                            {t.products.card.stock}
                          </span>
                          <strong className="truncate text-sm font-semibold tabular-nums">
                            {formatQty(product.stock_qty, locale)}
                          </strong>
                        </div>

                        <div className="flex min-w-0 flex-col gap-0.5">
                          <span className="text-2xs tracking-wide text-ink-3 uppercase">
                            {t.products.card.recipe}
                          </span>
                          <strong className="truncate text-sm font-semibold">
                            {product.current_recipe_version !== null
                              ? fill(t.products.card.version, {
                                  version: product.current_recipe_version,
                                })
                              : t.products.card.noRecipe}
                          </strong>
                        </div>

                        <div className="flex justify-self-end gap-1.5">
                          {canWrite && (
                            <Button
                              variant="ghost"
                              size="icon"
                              className="size-7 rounded-md"
                              type="button"
                              onClick={() =>
                                setProductDialog({ mode: "edit", product })
                              }
                              aria-label={t.products.card.edit}
                            >
                              <Pencil size={14} />
                            </Button>
                          )}
                          <Button
                            variant="ghost"
                            size="icon"
                            className="size-7 rounded-md"
                            type="button"
                            onClick={() => void toggle(product)}
                            aria-label={
                              open
                                ? t.products.card.collapse
                                : t.products.card.expand
                            }
                          >
                            {open ? (
                              <ChevronUp size={15} />
                            ) : (
                              <ChevronDown size={15} />
                            )}
                          </Button>
                        </div>
                      </div>

                      {open && (
                        <div className="flex flex-col gap-2.5 border-t border-border px-4 pt-3.5 pb-4">
                          <div className="flex items-center justify-between gap-3">
                            <h3 className="text-xs font-bold tracking-wide uppercase">
                              {t.products.recipe.history}
                            </h3>
                            {canWrite && (
                              <Button
                                variant="secondary"
                                size="sm"
                                type="button"
                                onClick={() => void openRecipe(product)}
                                disabled={materials.length === 0}
                              >
                                <Plus size={14} />
                                {product.current_recipe_id
                                  ? t.products.card.newVersion
                                  : t.products.card.createRecipe}
                              </Button>
                            )}
                          </div>

                          {materials.length === 0 && (
                            <p className="text-xs text-ink-3">
                              {t.products.recipe.needsMaterials}
                            </p>
                          )}

                          {recipesLoading === product.id && (
                            <p className="text-xs text-ink-3">
                              {t.products.loading}
                            </p>
                          )}

                          {recipesLoading !== product.id &&
                            versions.length === 0 && (
                              <div className="flex flex-wrap items-center justify-between gap-2.5 rounded-lg bg-surface-alt px-3.5 py-3">
                                <p className="text-xs leading-relaxed text-ink-2">
                                  <strong className="text-ink">
                                    {t.products.recipe.noRecipe}
                                  </strong>{" "}
                                  {t.products.recipe.noRecipeBody}
                                </p>
                                {canWrite && materials.length > 0 && (
                                  <Button
                                    variant="secondary"
                                    size="sm"
                                    type="button"
                                    onClick={() => void openRecipe(product)}
                                  >
                                    {t.products.card.createRecipe}
                                  </Button>
                                )}
                              </div>
                            )}

                          {recipesLoading !== product.id &&
                            versions.map((recipe) => (
                              <div
                                className={cn(
                                  "rounded-lg border px-3.5 py-3",
                                  recipe.is_active
                                    ? "border-brand"
                                    : "border-border",
                                )}
                                key={recipe.id}
                              >
                                <div className="flex flex-wrap items-center justify-between gap-3.5">
                                  <div className="flex items-center gap-2">
                                    <strong className="text-sm font-bold">
                                      {fill(t.products.card.version, {
                                        version: recipe.version,
                                      })}
                                    </strong>
                                    <Badge
                                      variant={
                                        recipe.is_active ? "brand" : "neutral"
                                      }
                                    >
                                      {recipe.is_active
                                        ? t.products.recipe.active
                                        : t.products.recipe.archived}
                                    </Badge>
                                  </div>
                                  <span className="text-xs text-ink-3">
                                    {fill(t.products.recipe.yield, {
                                      qty: formatQty(recipe.yield_qty, locale),
                                    })}
                                  </span>
                                  <span className="text-xs text-ink-3">
                                    {fill(t.products.recipe.labor, {
                                      minutes: recipe.labor_minutes,
                                    })}
                                  </span>
                                </div>

                                <div className="mt-2.5 flex flex-col gap-1">
                                  <div className="grid grid-cols-[minmax(0,1.6fr)_minmax(0,1fr)_minmax(0,0.6fr)] gap-2.5">
                                    <span className="text-2xs tracking-wide text-ink-3 uppercase">
                                      {t.products.recipe.ingredient}
                                    </span>
                                    <span className="text-2xs tracking-wide text-ink-3 uppercase">
                                      {t.products.recipe.quantity}
                                    </span>
                                    <span className="text-2xs tracking-wide text-ink-3 uppercase">
                                      {t.products.recipe.waste}
                                    </span>
                                  </div>
                                  {recipe.items.map((item) => (
                                    <div
                                      className="grid grid-cols-[minmax(0,1.6fr)_minmax(0,1fr)_minmax(0,0.6fr)] gap-2.5"
                                      key={item.material_id}
                                    >
                                      <span className="truncate text-xs tabular-nums">
                                        {item.material_name}
                                      </span>
                                      <span className="text-xs tabular-nums">
                                        {formatQty(item.qty, locale)}{" "}
                                        {item.unit_code}
                                      </span>
                                      <span className="text-xs tabular-nums">
                                        {formatQty(item.waste_pct, locale)}%
                                      </span>
                                    </div>
                                  ))}
                                </div>
                              </div>
                            ))}
                        </div>
                      )}
                    </article>
                  );
                })}
              </div>
            )}
          </div>

          {productDialog && (
            <ProductDialog
              product={
                productDialog.mode === "edit"
                  ? productDialog.product
                  : undefined
              }
              onClose={() => setProductDialog(null)}
              onSaved={(product) => void handleProductSaved(product)}
            />
          )}

          {recipeTarget && (
            <RecipeDialog
              product={recipeTarget.product}
              materials={materials}
              initial={recipeTarget.initial}
              onClose={() => setRecipeTarget(null)}
              onSaved={(recipe) => void handleRecipeSaved(recipe)}
            />
          )}
        </AppShell>
      )}
    </>
  );
}
