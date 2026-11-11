"use client";

import type {
  Batch,
  Material,
  StockLevel,
  Supplier,
  Unit,
} from "@takar/api-client";
import { Ellipsis, Plus, RotateCcw, Search, X } from "lucide-react";
import Link from "next/link";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { api } from "@/lib/api";
import { daysUntil, formatDate, formatMoney, formatQty } from "@/lib/format";
import { fill, useI18n, type Dictionary } from "@/lib/i18n";
import {
  dismissEntry,
  flushQueue,
  listQueue,
  offlineQueueAvailable,
  retryEntry,
  type QueuedStockIn,
} from "@/lib/outbox";
import { readToken } from "@/lib/session";
import { useMe } from "@/lib/use-me";

import { AppShell } from "../app-shell";
import { BatchPanel } from "./batch-panel";
import { MaterialDialog } from "./material-dialog";
import { StockInDialog } from "./stock-in-dialog";

type Filter = "all" | "below_min" | "expiring" | "empty";

type Data = {
  stock: StockLevel[];
  materials: Material[];
  suppliers: Supplier[];
  units: Unit[];
  batches: Batch[];
};

type Load =
  | { status: "loading" }
  | { status: "ready"; data: Data }
  | { status: "error" };

export default function StockPage() {
  const { t, locale } = useI18n();
  const { state: session, reload: reloadSession, signOut } = useMe();

  const [load, setLoad] = useState<Load>({ status: "loading" });
  const [filter, setFilter] = useState<Filter>("all");
  const [query, setQuery] = useState("");
  const [panelId, setPanelId] = useState<string | null>(null);
  const [stockInFor, setStockInFor] = useState<string | null | undefined>();
  const [materialOpen, setMaterialOpen] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [queue, setQueue] = useState<QueuedStockIn[]>([]);
  const [syncing, setSyncing] = useState(false);

  const fetchData = useCallback(async () => {
    const token = readToken();

    if (!token) return;

    setLoad((current) =>
      current.status === "ready" ? current : { status: "loading" },
    );

    try {
      const [stock, materials, suppliers, units, batches] = await Promise.all([
        api.listStock(token),
        api.listMaterials(token),
        api.listSuppliers(token),
        api.listUnits(token),
        api.listBatches(token),
      ]);

      setLoad({
        status: "ready",
        data: { stock, materials, suppliers, units, batches },
      });
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

  const refreshQueue = useCallback(async () => {
    if (!offlineQueueAvailable()) return;

    setQueue(await listQueue());
  }, []);

  const syncNow = useCallback(async () => {
    const token = readToken();

    if (!token || !offlineQueueAvailable()) return;

    setSyncing(true);

    try {
      const summary = await flushQueue(token);

      if (summary.synced > 0) {
        setNotice(fill(t.stock.offline.synced, { count: summary.synced }));
        void fetchData();
      }

      if (summary.failed > 0) {
        setNotice(fill(t.stock.offline.rejected, { count: summary.failed }));
      }
    } catch {
      // Still offline, or the server is unreachable: entries stay queued.
    } finally {
      await refreshQueue();
      setSyncing(false);
    }
  }, [fetchData, refreshQueue, t]);

  useEffect(() => {
    if (session.status !== "ready" || !offlineQueueAvailable()) return;

    void refreshQueue();

    if (navigator.onLine) {
      void syncNow();
    }

    const handleOnline = () => void syncNow();

    window.addEventListener("online", handleOnline);
    return () => window.removeEventListener("online", handleOnline);
  }, [session.status, refreshQueue, syncNow]);

  async function retryQueued(clientRef: string) {
    await retryEntry(clientRef);
    await refreshQueue();

    if (navigator.onLine) {
      await syncNow();
    }
  }

  async function dismissQueued(clientRef: string) {
    await dismissEntry(clientRef);
    await refreshQueue();
  }

  const data = load.status === "ready" ? load.data : null;

  // The dashboard links straight to one material; open its drawer once, then
  // leave the page alone so closing it sticks.
  const deepLinkApplied = useRef(false);

  useEffect(() => {
    if (!data || deepLinkApplied.current) return;

    deepLinkApplied.current = true;

    const id = new URLSearchParams(window.location.search).get("material");

    if (id && data.stock.some((row) => row.material_id === id)) {
      setPanelId(id);
    }
  }, [data]);

  const supplierById = useMemo(
    () => new Map((data?.suppliers ?? []).map((item) => [item.id, item])),
    [data],
  );

  const materialById = useMemo(
    () => new Map((data?.materials ?? []).map((item) => [item.id, item])),
    [data],
  );

  const batchesByMaterial = useMemo(() => {
    const map = new Map<string, Batch[]>();

    for (const batch of data?.batches ?? []) {
      const rows = map.get(batch.material_id) ?? [];
      rows.push(batch);
      map.set(batch.material_id, rows);
    }

    return map;
  }, [data]);

  const summary = useMemo(() => {
    if (!data) return null;

    let value = 0;
    let belowMin = 0;

    for (const row of data.stock) {
      value += Number(row.on_hand) * Number(row.last_cost);
      if (row.below_min) belowMin += 1;
    }

    let quantity = 0;
    let age = 0;
    let expiringBatches = 0;

    for (const batch of data.batches) {
      const remaining = Number(batch.qty_remaining);

      if (remaining <= 0) continue;

      quantity += remaining;
      age += remaining * Math.max(0, -daysUntil(batch.received_at));

      if (batch.expires_at && daysUntil(batch.expires_at) <= 7) {
        expiringBatches += 1;
      }
    }

    return {
      value,
      belowMin,
      expiringBatches,
      averageAge: quantity > 0 ? Math.round(age / quantity) : null,
    };
  }, [data]);

  const counts = useMemo(() => {
    const stock = data?.stock ?? [];

    return {
      all: stock.length,
      below_min: stock.filter((row) => row.below_min).length,
      expiring: stock.filter(
        (row) => row.nearest_expiry !== null && daysUntil(row.nearest_expiry) <= 7,
      ).length,
      empty: stock.filter((row) => row.active_batches === 0).length,
    };
  }, [data]);

  const visible = useMemo(() => {
    if (!data) return [];

    const term = query.trim().toLowerCase();

    return data.stock.filter((row) => {
      if (term && !row.name.toLowerCase().includes(term)) return false;

      switch (filter) {
        case "below_min":
          return row.below_min;
        case "expiring":
          return (
            row.nearest_expiry !== null && daysUntil(row.nearest_expiry) <= 7
          );
        case "empty":
          return row.active_batches === 0;
        default:
          return true;
      }
    });
  }, [data, filter, query]);

  const permissions = session.status === "ready" ? session.me.permissions : [];
  const canWriteBatches = permissions.includes("batch_write");
  const canWriteMaterials = permissions.includes("material_write");

  const panelRow =
    data?.stock.find((row) => row.material_id === panelId) ?? null;

  function supplierName(id: string | null): string {
    if (!id) return t.stock.meta.noSupplier;
    return supplierById.get(id)?.name ?? t.stock.meta.noSupplier;
  }

  function expiryLabel(row: StockLevel): string {
    if (!row.nearest_expiry) return "—";

    const days = daysUntil(row.nearest_expiry);
    const date = formatDate(row.nearest_expiry, locale);

    if (days < 0) {
      return `${date} · ${fill(t.stock.meta.daysOverdue, { days: Math.abs(days) })}`;
    }

    if (days === 0) return `${date} · ${t.stock.meta.today}`;

    return `${date} · ${fill(t.stock.meta.daysLeft, { days })}`;
  }

  function statusOf(row: StockLevel): { label: string; tone: string } {
    if (row.below_min) {
      return { label: t.stock.status.critical, tone: "critical" };
    }

    if (row.nearest_expiry) {
      const days = daysUntil(row.nearest_expiry);

      if (days < 0) {
        return { label: t.stock.status.expired, tone: "critical" };
      }

      if (days <= 7) {
        return {
          label: fill(t.stock.status.expiring, { days }),
          tone: "warning",
        };
      }
    }

    if (row.active_batches === 0) {
      return { label: t.stock.status.empty, tone: "neutral" };
    }

    return { label: t.stock.status.safe, tone: "safe" };
  }

  const batchCount = data?.batches.length ?? 0;

  const filterTabs: { key: Filter; label: string; short: string }[] = [
    { key: "all", label: t.stock.filters.all, short: t.stock.mobileFilters.all },
    {
      key: "below_min",
      label: t.stock.filters.belowMin,
      short: t.stock.mobileFilters.belowMin,
    },
    {
      key: "expiring",
      label: t.stock.filters.expiring,
      short: t.stock.mobileFilters.expiring,
    },
    { key: "empty", label: t.stock.filters.empty, short: t.stock.mobileFilters.empty },
  ];

  const subtitle = summary
    ? fill(
        data!.stock.length === 1 && batchCount === 1
          ? t.stock.subtitleSingle
          : t.stock.subtitle,
        {
          materials: data!.stock.length,
          batches: batchCount,
          value: formatMoney(summary.value, locale),
        },
      )
    : "";

  function closeStockIn() {
    setStockInFor(undefined);
  }

  return (
    <>
      {session.status === "loading" && (
        <main className="page">
          <p className="lede">{t.stock.loading}</p>
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
        <AppShell me={session.me} active="stock" onSignOut={signOut}>
          <div className="stock">
            <header className="stock__header">
              <div>
                <h1>{t.stock.title}</h1>
                <p className="stock__sub">{subtitle}</p>
              </div>
              <div className="stock__actions">
                {canWriteMaterials && (
                  <button
                    className="btn btn--ghost"
                    type="button"
                    onClick={() => setMaterialOpen(true)}
                  >
                    <Plus size={15} />
                    {t.stock.newMaterial}
                  </button>
                )}
                {canWriteBatches && (
                  <button
                    className="btn btn--primary"
                    type="button"
                    onClick={() => setStockInFor(null)}
                  >
                    <Plus size={15} />
                    {t.stock.stockIn}
                  </button>
                )}
              </div>
            </header>

            {summary && (
              <section className="summary" aria-label={t.stock.title}>
                <div className="summary__metric">
                  <span>{t.stock.summary.value}</span>
                  <strong>{formatMoney(summary.value, locale)}</strong>
                </div>
                <span className="summary__divider" />
                <div className="summary__metric">
                  <span>{t.stock.summary.belowMin}</span>
                  <strong className={summary.belowMin > 0 ? "is-danger" : undefined}>
                    {fill(t.stock.summary.materials, { count: summary.belowMin })}
                  </strong>
                </div>
                <span className="summary__divider" />
                <div className="summary__metric">
                  <span>{t.stock.summary.expiring}</span>
                  <strong
                    className={summary.expiringBatches > 0 ? "is-warning" : undefined}
                  >
                    {fill(t.stock.summary.batches, { count: summary.expiringBatches })}
                  </strong>
                </div>
                <span className="summary__divider" />
                <div className="summary__metric">
                  <span>{t.stock.summary.avgAge}</span>
                  <strong>
                    {summary.averageAge === null
                      ? t.stock.summary.empty
                      : fill(t.stock.summary.days, { count: summary.averageAge })}
                  </strong>
                </div>
              </section>
            )}

            {queue.length > 0 && (
              <section className="outbox" aria-live="polite">
                <div className="outbox__head">
                  <div>
                    <strong>
                      {fill(t.stock.offline.title, { count: queue.length })}
                    </strong>
                    <span>
                      {syncing ? t.stock.offline.syncing : t.stock.offline.body}
                    </span>
                  </div>
                  <button
                    className="btn btn--secondary"
                    type="button"
                    onClick={() => void syncNow()}
                    disabled={syncing || !navigator.onLine}
                  >
                    {t.stock.offline.sync}
                  </button>
                </div>

                <ul className="outbox__list">
                  {queue.map((entry) => {
                    const material = data?.materials.find(
                      (item) => item.id === entry.payload.material_id,
                    );

                    return (
                      <li
                        className={`outbox__item${
                          entry.status === "failed"
                            ? " outbox__item--failed"
                            : ""
                        }`}
                        key={entry.client_ref}
                      >
                        <div className="outbox__what">
                          <strong>
                            {material?.name ?? entry.payload.material_id}
                          </strong>
                          <span>
                            {formatQty(entry.payload.qty_received, locale)}{" "}
                            {material?.base_unit.code ?? ""} ·{" "}
                            {formatMoney(
                              Number(entry.payload.unit_cost),
                              locale,
                            )}
                          </span>
                        </div>

                        {entry.status === "failed" ? (
                          <div className="outbox__actions">
                            <span className="pill pill--critical">
                              {offlineReason(t, entry.code)}
                            </span>
                            <button
                              className="icon-btn"
                              type="button"
                              onClick={() => void retryQueued(entry.client_ref)}
                              aria-label={t.stock.offline.retry}
                            >
                              <RotateCcw size={14} />
                            </button>
                            <button
                              className="icon-btn"
                              type="button"
                              onClick={() =>
                                void dismissQueued(entry.client_ref)
                              }
                              aria-label={t.stock.offline.dismiss}
                            >
                              <X size={14} />
                            </button>
                          </div>
                        ) : (
                          <span className="pill pill--warning">
                            {t.stock.offline.pending}
                          </span>
                        )}
                      </li>
                    );
                  })}
                </ul>
              </section>
            )}

            {notice && (
              <p className="notice" role="status">
                {notice}
              </p>
            )}

            {load.status === "loading" && <p className="lede">{t.stock.loading}</p>}

            {load.status === "error" && (
              <div className="card">
                <p>{t.stock.error}</p>
                <button
                  className="btn btn--secondary"
                  type="button"
                  onClick={() => void fetchData()}
                >
                  {t.stock.retry}
                </button>
              </div>
            )}

            {data && (
              <section className="stock__card">
                <div className="stock__toolbar">
                  <div className="stock__tabs" role="tablist" aria-label={t.stock.title}>
                    {filterTabs.map((tab) => (
                      <button
                        key={tab.key}
                        type="button"
                        role="tab"
                        aria-selected={filter === tab.key}
                        className={`stock__tab${
                          filter === tab.key ? " stock__tab--active" : ""
                        }`}
                        onClick={() => setFilter(tab.key)}
                      >
                        <span className="stock__tab-long">{tab.label}</span>
                        <span className="stock__tab-short">{tab.short}</span>
                        <span className="stock__tab-count">{counts[tab.key]}</span>
                      </button>
                    ))}
                  </div>

                  <label className="stock__search">
                    <Search size={14} />
                    <input
                      type="search"
                      value={query}
                      onChange={(event) => setQuery(event.target.value)}
                      placeholder={t.stock.searchPlaceholder}
                      aria-label={t.stock.searchPlaceholder}
                    />
                  </label>
                </div>

                {data.stock.length === 0 ? (
                  <div className="stock__empty">
                    <h2>{t.stock.empty.title}</h2>
                    <p>{t.stock.empty.body}</p>
                    {canWriteMaterials && (
                      <div className="stock__empty-actions">
                        <button
                          className="btn btn--primary"
                          type="button"
                          onClick={() => setMaterialOpen(true)}
                        >
                          {t.stock.empty.cta}
                        </button>
                        <Link className="btn btn--ghost" href="/onboarding">
                          {t.stock.empty.template}
                        </Link>
                      </div>
                    )}
                  </div>
                ) : visible.length === 0 ? (
                  <p className="stock__empty-line">{t.stock.empty.filtered}</p>
                ) : (
                  <div className="stock-table" role="table" aria-label={t.stock.title}>
                    <div className="stock-table__head" role="row">
                      <span role="columnheader">{t.stock.table.material}</span>
                      <span role="columnheader">{t.stock.table.stock}</span>
                      <span role="columnheader">{t.stock.table.min}</span>
                      <span role="columnheader">{t.stock.table.batches}</span>
                      <span role="columnheader">{t.stock.table.nearestExpiry}</span>
                      <span role="columnheader">{t.stock.table.status}</span>
                      <span role="columnheader">{t.stock.table.value}</span>
                      <span role="columnheader" aria-hidden="true" />
                    </div>

                    {visible.map((row) => {
                      const status = statusOf(row);

                      return (
                        <div className="stock-row" role="row" key={row.material_id}>
                          <div className="stock-row__material" role="cell">
                            <button
                              className="stock-row__name"
                              type="button"
                              onClick={() => setPanelId(row.material_id)}
                            >
                              {row.name}
                            </button>
                            <span>
                              {supplierName(
                                materialById.get(row.material_id)
                                  ?.default_supplier_id ?? null,
                              )}
                            </span>
                          </div>
                          <div className="stock-row__stock" role="cell">
                            <strong>{formatQty(row.on_hand, locale)}</strong>
                            <span>{row.base_unit.code}</span>
                          </div>
                          <div className="stock-row__min" role="cell">
                            {formatQty(row.min_stock, locale)} {row.base_unit.code}
                          </div>
                          <div className="stock-row__batches" role="cell">
                            {row.active_batches}
                          </div>
                          <div className="stock-row__expiry" role="cell">
                            {expiryLabel(row)}
                          </div>
                          <div className="stock-row__status" role="cell">
                            <span className={`pill pill--${status.tone}`}>
                              {status.label}
                            </span>
                          </div>
                          <div className="stock-row__value" role="cell">
                            {formatMoney(
                              Number(row.on_hand) * Number(row.last_cost),
                              locale,
                            )}
                          </div>
                          <div className="stock-row__action" role="cell">
                            <button
                              className="icon-btn"
                              type="button"
                              onClick={() => setPanelId(row.material_id)}
                              aria-label={t.stock.panel.title}
                            >
                              <Ellipsis size={15} />
                            </button>
                          </div>
                        </div>
                      );
                    })}
                  </div>
                )}
              </section>
            )}
          </div>

          {panelRow && (
            <BatchPanel
              material={panelRow}
              batches={batchesByMaterial.get(panelRow.material_id) ?? []}
              suppliers={data?.suppliers ?? []}
              canStockIn={canWriteBatches}
              onClose={() => setPanelId(null)}
              onStockIn={() => {
                const id = panelRow.material_id;
                setPanelId(null);
                setStockInFor(id);
              }}
            />
          )}

          {stockInFor !== undefined && (
            <StockInDialog
              materials={data?.materials ?? []}
              suppliers={data?.suppliers ?? []}
              defaultMaterialId={stockInFor}
              onClose={closeStockIn}
              onSaved={() => {
                closeStockIn();
                setNotice(t.stock.saved);
                void fetchData();
              }}
              onQueued={() => {
                closeStockIn();
                setNotice(t.stock.offline.queued);
                void refreshQueue();
              }}
            />
          )}

          {materialOpen && (
            <MaterialDialog
              units={data?.units ?? []}
              suppliers={data?.suppliers ?? []}
              onClose={() => setMaterialOpen(false)}
              onCreated={() => {
                setMaterialOpen(false);
                setNotice(t.stock.materialCreated);
                void fetchData();
              }}
            />
          )}
        </AppShell>
      )}
    </>
  );
}

function offlineReason(t: Dictionary, code: string | null): string {
  switch (code) {
    case "material_not_found":
      return t.stock.offline.reasons.materialMissing;
    case "invalid_quantity":
      return t.stock.offline.reasons.invalidQuantity;
    case "invalid_cost":
      return t.stock.offline.reasons.invalidCost;
    case "invalid_supplier":
      return t.stock.offline.reasons.invalidSupplier;
    case "permission_denied":
      return t.stock.offline.reasons.forbidden;
    default:
      return t.stock.offline.reasons.generic;
  }
}
