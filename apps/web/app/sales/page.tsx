"use client";

import {
  ApiError,
  type Product,
  type Sale,
  type SaleImportSummary,
  type SaleRequest,
} from "@takar/api-client";
import { Plus, Receipt, Trash2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import {
  LoadingRegion,
  SkeletonForm,
  SkeletonList,
} from "@/components/skeletons";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select } from "@/components/ui/select";
import { api } from "@/lib/api";
import { formatDateTime, formatMoney, formatQty, normalizeDecimal } from "@/lib/format";
import { fill, useI18n, type Dictionary } from "@/lib/i18n";
import { readToken } from "@/lib/session";
import { useMe } from "@/lib/use-me";
import { cn } from "@/lib/utils";

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

function importRowError(t: Dictionary, code: string | null): string {
  switch (code) {
    case "unknown_product":
      return t.sales.import.errors.unknownProduct;
    case "product_required":
      return t.sales.import.errors.productRequired;
    case "invalid_qty":
      return t.sales.import.errors.invalidQty;
    case "invalid_price":
      return t.sales.import.errors.invalidPrice;
    case "invalid_sold_at":
      return t.sales.import.errors.invalidSoldAt;
    case "invalid_channel":
      return t.sales.import.errors.invalidChannel;
    case "duplicate_product":
      return t.sales.import.errors.duplicateProduct;
    case "insufficient_stock":
      return t.sales.import.errors.insufficientStock;
    case "note_too_long":
      return t.sales.import.errors.noteTooLong;
    default:
      return code ?? t.sales.import.errors.generic;
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

  const [file, setFile] = useState<File | null>(null);
  const [importKey, setImportKey] = useState(0);
  const [importing, setImporting] = useState(false);
  const [importError, setImportError] = useState<string | null>(null);
  const [importResult, setImportResult] = useState<SaleImportSummary | null>(
    null,
  );

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

  async function submitImport(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setImportError(null);
    setImportResult(null);

    const token = readToken();

    if (!token || !file) return;

    setImporting(true);

    try {
      const text = await file.text();
      // Naive timestamps in the file belong to the shop's clock, not UTC.
      const result = await api.importSales(
        token,
        text,
        -new Date().getTimezoneOffset(),
      );

      setImportResult(result);
      setFile(null);
      setImportKey((key) => key + 1);

      const [fresh, list] = await Promise.all([
        api.listProducts(token),
        api.listSales(token, 50),
      ]);

      setLoad({ status: "ready", products: fresh });
      setSales(list.items);
    } catch (cause) {
      if (cause instanceof ApiError && cause.code === "permission_denied") {
        setImportError(t.sales.errors.forbidden);
      } else if (cause instanceof ApiError && cause.code === "too_many_rows") {
        setImportError(t.sales.import.errors.tooManyRows);
      } else if (cause instanceof ApiError && cause.code === "missing_header") {
        setImportError(t.sales.import.errors.missingHeader);
      } else if (cause instanceof ApiError && cause.code === "empty_file") {
        setImportError(t.sales.import.errors.emptyFile);
      } else if (cause instanceof ApiError && cause.code === "unsupported_format") {
        setImportError(t.sales.import.errors.unsupportedFormat);
      } else {
        setImportError(t.sales.import.errors.generic);
      }
    } finally {
      setImporting(false);
    }
  }

  return (
    <>
      {session.status === "loading" && (
        <AppShell active="pos">
          <div className="flex max-w-[1100px] flex-col gap-4">
            <LoadingRegion label={t.sales.loading} className="flex flex-col gap-4">
              <SkeletonForm fields={4} />
              <SkeletonList rows={3} />
            </LoadingRegion>
          </div>
        </AppShell>
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
        <AppShell me={session.me} active="pos" onSignOut={signOut}>
          <div className="flex max-w-[1100px] flex-col gap-4">
            <header>
              <h1 className="text-xl font-extrabold tracking-tight sm:text-3xl">
                {t.sales.title}
              </h1>
              <p className="mt-1 text-sm text-muted-foreground">
                {t.sales.subtitle}
              </p>
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
              <LoadingRegion
                label={t.sales.loading}
                className="flex flex-col gap-4"
              >
                <SkeletonForm fields={4} />
                <SkeletonList rows={3} />
              </LoadingRegion>
            )}

            {load.status === "error" && (
              <div className="flex flex-col gap-3 rounded-xl border border-border bg-card p-5">
                <p className="text-sm text-muted-foreground">{t.sales.error}</p>
                <Button
                  variant="secondary"
                  size="sm"
                  className="self-start"
                  type="button"
                  onClick={() => window.location.reload()}
                >
                  {t.sales.retry}
                </Button>
              </div>
            )}

            {load.status === "ready" && (
              <>
                {canWrite && (
                  <form
                    className="flex flex-col gap-3.5 rounded-xl border border-border bg-card p-4 sm:p-4.5"
                    onSubmit={submit}
                  >
                    <div>
                      <h2 className="text-base font-bold">
                        {t.sales.form.title}
                      </h2>
                      <p className="mt-0.5 max-w-[70ch] text-xs leading-relaxed text-ink-3">
                        {t.sales.form.hint}
                      </p>
                    </div>

                    <div className="grid gap-2.5 sm:grid-cols-[160px_190px_minmax(0,1fr)]">
                      <Field label={t.sales.form.channel}>
                        <Select
                          value={channel}
                          onChange={(event) => setChannel(event.target.value)}
                        >
                          {CHANNELS.map((value) => (
                            <option key={value} value={value}>
                              {channelLabel(t, value)}
                            </option>
                          ))}
                        </Select>
                      </Field>

                      <Field label={t.sales.form.soldAt}>
                        <Input
                          type="datetime-local"
                          value={soldAt}
                          onChange={(event) => setSoldAt(event.target.value)}
                        />
                      </Field>

                      <Field label={t.sales.form.note}>
                        <Input
                          type="text"
                          maxLength={500}
                          placeholder={t.sales.form.notePlaceholder}
                          value={note}
                          onChange={(event) => setNote(event.target.value)}
                        />
                      </Field>
                    </div>

                    <div className="flex flex-col gap-2.5">
                      {lines.map((line) => (
                        <div
                          className="flex flex-wrap items-end gap-2.5"
                          key={line.key}
                        >
                          <Field
                            label={t.sales.form.product}
                            className="min-w-[200px] flex-1"
                          >
                            <Select
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
                                  {product.name} ·{" "}
                                  {formatQty(product.stock_qty, locale)}
                                </option>
                              ))}
                            </Select>
                          </Field>

                          <Field label={t.sales.form.qty} className="w-[100px]">
                            <Input
                              type="text"
                              inputMode="decimal"
                              value={line.qty}
                              onChange={(event) =>
                                updateLine(line.key, { qty: event.target.value })
                              }
                              required
                            />
                          </Field>

                          <Field label={t.sales.form.price} className="w-[150px]">
                            <Input
                              type="text"
                              inputMode="decimal"
                              placeholder="0"
                              value={line.price}
                              onChange={(event) =>
                                updateLine(line.key, {
                                  price: event.target.value,
                                })
                              }
                            />
                          </Field>

                          <Button
                            variant="ghost"
                            size="icon"
                            className="size-11"
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
                          </Button>
                        </div>
                      ))}
                    </div>

                    <div className="flex flex-wrap items-center gap-2.5 border-t border-border pt-3.5">
                      <Button
                        variant="secondary"
                        type="button"
                        onClick={() =>
                          setLines((current) => [...current, emptyLine()])
                        }
                      >
                        <Plus size={15} />
                        {t.sales.form.addLine}
                      </Button>

                      <div className="flex items-baseline gap-2 sm:ml-auto">
                        <span className="text-2xs font-semibold tracking-wide text-ink-3 uppercase">
                          {t.sales.form.total}
                        </span>
                        <strong className="text-xl font-extrabold tabular-nums">
                          {formatMoney(total, locale)}
                        </strong>
                      </div>

                      <Button type="submit" disabled={saving}>
                        {saving ? t.sales.form.saving : t.sales.form.submit}
                      </Button>
                    </div>

                    {error && (
                      <p
                        role="alert"
                        className="rounded-lg bg-danger-soft px-3 py-2.5 text-xs text-destructive"
                      >
                        {error}
                      </p>
                    )}
                  </form>
                )}

                {canWrite && (
                  <section className="flex flex-col gap-3 rounded-xl border border-dashed border-input bg-card p-4 sm:p-4.5">
                    <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
                      <div>
                        <h2 className="text-base font-bold">
                          {t.sales.import.title}
                        </h2>
                        <p className="mt-0.5 max-w-[66ch] text-xs leading-relaxed text-ink-3">
                          {t.sales.import.body}
                        </p>
                      </div>

                      <Button variant="secondary" asChild className="shrink-0">
                        <a href="/templates/sales-import.csv" download>
                          {t.sales.import.download}
                        </a>
                      </Button>
                    </div>

                    <form
                      className="flex flex-wrap items-center gap-2.5"
                      onSubmit={submitImport}
                    >
                      <input
                        key={importKey}
                        type="file"
                        accept=".csv,text/csv"
                        onChange={(event) => {
                          setFile(event.target.files?.[0] ?? null);
                          setImportResult(null);
                          setImportError(null);
                        }}
                        required
                        className="max-w-full text-sm text-muted-foreground file:mr-2.5 file:h-9 file:cursor-pointer file:rounded-lg file:border file:border-input file:bg-surface-alt file:px-3 file:text-xs file:font-semibold file:text-ink"
                      />

                      <Button
                        type="submit"
                        disabled={importing || !file}
                      >
                        {importing
                          ? t.sales.import.importing
                          : t.sales.import.submit}
                      </Button>
                    </form>

                    {importError && (
                      <p
                        role="alert"
                        className="rounded-lg bg-danger-soft px-3 py-2.5 text-xs text-destructive"
                      >
                        {importError}
                      </p>
                    )}

                    {importResult && (
                      <div
                        role="status"
                        className="rounded-lg bg-surface-alt px-3 py-2.5 text-sm"
                      >
                        <p>
                          {fill(t.sales.import.summary, {
                            created: importResult.created,
                            duplicate: importResult.duplicate,
                            rejected: importResult.rejected,
                          })}
                        </p>

                        {importResult.rejected > 0 && (
                          <ul className="mt-1.5 flex list-none flex-col gap-1 p-0 text-destructive">
                            {importResult.rows
                              .filter((row) => row.status === "rejected")
                              .map((row) => (
                                <li key={row.row} className="text-xs">
                                  <strong>
                                    {fill(t.sales.import.row, { row: row.row })}
                                  </strong>
                                  {": "}
                                  {importRowError(t, row.code)}
                                </li>
                              ))}
                          </ul>
                        )}
                      </div>
                    )}
                  </section>
                )}

                <section className="flex flex-col gap-2.5">
                  <h2 className="text-base font-bold">{t.sales.list.title}</h2>

                  {sales.length === 0 ? (
                    <div className="flex flex-col items-center gap-2 rounded-xl border border-border bg-card px-5 py-9 text-center text-ink-3">
                      <Receipt size={24} />
                      <p className="text-sm">{t.sales.list.empty}</p>
                    </div>
                  ) : (
                    sales.map((sale) => (
                      <article
                        className="flex flex-col gap-2.5 rounded-xl border border-border bg-card px-4 py-3.5"
                        key={sale.id}
                      >
                        <header className="flex items-center justify-between gap-3">
                          <div className="flex items-center gap-2">
                            <strong className="text-base font-bold">
                              {formatDateTime(sale.sold_at, locale)}
                            </strong>
                            <Badge>{channelLabel(t, sale.channel)}</Badge>
                          </div>
                          <strong className="text-lg font-extrabold tabular-nums">
                            {formatMoney(Number(sale.total), locale)}
                          </strong>
                        </header>

                        <ul className="flex list-none flex-col gap-1 p-0">
                          {sale.items.map((item) => (
                            <li
                              key={item.product_id}
                              className="grid grid-cols-[minmax(0,1fr)_auto_90px] items-baseline gap-2.5 border-t border-border pt-1 text-sm"
                            >
                              <span>{item.product_name}</span>
                              <span className="text-xs text-ink-3 tabular-nums">
                                {formatQty(item.qty, locale)} ×{" "}
                                {formatMoney(Number(item.price), locale)}
                              </span>
                              <strong className="text-right tabular-nums">
                                {formatMoney(Number(item.subtotal), locale)}
                              </strong>
                            </li>
                          ))}
                        </ul>

                        {sale.note && (
                          <p className="text-xs text-muted-foreground">
                            {sale.note}
                          </p>
                        )}
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

function Field({
  label,
  className,
  children,
}: {
  label: string;
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <label className={cn("flex flex-col gap-1.5", className)}>
      <Label>{label}</Label>
      {children}
    </label>
  );
}

// Drops trailing zeros so the input shows "15000", not "15000.00".
function trimDecimal(value: string): string {
  return value.includes(".")
    ? value.replace(/0+$/, "").replace(/\.$/, "")
    : value;
}
