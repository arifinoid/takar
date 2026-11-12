"use client";

import {
  ApiError,
  type Product,
  type Sale,
  type SaleRequest,
} from "@takar/api-client";
import { Plus, Receipt, Trash2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import { api } from "@/lib/api";
import { formatDateTime, formatMoney, formatQty, normalizeDecimal } from "@/lib/format";
import { fill, useI18n, type Dictionary } from "@/lib/i18n";
import { readToken } from "@/lib/session";
import { useMe } from "@/lib/use-me";

import { AppShell } from "../app-shell";

type Load =
  | { status: "loading" }
  | { status: "ready"; products: Product[] }
  | { status: "error" };

type Line = {
  key: string;
  productId: string;
  qty: string;
  price: string;
};

type Shortage = {
  product_name: string;
  available: string;
  requested: string;
};

const CHANNELS = ["toko", "reseller", "online", "lainnya"] as const;

function emptyLine(): Line {
  return { key: newRef(), productId: "", qty: "1", price: "" };
}

/**
 * Idempotency key for one sale. It survives a retry, so a request that timed
 * out after the server committed cannot decrement stock twice.
 */
function newRef(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }

  return "xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx".replace(/[xy]/g, (char) => {
    const value = (Math.random() * 16) | 0;
    const digit = char === "x" ? value : (value & 0x3) | 0x8;
    return digit.toString(16);
  });
}

