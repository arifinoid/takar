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

import { LoadingRegion, SkeletonTable } from "@/components/skeletons";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
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

  function statusOf(row: StockLevel): { label: string; tone: Tone } {
    if (row.below_min) {
      return { label: t.stock.status.critical, tone: "danger" };
    }

    if (row.nearest_expiry) {
      const days = daysUntil(row.nearest_expiry);

      if (days < 0) {
        return { label: t.stock.status.expired, tone: "danger" };
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

    return { label: t.stock.status.safe, tone: "brand" };
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
        <main className="mx-auto max-w-[1200px] p-6">
          <LoadingRegion label={t.stock.loading}>
            <SkeletonTable rows={6} columns={5} />
          </LoadingRegion>
        </main>
      )}

      {session.status === "error" && (
        <main className="mx-auto max-w-[1200px] p-6">
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
        <AppShell me={session.me} active="stock" onSignOut={signOut}>
          <div className="flex max-w-[1200px] flex-col gap-4">
            <header className="flex flex-col items-start gap-4 sm:flex-row sm:items-end sm:justify-between">
              <div>
                <h1 className="text-xl font-extrabold tracking-tight sm:text-3xl">
                  {t.stock.title}
                </h1>
                <p className="mt-1 text-sm text-muted-foreground">{subtitle}</p>
              </div>
              <div className="flex w-full flex-wrap items-center gap-2.5 sm:w-auto">
                {canWriteMaterials && (
                  <Button
                    variant="ghost"
                    className="flex-1 sm:flex-none"
                    type="button"
                    onClick={() => setMaterialOpen(true)}
                  >
                    <Plus size={15} />
                    {t.stock.newMaterial}
                  </Button>
                )}
                {canWriteBatches && (
                  <Button
                    className="flex-1 sm:flex-none"
                    type="button"
                    onClick={() => setStockInFor(null)}
                  >
                    <Plus size={15} />
                    {t.stock.stockIn}
                  </Button>
                )}
              </div>
            </header>

            {summary && (
              <section
                aria-label={t.stock.title}
                className="grid grid-cols-2 gap-y-3 rounded-xl border border-border bg-card px-2 py-2 sm:grid-cols-4 sm:py-3.5"
              >
                <Metric label={t.stock.summary.value}>
                  {formatMoney(summary.value, locale)}
                </Metric>
                <Metric
                  label={t.stock.summary.belowMin}
                  tone={summary.belowMin > 0 ? "danger" : undefined}
                >
                  {fill(t.stock.summary.materials, { count: summary.belowMin })}
                </Metric>
                <Metric
                  label={t.stock.summary.expiring}
                  tone={summary.expiringBatches > 0 ? "warning" : undefined}
                >
                  {fill(t.stock.summary.batches, {
                    count: summary.expiringBatches,
                  })}
                </Metric>
                <Metric label={t.stock.summary.avgAge}>
                  {summary.averageAge === null
                    ? t.stock.summary.empty
                    : fill(t.stock.summary.days, { count: summary.averageAge })}
                </Metric>
              </section>
            )}

            {queue.length > 0 && (
              <section
                aria-live="polite"
                className="flex flex-col gap-3 rounded-xl border border-border bg-card p-4"
              >
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div className="flex flex-col gap-0.5">
                    <strong className="text-sm font-bold">
                      {fill(t.stock.offline.title, { count: queue.length })}
                    </strong>
                    <span className="text-xs text-muted-foreground">
                      {syncing ? t.stock.offline.syncing : t.stock.offline.body}
                    </span>
                  </div>
                  <Button
                    variant="secondary"
                    size="sm"
                    type="button"
                    onClick={() => void syncNow()}
                    disabled={syncing || !navigator.onLine}
                  >
                    {t.stock.offline.sync}
                  </Button>
                </div>

                <ul className="flex list-none flex-col gap-1.5 p-0">
                  {queue.map((entry) => {
                    const material = data?.materials.find(
                      (item) => item.id === entry.payload.material_id,
                    );

                    return (
                      <li
                        className={`flex items-center justify-between gap-2.5 rounded-lg px-2.5 py-2 ${
                          entry.status === "failed"
                            ? "bg-danger-soft"
                            : "bg-surface-alt"
                        }`}
                        key={entry.client_ref}
                      >
                        <div className="flex min-w-0 flex-col gap-px">
                          <strong className="truncate text-xs font-bold">
                            {material?.name ?? entry.payload.material_id}
                          </strong>
                          <span className="text-2xs text-ink-3 tabular-nums">
                            {formatQty(entry.payload.qty_received, locale)}{" "}
                            {material?.base_unit.code ?? ""} ·{" "}
                            {formatMoney(
                              Number(entry.payload.unit_cost),
                              locale,
                            )}
                          </span>
                        </div>

                        {entry.status === "failed" ? (
                          <div className="flex items-center gap-1.5">
                            <Badge variant="danger">
                              {offlineReason(t, entry.code)}
                            </Badge>
                            <Button
                              variant="ghost"
                              size="icon"
                              className="size-7 rounded-md"
                              type="button"
                              onClick={() => void retryQueued(entry.client_ref)}
                              aria-label={t.stock.offline.retry}
                            >
                              <RotateCcw size={14} />
                            </Button>
                            <Button
                              variant="ghost"
                              size="icon"
                              className="size-7 rounded-md"
                              type="button"
                              onClick={() =>
                                void dismissQueued(entry.client_ref)
                              }
                              aria-label={t.stock.offline.dismiss}
                            >
                              <X size={14} />
                            </Button>
                          </div>
                        ) : (
                          <Badge variant="warning">
                            {t.stock.offline.pending}
                          </Badge>
                        )}
                      </li>
                    );
                  })}
                </ul>
              </section>
            )}

            {notice && (
              <p
                role="status"
                className="rounded-lg bg-brand-soft px-3 py-2.5 text-xs font-medium text-brand"
              >
                {notice}
              </p>
            )}

            {load.status === "loading" && (
              <LoadingRegion label={t.stock.loading}>
                <div className="rounded-xl border border-border bg-card p-5">
                  <SkeletonTable rows={6} columns={5} />
                </div>
              </LoadingRegion>
            )}

            {load.status === "error" && (
              <div className="flex flex-col gap-3 rounded-xl border border-border bg-card p-5">
                <p className="text-sm text-muted-foreground">{t.stock.error}</p>
                <Button
                  variant="secondary"
                  size="sm"
                  className="self-start"
                  type="button"
                  onClick={() => void fetchData()}
                >
                  {t.stock.retry}
                </Button>
              </div>
            )}

            {data && (
              <section className="overflow-hidden rounded-xl border border-border bg-card">
                <div className="flex flex-col items-stretch gap-3 border-b border-border px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
                  <div
                    role="tablist"
                    aria-label={t.stock.title}
                    className="flex gap-4.5 overflow-x-auto [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
                  >
                    {filterTabs.map((tab) => {
                      const active = filter === tab.key;

                      return (
                        <button
                          key={tab.key}
                          type="button"
                          role="tab"
                          aria-selected={active}
                          className={`flex shrink-0 items-center gap-1.5 border-b-2 pb-2 pt-0.5 text-sm whitespace-nowrap transition-colors duration-150 ${
                            active
                              ? "border-brand font-bold text-ink"
                              : "border-transparent font-semibold text-ink-3 hover:text-ink-2"
                          }`}
                          onClick={() => setFilter(tab.key)}
                        >
                          <span className="hidden sm:inline">{tab.label}</span>
                          <span className="sm:hidden">{tab.short}</span>
                          <span
                            className={`rounded-full px-1.5 py-px text-2xs font-bold ${
                              active
                                ? "bg-brand-soft text-brand"
                                : "bg-surface-alt text-ink-3"
                            }`}
                          >
                            {counts[tab.key]}
                          </span>
                        </button>
                      );
                    })}
                  </div>

                  <label className="flex w-full items-center gap-2 rounded-lg border border-border bg-surface-alt px-2.5 py-1.5 text-ink-3 sm:w-[220px]">
                    <Search size={14} />
                    <input
                      type="search"
                      value={query}
                      onChange={(event) => setQuery(event.target.value)}
                      placeholder={t.stock.searchPlaceholder}
                      aria-label={t.stock.searchPlaceholder}
                      className="w-full border-0 bg-transparent text-sm text-ink outline-none placeholder:text-ink-3"
                    />
                  </label>
                </div>

                {data.stock.length === 0 ? (
                  <div className="flex flex-col items-center gap-2 px-5 py-11 text-center">
                    <h2 className="text-lg font-bold">{t.stock.empty.title}</h2>
                    <p className="text-sm text-muted-foreground">
                      {t.stock.empty.body}
                    </p>
                    {canWriteMaterials && (
                      <div className="mt-1 flex flex-wrap justify-center gap-2">
                        <Button
                          type="button"
                          onClick={() => setMaterialOpen(true)}
                        >
                          {t.stock.empty.cta}
                        </Button>
                        <Button variant="ghost" asChild>
                          <Link href="/onboarding">{t.stock.empty.template}</Link>
                        </Button>
                      </div>
                    )}
                  </div>
                ) : visible.length === 0 ? (
                  <p className="px-4 py-6 text-sm text-muted-foreground">
                    {t.stock.empty.filtered}
                  </p>
                ) : (
                  <div role="table" aria-label={t.stock.title}>
                    <div
                      role="row"
                      className="hidden items-center gap-3 bg-surface-alt px-4 py-2.5 text-2xs font-bold tracking-wider text-ink-3 uppercase sm:grid sm:grid-cols-[minmax(0,1fr)_110px_70px_62px_130px_96px_104px_40px]"
                    >
                      <span role="columnheader">{t.stock.table.material}</span>
                      <span role="columnheader">{t.stock.table.stock}</span>
                      <span role="columnheader">{t.stock.table.min}</span>
                      <span role="columnheader">{t.stock.table.batches}</span>
                      <span role="columnheader">
                        {t.stock.table.nearestExpiry}
                      </span>
                      <span role="columnheader">{t.stock.table.status}</span>
                      <span role="columnheader">{t.stock.table.value}</span>
                      <span role="columnheader" aria-hidden="true" />
                    </div>

                    {visible.map((row) => {
                      const status = statusOf(row);

                      return (
                        <div
                          role="row"
                          key={row.material_id}
                          className="grid grid-cols-[minmax(0,1fr)_auto_auto] items-center gap-2 border-b border-border px-3.5 py-3 text-sm last:border-b-0 sm:grid-cols-[minmax(0,1fr)_110px_70px_62px_130px_96px_104px_40px] sm:gap-3 sm:px-4"
                        >
                          <div
                            role="cell"
                            className="flex min-w-0 flex-col gap-px"
                          >
                            <button
                              type="button"
                              className="cursor-pointer text-left text-sm font-semibold text-ink transition-colors duration-150 hover:text-brand"
                              onClick={() => setPanelId(row.material_id)}
                            >
                              {row.name}
                            </button>
                            <span className="truncate text-2xs text-ink-3">
                              {supplierName(
                                materialById.get(row.material_id)
                                  ?.default_supplier_id ?? null,
                              )}
                            </span>
                          </div>

                          <div
                            role="cell"
                            className="flex items-baseline justify-self-end gap-1 sm:justify-self-auto"
                          >
                            <strong className="text-sm font-bold tabular-nums">
                              {formatQty(row.on_hand, locale)}
                            </strong>
                            <span className="text-xs text-ink-3">
                              {row.base_unit.code}
                            </span>
                          </div>

                          <div
                            role="cell"
                            className="hidden text-xs text-ink-3 sm:block"
                          >
                            {formatQty(row.min_stock, locale)}{" "}
                            {row.base_unit.code}
                          </div>

                          <div
                            role="cell"
                            className="hidden text-xs text-ink-2 sm:block"
                          >
                            {row.active_batches}
                          </div>

                          <div
                            role="cell"
                            className="hidden text-xs text-ink-2 sm:block"
                          >
                            {expiryLabel(row)}
                          </div>

                          <div role="cell">
                            <Badge variant={status.tone}>{status.label}</Badge>
                          </div>

                          <div
                            role="cell"
                            className="hidden text-xs font-semibold tabular-nums sm:block"
                          >
                            {formatMoney(
                              Number(row.on_hand) * Number(row.last_cost),
                              locale,
                            )}
                          </div>

                          <div
                            role="cell"
                            className="hidden justify-self-end sm:block"
                          >
                            <Button
                              variant="ghost"
                              size="icon"
                              className="size-7 rounded-md"
                              type="button"
                              onClick={() => setPanelId(row.material_id)}
                              aria-label={t.stock.panel.title}
                            >
                              <Ellipsis size={15} />
                            </Button>
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

type Tone = "danger" | "warning" | "brand" | "neutral";

function Metric({
  label,
  tone,
  children,
}: {
  label: string;
  tone?: "danger" | "warning";
  children: React.ReactNode;
}) {
  return (
    <div className="flex flex-col gap-0.5 px-3 sm:border-l sm:border-border sm:px-4 sm:first:border-l-0">
      <span className="text-2xs font-semibold tracking-wide text-ink-3">
        {label}
      </span>
      <strong
        className={`text-base font-bold tabular-nums ${
          tone === "danger"
            ? "text-destructive"
            : tone === "warning"
              ? "text-warning"
              : ""
        }`}
      >
        {children}
      </strong>
    </div>
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
