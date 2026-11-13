"use client";

import type { Material, Product, Recipe } from "@takar/api-client";
import { Boxes, ChevronDown, ChevronUp, Pencil, Plus } from "lucide-react";
import { useCallback, useEffect, useState } from "react";

import { LoadingRegion, SkeletonList } from "@/components/skeletons";
import { api } from "@/lib/api";
import { formatMoney, formatQty } from "@/lib/format";
import { fill, useI18n } from "@/lib/i18n";
import { readToken } from "@/lib/session";
import { useMe } from "@/lib/use-me";

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
        <main className="page">
          <p className="lede">{t.products.loading}</p>
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
        <AppShell me={session.me} active="products" onSignOut={signOut}>
          <div className="products">
            <header className="products__header">
              <div>
                <h1>{t.products.title}</h1>
                <p className="products__sub">
                  {fill(t.products.subtitle, { count: products.length })}
                </p>
              </div>
              {canWrite && (
                <button
                  className="btn btn--primary"
                  type="button"
                  onClick={() => setProductDialog({ mode: "create" })}
                >
                  <Plus size={15} />
                  {t.products.newProduct}
                </button>
              )}
            </header>

            {notice && (
              <p className="notice" role="status">
                {notice}
              </p>
            )}

            {load.status === "loading" && (
              <LoadingRegion label={t.products.loading}>
                <SkeletonList rows={4} />
              </LoadingRegion>
            )}

            {load.status === "error" && (
              <div className="card">
                <p>{t.products.error}</p>
                <button
                  className="btn btn--secondary"
                  type="button"
                  onClick={() => void fetchData()}
                >
                  {t.products.retry}
                </button>
              </div>
            )}

            {load.status === "ready" && products.length === 0 && (
              <div className="products__empty">
                <Boxes size={26} />
                <h2>{t.products.empty.title}</h2>
                <p>
                  {canWrite
                    ? t.products.empty.body
                    : t.products.empty.readOnly}
                </p>
                {canWrite && (
                  <button
                    className="btn btn--primary"
                    type="button"
                    onClick={() => setProductDialog({ mode: "create" })}
                  >
                    {t.products.empty.cta}
                  </button>
                )}
              </div>
            )}

            {load.status === "ready" && products.length > 0 && (
              <div className="products__list">
                {products.map((product) => {
                  const open = expanded === product.id;
                  const versions = recipes[product.id] ?? [];

                  return (
                    <article
                      className={`product${open ? " product--open" : ""}`}
                      key={product.id}
                    >
                      <div className="product__head">
                        <button
                          className="product__name"
                          type="button"
                          onClick={() => void toggle(product)}
                          aria-expanded={open}
                        >
                          <strong>{product.name}</strong>
                          <span>
                            {product.sku ?? t.products.card.noSku}
                          </span>
                        </button>

                        <div className="product__meta">
                          <span>{t.products.card.price}</span>
                          <strong>
                            {formatMoney(Number(product.sell_price), locale)}
                          </strong>
                        </div>

                        <div className="product__meta">
                          <span>{t.products.card.stock}</span>
                          <strong>{formatQty(product.stock_qty, locale)}</strong>
                        </div>

                        <div className="product__meta">
                          <span>{t.products.card.recipe}</span>
                          <strong>
                            {product.current_recipe_version !== null
                              ? fill(t.products.card.version, {
                                  version: product.current_recipe_version,
                                })
                              : t.products.card.noRecipe}
                          </strong>
                        </div>

                        <div className="product__actions">
                          {canWrite && (
                            <button
                              className="icon-btn"
                              type="button"
                              onClick={() =>
                                setProductDialog({ mode: "edit", product })
                              }
                              aria-label={t.products.card.edit}
                            >
                              <Pencil size={14} />
                            </button>
                          )}
                          <button
                            className="icon-btn"
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
                          </button>
                        </div>
                      </div>

                      {open && (
                        <div className="product__body">
                          <div className="product__body-head">
                            <h3>{t.products.recipe.history}</h3>
                            {canWrite && (
                              <button
                                className="btn btn--secondary"
                                type="button"
                                onClick={() => void openRecipe(product)}
                                disabled={materials.length === 0}
                              >
                                <Plus size={14} />
                                {product.current_recipe_id
                                  ? t.products.card.newVersion
                                  : t.products.card.createRecipe}
                              </button>
                            )}
                          </div>

                          {materials.length === 0 && (
                            <p className="product__hint">
                              {t.products.recipe.needsMaterials}
                            </p>
                          )}

                          {recipesLoading === product.id && (
                            <p className="product__hint">
                              {t.products.loading}
                            </p>
                          )}

                          {recipesLoading !== product.id &&
                            versions.length === 0 && (
                              <div className="product__no-recipe">
                                <p>
                                  <strong>{t.products.recipe.noRecipe}</strong>{" "}
                                  {t.products.recipe.noRecipeBody}
                                </p>
                                {canWrite && materials.length > 0 && (
                                  <button
                                    className="btn btn--secondary"
                                    type="button"
                                    onClick={() => void openRecipe(product)}
                                  >
                                    {t.products.card.createRecipe}
                                  </button>
                                )}
                              </div>
                            )}

                          {recipesLoading !== product.id &&
                            versions.map((recipe) => (
                              <div
                                className={`recipe${
                                  recipe.is_active ? " recipe--active" : ""
                                }`}
                                key={recipe.id}
                              >
                                <div className="recipe__head">
                                  <div className="recipe__version">
                                    <strong>
                                      {fill(t.products.card.version, {
                                        version: recipe.version,
                                      })}
                                    </strong>
                                    <span
                                      className={`pill pill--${
                                        recipe.is_active ? "safe" : "neutral"
                                      }`}
                                    >
                                      {recipe.is_active
                                        ? t.products.recipe.active
                                        : t.products.recipe.archived}
                                    </span>
                                  </div>
                                  <span>
                                    {fill(t.products.recipe.yield, {
                                      qty: formatQty(recipe.yield_qty, locale),
                                    })}
                                  </span>
                                  <span>
                                    {fill(t.products.recipe.labor, {
                                      minutes: recipe.labor_minutes,
                                    })}
                                  </span>
                                </div>

                                <div className="recipe__items">
                                  <div className="recipe__items-head">
                                    <span>
                                      {t.products.recipe.ingredient}
                                    </span>
                                    <span>{t.products.recipe.quantity}</span>
                                    <span>{t.products.recipe.waste}</span>
                                  </div>
                                  {recipe.items.map((item) => (
                                    <div
                                      className="recipe__item"
                                      key={item.material_id}
                                    >
                                      <span>{item.material_name}</span>
                                      <span>
                                        {formatQty(item.qty, locale)}{" "}
                                        {item.unit_code}
                                      </span>
                                      <span>
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