/** `datetime-local` wants a local wall clock, not an ISO instant. */
function localNow(): string {
  const now = new Date();
  const pad = (value: number) => String(value).padStart(2, "0");

  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}T${pad(now.getHours())}:${pad(now.getMinutes())}`;
}

function channelLabel(t: Dictionary, channel: string): string {
  switch (channel) {
    case "toko":
      return t.sales.channels.toko;
    case "reseller":
      return t.sales.channels.reseller;
    case "online":
      return t.sales.channels.online;
    case "lainnya":
      return t.sales.channels.lainnya;
    default:
      return channel;
  }
}

function shortageMessage(
  t: Dictionary,
  locale: "id" | "en",
  details: unknown,
): string | null {
  const shortages = (details as { shortages?: Shortage[] } | null)?.shortages;

  if (!Array.isArray(shortages) || shortages.length === 0) return null;

  const parts = shortages.map((shortage) =>
    fill(t.sales.errors.shortageItem, {
      product: shortage.product_name,
      available: formatQty(shortage.available, locale),
      requested: formatQty(shortage.requested, locale),
    }),
  );

  return fill(t.sales.errors.shortage, { detail: parts.join("; ") });
}

export default function SalesPage() {
  const { t, locale } = useI18n();
  const { state: session, reload: reloadSession, signOut } = useMe();

  const [load, setLoad] = useState<Load>({ status: "loading" });
  const [sales, setSales] = useState<Sale[]>([]);
  const [lines, setLines] = useState<Line[]>([emptyLine()]);
  const [channel, setChannel] = useState<string>(CHANNELS[0]);
  const [soldAt, setSoldAt] = useState(localNow());
  const [note, setNote] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const clientRef = useRef(newRef());

  useEffect(() => {
    if (session.status !== "ready") return;

    const token = readToken();

    if (!token) return;

    let active = true;

    Promise.all([api.listProducts(token), api.listSales(token, 50)])
      .then(([products, list]) => {
        if (!active) return;
        setLoad({ status: "ready", products });
        setSales(list.items);
      })
      .catch(() => {
        if (active) setLoad({ status: "error" });
      });

    return () => {
      active = false;
    };
  }, [session.status]);

  useEffect(() => {
    if (!notice) return;

    const timer = window.setTimeout(() => setNotice(null), 5_000);
    return () => window.clearTimeout(timer);
  }, [notice]);

  const products = load.status === "ready" ? load.products : [];
  const permissions = session.status === "ready" ? session.me.permissions : [];
  const canWrite = permissions.includes("sales_write");

  const total = lines.reduce((sum, line) => {
    const qty = Number(normalizeDecimal(line.qty) ?? 0);
    const price = Number(normalizeDecimal(line.price) ?? 0);

    return sum + qty * price;
  }, 0);

  function updateLine(key: string, patch: Partial<Line>) {
    setLines((current) =>
      current.map((line) => (line.key === key ? { ...line, ...patch } : line)),
    );
  }

  function pickProduct(key: string, productId: string) {
    const product = products.find((item) => item.id === productId);
    updateLine(key, {
      productId,
      price: product ? trimDecimal(product.sell_price) : "",
    });
  }

  async function submit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError(null);
    setNotice(null);

    const token = readToken();

    if (!token) return;

    const items = [];

    for (const line of lines) {
      if (!line.productId) {
        setError(t.sales.errors.productRequired);
        return;
      }

      const qty = normalizeDecimal(line.qty);

      if (qty === null || Number(qty) <= 0) {
        setError(t.sales.errors.qtyRequired);
        return;
      }

      const price = line.price.trim() === "" ? undefined : normalizeDecimal(line.price);

      if (price === null) {
        setError(t.sales.errors.priceInvalid);
        return;
      }

      items.push(
        price === undefined
          ? { product_id: line.productId, qty }
          : { product_id: line.productId, qty, price },
      );
    }

    const body: SaleRequest = {
      channel,
      client_ref: clientRef.current,
      items,
    };

    if (note.trim()) body.note = note.trim();

    if (soldAt) {
      const sold = new Date(soldAt);

      if (!Number.isNaN(sold.getTime())) body.sold_at = sold.toISOString();
    }

    setSaving(true);

    try {
      const saved = await api.createSale(token, body);

      setSales((current) => [saved, ...current]);
      setLines([emptyLine()]);
      setNote("");
      setSoldAt(localNow());
      setChannel(CHANNELS[0]);
      clientRef.current = newRef();
      setNotice(t.sales.saved);

      const fresh = await api.listProducts(token);
      setLoad({ status: "ready", products: fresh });
    } catch (cause) {
      if (cause instanceof ApiError && cause.code === "insufficient_stock") {
        setError(
          shortageMessage(t, locale, cause.details) ?? t.sales.errors.shortageUnknown,
        );
      } else if (cause instanceof ApiError && cause.code === "permission_denied") {
        setError(t.sales.errors.forbidden);
      } else {
        setError(t.sales.errors.generic);
      }
    } finally {
      setSaving(false);
    }
  }

  return (
    <>
      {session.status === "loading" && (
        <main className="page">
          <p className="lede">{t.sales.loading}</p>
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
        <AppShell me={session.me} active="pos" onSignOut={signOut}>
          <div className="sales">
            <header className="sales__header">
              <div>
                <h1>{t.sales.title}</h1>
                <p className="sales__sub">{t.sales.subtitle}</p>
              </div>
            </header>

            {notice && (
              <p className="notice" role="status">
                {notice}
              </p>
            )}

            {load.status === "loading" && <p className="lede">{t.sales.loading}</p>}

            {load.status === "error" && (
              <div className="card">
                <p>{t.sales.error}</p>
                <button
                  className="btn btn--secondary"
                  type="button"
                  onClick={() => window.location.reload()}
                >
                  {t.sales.retry}
                </button>
              </div>
            )}

            {load.status === "ready" && (
              <>
                {canWrite && (
                  <form className="sale-form" onSubmit={submit}>
                    <div className="sale-form__head">
                      <h2>{t.sales.form.title}</h2>
                      <p>{t.sales.form.hint}</p>
                    </div>

                    <div className="sale-form__meta">
                      <label className="field">
                        <span className="field__label">{t.sales.form.channel}</span>
                        <select
                          className="field__input"
                          value={channel}
                          onChange={(event) => setChannel(event.target.value)}
                        >
                          {CHANNELS.map((value) => (
                            <option key={value} value={value}>
                              {channelLabel(t, value)}
                            </option>
                          ))}
                        </select>
                      </label>

                      <label className="field">
                        <span className="field__label">{t.sales.form.soldAt}</span>
                        <input
                          className="field__input"
                          type="datetime-local"
                          value={soldAt}
                          onChange={(event) => setSoldAt(event.target.value)}
                        />
                      </label>

                      <label className="field field--grow">
                        <span className="field__label">{t.sales.form.note}</span>
                        <input
                          className="field__input"
                          type="text"
                          maxLength={500}
                          placeholder={t.sales.form.notePlaceholder}
                          value={note}
                          onChange={(event) => setNote(event.target.value)}
                        />
                      </label>
                    </div>

                    <div className="sale-lines">
                      {lines.map((line) => (
                        <div className="sale-line" key={line.key}>
                          <label className="field field--grow">
                            <span className="field__label">
                              {t.sales.form.product}
                            </span>
                            <select
                              className="field__input"
                              value={line.productId}
                              onChange={(event) =>
                                pickProduct(line.key, event.target.value)
                              }
                              required
                            >
                              <option value="">
                                {t.sales.form.selectProduct}
                              </option>
                              {products.map((product) => (
                                <option key={product.id} value={product.id}>
                                  {product.name} · {formatQty(product.stock_qty, locale)}
                                </option>
                              ))}
                            </select>
                          </label>

                          <label className="field field--qty">
                            <span className="field__label">{t.sales.form.qty}</span>
                            <input
                              className="field__input"
                              type="text"
                              inputMode="decimal"
                              value={line.qty}
                              onChange={(event) =>
                                updateLine(line.key, { qty: event.target.value })
                              }
                              required
                            />
                          </label>

                          <label className="field field--price">
                            <span className="field__label">
                              {t.sales.form.price}
                            </span>
                            <input
                              className="field__input"
                              type="text"
                              inputMode="decimal"
                              placeholder="0"
                              value={line.price}
                              onChange={(event) =>
                                updateLine(line.key, { price: event.target.value })
                              }
                            />
                          </label>

                          <button
                            className="btn btn--ghost sale-line__remove"
                            type="button"
                            aria-label={t.sales.form.removeLine}
                            disabled={lines.length === 1}
                            onClick={() =>
                              setLines((current) =>
                                current.filter((item) => item.key !== line.key),
                              )
                            }
                          >
                            <Trash2 size={15} />
                          </button>
                        </div>
                      ))}
                    </div>

                    <div className="sale-form__actions">
                      <button
                        className="btn btn--secondary"
                        type="button"
                        onClick={() =>
                          setLines((current) => [...current, emptyLine()])
                        }
                      >
                        <Plus size={15} />
                        {t.sales.form.addLine}
                      </button>

                      <div className="sale-form__total">
                        <span>{t.sales.form.total}</span>
                        <strong>{formatMoney(total, locale)}</strong>
                      </div>

                      <button
                        className="btn btn--primary"
                        type="submit"
                        disabled={saving}
                      >
                        {saving ? t.sales.form.saving : t.sales.form.submit}
                      </button>
                    </div>

                    {error && (
                      <p className="form__error" role="alert">
                        {error}
                      </p>
                    )}
                  </form>
                )}

                <section className="sales-list">
                  <h2>{t.sales.list.title}</h2>

                  {sales.length === 0 ? (
                    <div className="sales-list__empty">
                      <Receipt size={24} />
                      <p>{t.sales.list.empty}</p>
                    </div>
                  ) : (
                    sales.map((sale) => (
                      <article className="sale" key={sale.id}>
                        <header className="sale__head">
                          <div className="sale__meta">
                            <strong>{formatDateTime(sale.sold_at, locale)}</strong>
                            <span className="pill pill--neutral">
                              {channelLabel(t, sale.channel)}
                            </span>
                          </div>
                          <strong className="sale__total">
                            {formatMoney(Number(sale.total), locale)}
                          </strong>
                        </header>

                        <ul className="sale__lines">
                          {sale.items.map((item) => (
                            <li key={item.product_id}>
                              <span>{item.product_name}</span>
                              <span className="sale__calc">
                                {formatQty(item.qty, locale)} ×{" "}
                                {formatMoney(Number(item.price), locale)}
                              </span>
                              <strong>
                                {formatMoney(Number(item.subtotal), locale)}
                              </strong>
                            </li>
                          ))}
                        </ul>

                        {sale.note && <p className="sale__note">{sale.note}</p>}
                      </article>
                    ))
                  )}
                </section>
              </>
            )}
          </div>
        </AppShell>
      )}
    </>
  );
}

// Drops trailing zeros so the input shows "15000", not "15000.00".
function trimDecimal(value: string): string {
  return value.includes(".")
    ? value.replace(/0+$/, "").replace(/\.$/, "")
    : value;
}
