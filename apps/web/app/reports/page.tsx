"use client";

import type { BusinessReport } from "@takar/api-client";
import { BarChart3 } from "lucide-react";
import { useEffect, useState } from "react";

import {
  LoadingRegion,
  SkeletonList,
  SkeletonStats,
} from "@/components/skeletons";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { api } from "@/lib/api";
import {
  formatMoney,
  formatPercent,
  formatQty,
} from "@/lib/format";
import { fill, useI18n } from "@/lib/i18n";
import { readToken } from "@/lib/session";
import { useMe } from "@/lib/use-me";

import { AppShell } from "../app-shell";

type Load =
  | { status: "loading" }
  | { status: "ready"; data: BusinessReport }
  | { status: "error" };

export default function ReportsPage() {
  const { t, locale } = useI18n();
  const { state: session, reload: reloadSession, signOut } = useMe();

  const [load, setLoad] = useState<Load>({ status: "loading" });

  useEffect(() => {
    if (session.status !== "ready") return;

    const token = readToken();

    if (!token) return;

    let active = true;

    api
      .getBusinessReport(token)
      .then((data) => {
        if (active) setLoad({ status: "ready", data });
      })
      .catch(() => {
        if (active) setLoad({ status: "error" });
      });

    return () => {
      active = false;
    };
  }, [session.status]);

  const data = load.status === "ready" ? load.data : null;

  return (
    <>
      {session.status === "loading" && (
        <AppShell active="reports">
          <div className="flex max-w-[1100px] flex-col gap-4">
            <LoadingRegion
              label={t.reports.loading}
              className="flex flex-col gap-4"
            >
              <SkeletonStats count={2} />
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
        <AppShell me={session.me} active="reports" onSignOut={signOut}>
          <div className="flex max-w-[1100px] flex-col gap-4">
            <header>
              <h1 className="text-xl font-extrabold tracking-tight sm:text-3xl">
                {t.reports.title}
              </h1>
              <p className="mt-1 text-sm text-muted-foreground">
                {data
                  ? fill(t.reports.subtitle, { days: data.window_days })
                  : t.reports.loading}
              </p>
            </header>

            {load.status === "loading" && (
              <LoadingRegion
                label={t.reports.loading}
                className="flex flex-col gap-4"
              >
                <SkeletonStats count={2} />
                <SkeletonList rows={3} />
              </LoadingRegion>
            )}

            {load.status === "error" && (
              <div className="flex flex-col gap-3 rounded-xl border border-border bg-card p-5">
                <p className="text-sm text-muted-foreground">
                  {t.reports.error}
                </p>
                <Button
                  variant="secondary"
                  size="sm"
                  className="self-start"
                  type="button"
                  onClick={() => window.location.reload()}
                >
                  {t.reports.retry}
                </Button>
              </div>
            )}

            {data && (
              <>
                <section className="grid gap-3 lg:grid-cols-2">
                  <article className="flex flex-col gap-1.5 rounded-xl border border-border bg-card px-4.5 py-4">
                    <span className="text-2xs font-semibold tracking-wide text-ink-3 uppercase">
                      {t.reports.stock.title}
                    </span>
                    <strong className="text-2xl font-extrabold tracking-tight tabular-nums">
                      {formatMoney(Number(data.stock.total), locale)}
                    </strong>

                    <dl className="mt-1.5 flex gap-5.5">
                      <div className="flex flex-col gap-px">
                        <dt className="text-2xs text-ink-3">
                          {t.reports.stock.materials}
                        </dt>
                        <dd className="m-0 text-base tabular-nums">
                          {formatMoney(Number(data.stock.materials), locale)}
                        </dd>
                      </div>
                      <div className="flex flex-col gap-px">
                        <dt className="text-2xs text-ink-3">
                          {t.reports.stock.products}
                        </dt>
                        <dd className="m-0 text-base tabular-nums">
                          {formatMoney(Number(data.stock.products), locale)}
                        </dd>
                      </div>
                    </dl>
                  </article>

                  <article className="flex flex-col gap-1.5 rounded-xl border border-border bg-card px-4.5 py-4">
                    <span className="text-2xs font-semibold tracking-wide text-ink-3 uppercase">
                      {t.reports.waste.title}
                    </span>
                    <strong className="text-2xl font-extrabold tracking-tight tabular-nums">
                      {formatMoney(Number(data.waste.value), locale)}
                    </strong>
                    <p className="text-xs text-ink-3">
                      {fill(t.reports.waste.movements, {
                        count: data.waste.movements,
                      })}
                    </p>

                    {data.waste.by_material.length > 0 ? (
                      <ul className="mt-1.5 flex list-none flex-col gap-1 p-0">
                        {data.waste.by_material.map((row) => (
                          <li
                            key={row.material_id}
                            className="grid grid-cols-[minmax(0,1fr)_auto_90px] items-baseline gap-2.5 border-t border-border pt-1 text-sm"
                          >
                            <span className="truncate">{row.material_name}</span>
                            <span className="text-xs text-ink-3 tabular-nums">
                              {formatQty(row.qty, locale)} {row.unit_code}
                            </span>
                            <strong className="text-right tabular-nums">
                              {formatMoney(Number(row.value), locale)}
                            </strong>
                          </li>
                        ))}
                      </ul>
                    ) : (
                      <p className="text-xs text-ink-3">
                        {t.reports.waste.empty}
                      </p>
                    )}
                  </article>
                </section>

                <section className="flex flex-col gap-2.5">
                  <div>
                    <h2 className="text-base font-bold">
                      {t.reports.products.title}
                    </h2>
                    <p className="mt-0.5 max-w-[72ch] text-xs leading-relaxed text-ink-3">
                      {t.reports.products.costNote}
                    </p>
                  </div>

                  {data.products.length === 0 ? (
                    <div className="flex flex-col items-center gap-2 rounded-xl border border-border bg-card px-5 py-9 text-center text-ink-3">
                      <BarChart3 size={24} />
                      <p className="text-sm">{t.reports.products.empty}</p>
                    </div>
                  ) : (
                    <div className="overflow-hidden rounded-xl border border-border bg-card">
                      <Table className="min-w-[720px]">
                        <TableHeader>
                          <TableRow className="border-border">
                            <TableHead>{t.reports.products.product}</TableHead>
                            <TableHead className="text-right">
                              {t.reports.products.price}
                            </TableHead>
                            <TableHead className="text-right">
                              {t.reports.products.cost}
                            </TableHead>
                            <TableHead className="text-right">
                              {t.reports.products.unitMargin}
                            </TableHead>
                            <TableHead className="text-right">
                              {t.reports.products.sold}
                            </TableHead>
                            <TableHead className="text-right">
                              {t.reports.products.revenue}
                            </TableHead>
                            <TableHead className="text-right">
                              {t.reports.products.margin}
                            </TableHead>
                          </TableRow>
                        </TableHeader>
                        <TableBody>
                          {data.products.map((product) => (
                            <TableRow key={product.product_id}>
                              <TableCell className="whitespace-normal">
                                <span className="inline-flex flex-wrap items-center gap-2">
                                  <strong className="text-sm font-semibold">
                                    {product.name}
                                  </strong>
                                  {product.cost_incomplete && (
                                    <Badge variant="warning">
                                      {t.reports.products.incomplete}
                                    </Badge>
                                  )}
                                </span>
                              </TableCell>
                              <TableCell className="text-right tabular-nums">
                                {formatMoney(Number(product.sell_price), locale)}
                              </TableCell>
                              <TableCell className="text-right tabular-nums">
                                {product.unit_cost ? (
                                  formatMoney(Number(product.unit_cost), locale)
                                ) : (
                                  <Badge>
                                    {t.reports.products.noCost}
                                  </Badge>
                                )}
                              </TableCell>
                              <TableCell className="text-right tabular-nums">
                                {product.margin_per_unit ? (
                                  <span className="inline-flex items-baseline justify-end gap-1.5">
                                    {formatMoney(
                                      Number(product.margin_per_unit),
                                      locale,
                                    )}
                                    {product.margin_pct && (
                                      <span className="text-2xs text-ink-3">
                                        {formatPercent(
                                          product.margin_pct,
                                          locale,
                                        )}
                                      </span>
                                    )}
                                  </span>
                                ) : (
                                  "—"
                                )}
                              </TableCell>
                              <TableCell className="text-right tabular-nums">
                                {formatQty(product.sold_qty, locale)}
                              </TableCell>
                              <TableCell className="text-right tabular-nums">
                                {formatMoney(Number(product.revenue), locale)}
                              </TableCell>
                              <TableCell className="text-right tabular-nums">
                                {product.margin
                                  ? formatMoney(Number(product.margin), locale)
                                  : "—"}
                              </TableCell>
                            </TableRow>
                          ))}
                        </TableBody>
                      </Table>
                    </div>
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
