"use client";

import type { Dashboard } from "@takar/api-client";
import {
  AlertTriangle,
  CalendarX2,
  CheckCircle2,
  ChevronRight,
  Clock3,
  Wallet,
} from "lucide-react";
import Link from "next/link";
import { useCallback, useEffect, useState } from "react";

import { LoadingRegion, SkeletonStats } from "@/components/skeletons";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { api } from "@/lib/api";
import {
  formatDate,
  formatLongDate,
  formatMoney,
  formatQty,
} from "@/lib/format";
import { fill, useI18n } from "@/lib/i18n";
import { readToken } from "@/lib/session";
import { useMe } from "@/lib/use-me";

import { AppShell } from "../app-shell";

type Load =
  | { status: "loading" }
  | { status: "ready"; data: Dashboard }
  | { status: "error" };

export default function DashboardPage() {
  const { t, locale } = useI18n();
  const { state: session, reload: reloadSession, signOut } = useMe();

  const [load, setLoad] = useState<Load>({ status: "loading" });

  // Starts before the session resolves: the token is already in storage, so
  // the overview and /me travel in parallel and the screen needs one round
  // trip, not two.
  const fetchData = useCallback(async () => {
    const token = readToken();

    if (!token) return;

    try {
      const data = await api.getDashboard(token);
      setLoad({ status: "ready", data });
    } catch {
      setLoad({ status: "error" });
    }
  }, []);

  useEffect(() => {
    void fetchData();
  }, [fetchData]);

  const data = load.status === "ready" ? load.data : null;

  return (
    <>
      {session.status === "loading" && (
        <AppShell active="overview">
          <div className="flex max-w-[1200px] flex-col gap-4">
            <DashboardSkeleton label={t.dashboard.loading} />
          </div>
        </AppShell>
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
        <AppShell me={session.me} active="overview" onSignOut={signOut}>
          <div className="flex max-w-[1200px] flex-col gap-4">
            <header className="flex flex-col items-start gap-4 sm:flex-row sm:items-end sm:justify-between">
              <div>
                <h1 className="text-xl font-extrabold tracking-tight sm:text-3xl">
                  {t.dashboard.title}
                </h1>
                <p className="mt-1 text-sm text-muted-foreground">
                  {fill(t.dashboard.subtitle, {
                    tenant: session.me.tenant.name,
                    date: formatLongDate(new Date(), locale),
                  })}
                </p>
              </div>
              <Button variant="ghost" asChild>
                <Link href="/stock">
                  {t.dashboard.openStock}
                  <ChevronRight size={15} />
                </Link>
              </Button>
            </header>

            {load.status === "loading" && (
              <DashboardSkeleton label={t.dashboard.loading} />
            )}

            {load.status === "error" && (
              <div className="flex flex-col gap-3 rounded-xl border border-border bg-card p-5">
                <p className="text-sm text-muted-foreground">
                  {t.dashboard.error}
                </p>
                <Button
                  variant="secondary"
                  size="sm"
                  className="self-start"
                  type="button"
                  onClick={() => void fetchData()}
                >
                  {t.dashboard.retry}
                </Button>
              </div>
            )}

            {data && data.summary.materials === 0 && (
              <div className="flex flex-col items-start gap-2 rounded-xl border border-border bg-card p-5">
                <h2 className="text-lg font-bold">
                  {t.dashboard.emptyTenant.title}
                </h2>
                <p className="max-w-[46ch] text-sm text-muted-foreground">
                  {t.dashboard.emptyTenant.body}
                </p>
                <Button asChild className="mt-1">
                  <Link href="/onboarding">{t.dashboard.emptyTenant.cta}</Link>
                </Button>
              </div>
            )}

            {data && data.summary.materials > 0 && (
              <>
                <section
                  aria-label={t.dashboard.title}
                  className="grid grid-cols-2 gap-3 lg:grid-cols-4"
                >
                  <Metric
                    icon={<AlertTriangle size={14} />}
                    label={t.dashboard.metrics.belowMin}
                    value={data.summary.below_min}
                    hint={fill(t.dashboard.metrics.belowMinHint, {
                      count: data.summary.out_of_stock,
                    })}
                    tone={data.summary.below_min > 0 ? "danger" : undefined}
                  />

                  <Metric
                    icon={<Clock3 size={14} />}
                    label={fill(t.dashboard.metrics.expiring, {
                      days: data.summary.horizon_days,
                    })}
                    value={data.summary.expiring}
                    hint={t.dashboard.metrics.expiringUnit}
                    tone={data.summary.expiring > 0 ? "warning" : undefined}
                  />

                  <Metric
                    icon={<CalendarX2 size={14} />}
                    label={t.dashboard.metrics.expired}
                    value={data.summary.expired}
                    hint={t.dashboard.metrics.expiredUnit}
                    tone={data.summary.expired > 0 ? "danger" : undefined}
                  />

                  <Metric
                    icon={<Wallet size={14} />}
                    label={t.dashboard.metrics.value}
                    value={formatMoney(Number(data.summary.stock_value), locale)}
                    hint={t.dashboard.metrics.valueUnit}
                    money
                  />
                </section>

                <section className="grid gap-3 lg:grid-cols-2">
                  <article className="flex flex-col overflow-hidden rounded-xl border border-border bg-card">
                    <header className="flex items-start justify-between gap-3 border-b border-border px-4 py-3">
                      <div>
                        <h2 className="text-base font-bold tracking-tight">
                          {t.dashboard.critical.title}
                        </h2>
                        <p className="mt-0.5 text-2xs text-ink-3">
                          {t.dashboard.critical.subtitle}
                        </p>
                      </div>
                      <PanelLink href="/stock">
                        {t.dashboard.critical.seeAll}
                      </PanelLink>
                    </header>

                    {data.critical.length === 0 ? (
                      <PanelEmpty
                        title={t.dashboard.critical.emptyTitle}
                        body={t.dashboard.critical.empty}
                      />
                    ) : (
                      <ul className="flex list-none flex-col py-1">
                        {data.critical.map((row) => {
                          const empty = Number(row.on_hand) === 0;

                          return (
                            <li
                              key={row.material_id}
                              className="border-t border-border first:border-t-0"
                            >
                              <Link
                                className="flex items-center justify-between gap-3 px-4 py-2.5 text-inherit no-underline transition-colors duration-150 hover:bg-surface-alt focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-brand"
                                href={`/stock?material=${row.material_id}`}
                              >
                                <div className="flex min-w-0 flex-col gap-0.5">
                                  <div className="flex min-w-0 items-center gap-2">
                                    <strong className="truncate text-sm font-semibold">
                                      {row.name}
                                    </strong>
                                    <Badge
                                      variant={empty ? "danger" : "warning"}
                                    >
                                      {empty
                                        ? t.dashboard.critical.out
                                        : t.dashboard.critical.low}
                                    </Badge>
                                  </div>
                                  <span className="text-2xs text-ink-3">
                                    {fill(t.dashboard.critical.detail, {
                                      onHand: formatQty(row.on_hand, locale),
                                      min: formatQty(row.min_stock, locale),
                                      unit: row.base_unit.code,
                                    })}
                                  </span>
                                </div>
                                <div className="flex flex-col items-end gap-0.5 text-right">
                                  <strong className="text-xs font-semibold whitespace-nowrap text-destructive">
                                    {fill(t.dashboard.critical.short, {
                                      qty: formatQty(row.shortfall, locale),
                                      unit: row.base_unit.code,
                                    })}
                                  </strong>
                                  {row.nearest_expiry && (
                                    <span className="text-2xs whitespace-nowrap text-ink-3">
                                      {formatDate(row.nearest_expiry, locale)}
                                    </span>
                                  )}
                                </div>
                              </Link>
                            </li>
                          );
                        })}
                      </ul>
                    )}
                  </article>

                  <article className="flex flex-col overflow-hidden rounded-xl border border-border bg-card">
                    <header className="flex items-start justify-between gap-3 border-b border-border px-4 py-3">
                      <div>
                        <h2 className="text-base font-bold tracking-tight">
                          {t.dashboard.expiringList.title}
                        </h2>
                        <p className="mt-0.5 text-2xs text-ink-3">
                          {fill(t.dashboard.expiringList.subtitle, {
                            days: data.summary.horizon_days,
                          })}
                        </p>
                      </div>
                      <PanelLink href="/stock">
                        {t.dashboard.critical.seeAll}
                      </PanelLink>
                    </header>

                    {data.expiring.length === 0 ? (
                      <PanelEmpty
                        title={t.dashboard.expiringList.emptyTitle}
                        body={fill(t.dashboard.expiringList.empty, {
                          days: data.summary.horizon_days,
                        })}
                      />
                    ) : (
                      <ul className="flex list-none flex-col py-1">
                        {data.expiring.map((row) => (
                          <li
                            key={row.batch_id}
                            className="border-t border-border first:border-t-0"
                          >
                            <Link
                              className="flex items-center justify-between gap-3 px-4 py-2.5 text-inherit no-underline transition-colors duration-150 hover:bg-surface-alt focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-brand"
                              href={`/stock?material=${row.material_id}`}
                            >
                              <div className="flex min-w-0 flex-col gap-0.5">
                                <div className="flex min-w-0 items-center gap-2">
                                  <strong className="truncate text-sm font-semibold">
                                    {row.material_name}
                                  </strong>
                                </div>
                                <span className="text-2xs text-ink-3">
                                  {fill(t.dashboard.expiringList.detail, {
                                    qty: formatQty(row.qty_remaining, locale),
                                    unit: row.base_unit.code,
                                    cost: formatMoney(
                                      Number(row.unit_cost),
                                      locale,
                                    ),
                                  })}
                                </span>
                              </div>
                              <div className="flex flex-col items-end gap-0.5 text-right">
                                <Badge
                                  variant={
                                    row.days_left < 0 ? "danger" : "warning"
                                  }
                                >
                                  {row.days_left < 0
                                    ? fill(t.dashboard.expiringList.overdue, {
                                        days: Math.abs(row.days_left),
                                      })
                                    : row.days_left === 0
                                      ? t.dashboard.expiringList.today
                                      : fill(t.dashboard.expiringList.daysLeft, {
                                          days: row.days_left,
                                        })}
                                </Badge>
                                <span className="text-2xs whitespace-nowrap text-ink-3">
                                  {formatDate(row.expires_at, locale)}
                                </span>
                              </div>
                            </Link>
                          </li>
                        ))}
                      </ul>
                    )}
                  </article>
                </section>
              </>
            )}
          </div>
        </AppShell>
      )}
    </>
  );
}

function Metric({
  icon,
  label,
  value,
  hint,
  tone,
  money,
}: {
  icon: React.ReactNode;
  label: string;
  value: React.ReactNode;
  hint: string;
  tone?: "danger" | "warning";
  money?: boolean;
}) {
  return (
    <article className="flex flex-col gap-0.5 rounded-xl border border-border bg-card px-3.5 py-3">
      <div className="flex items-center gap-1.5 text-2xs font-semibold leading-snug text-ink-3">
        {icon}
        <span>{label}</span>
      </div>
      <strong
        className={`font-extrabold tracking-tight tabular-nums ${
          money ? "text-xl" : "text-2xl"
        } ${
          tone === "danger"
            ? "text-destructive"
            : tone === "warning"
              ? "text-warning"
              : ""
        }`}
      >
        {value}
      </strong>
      <span className="text-2xs text-ink-3">{hint}</span>
    </article>
  );
}

function PanelLink({
  href,
  children,
}: {
  href: string;
  children: React.ReactNode;
}) {
  return (
    <Link
      href={href}
      className="inline-flex shrink-0 items-center gap-0.5 text-2xs font-semibold whitespace-nowrap text-brand hover:underline"
    >
      {children}
      <ChevronRight size={13} />
    </Link>
  );
}

function PanelEmpty({ title, body }: { title: string; body: string }) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-1.5 px-5 py-9 text-center text-ink-3">
      <CheckCircle2 size={18} className="text-brand" />
      <strong className="text-sm font-semibold text-ink">{title}</strong>
      <span className="max-w-[34ch] text-xs leading-relaxed">{body}</span>
    </div>
  );
}

/** Placeholder shapes, so a slow connection does not flash an empty screen. */
function DashboardSkeleton({ label }: { label: string }) {
  return (
    <LoadingRegion label={label} className="flex flex-col gap-4">
      <SkeletonStats count={4} />
      <div className="grid gap-3 lg:grid-cols-2">
        {[0, 1].map((panel) => (
          <div
            key={panel}
            className="flex flex-col gap-3 rounded-xl border border-border bg-card p-4"
          >
            <Skeleton className="h-3 w-36" />
            {[0, 1, 2].map((row) => (
              <div
                key={row}
                className="flex items-center justify-between gap-3 border-t border-border pt-3"
              >
                <div className="flex flex-col gap-1.5">
                  <Skeleton className="h-3 w-32" />
                  <Skeleton className="h-2.5 w-24" />
                </div>
                <Skeleton className="h-3 w-16" />
              </div>
            ))}
          </div>
        ))}
      </div>
    </LoadingRegion>
  );
}
