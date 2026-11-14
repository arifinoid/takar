"use client";

import type { RecommendationList } from "@takar/api-client";
import { ShoppingBasket } from "lucide-react";
import { useEffect, useState } from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { LoadingRegion, SkeletonList } from "@/components/skeletons";
import { api } from "@/lib/api";
import { formatMoney, formatQty } from "@/lib/format";
import { fill, useI18n } from "@/lib/i18n";
import { readToken } from "@/lib/session";
import { useMe } from "@/lib/use-me";

import { AppShell } from "../app-shell";

type Load =
  | { status: "loading" }
  | { status: "ready"; data: RecommendationList }
  | { status: "error" };

export default function PurchasingPage() {
  const { t, locale } = useI18n();
  const { state: session, reload: reloadSession, signOut } = useMe();

  const [load, setLoad] = useState<Load>({ status: "loading" });

  useEffect(() => {
    if (session.status !== "ready") return;

    const token = readToken();

    if (!token) return;

    let active = true;

    setLoad((current) =>
      current.status === "ready" ? current : { status: "loading" },
    );

    api
      .listRecommendations(token)
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
  const items = data?.items ?? [];
  const total = items.reduce(
    (sum, item) => sum + Number(item.estimated_cost),
    0,
  );

  return (
    <>
      {session.status === "loading" && (
        <AppShell active="purchasing">
          <div className="flex max-w-[1000px] flex-col gap-4">
            <LoadingRegion label={t.purchasing.loading}>
              <SkeletonList rows={4} />
            </LoadingRegion>
          </div>
        </AppShell>
      )}

      {session.status === "error" && (
        <main className="mx-auto max-w-[1000px] p-6">
          <Card className="gap-3 p-5">
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
          </Card>
        </main>
      )}

      {session.status === "ready" && (
        <AppShell me={session.me} active="purchasing" onSignOut={signOut}>
          <div className="flex max-w-[1000px] flex-col gap-4">
            <header>
              <h1 className="text-3xl font-extrabold tracking-tight">
                {t.purchasing.title}
              </h1>
              <p className="mt-1 text-sm text-muted-foreground">
                {data
                  ? fill(t.purchasing.subtitle, {
                      count: items.length,
                      days: data.window_days,
                    })
                  : t.purchasing.loading}
              </p>
            </header>

            {load.status === "loading" && (
              <LoadingRegion label={t.purchasing.loading}>
                <SkeletonList rows={4} />
              </LoadingRegion>
            )}

            {load.status === "error" && (
              <Card className="gap-3 p-5">
                <p className="text-sm text-muted-foreground">
                  {t.purchasing.error}
                </p>
                <Button
                  variant="secondary"
                  size="sm"
                  className="self-start"
                  type="button"
                  onClick={() => window.location.reload()}
                >
                  {t.purchasing.retry}
                </Button>
              </Card>
            )}

            {data && items.length === 0 && (
              <div className="flex flex-col items-center gap-2 rounded-xl border border-border bg-card px-5 py-10 text-center">
                <ShoppingBasket size={26} className="text-brand" />
                <h2 className="text-lg font-bold">{t.purchasing.empty.title}</h2>
                <p className="max-w-[46ch] text-sm text-muted-foreground">
                  {t.purchasing.empty.body}
                </p>
              </div>
            )}

            {data && items.length > 0 && (
              <>
                <section className="flex flex-wrap items-end justify-between gap-4 rounded-xl border border-border bg-brand-soft/60 px-4 py-4">
                  <div>
                    <span className="block text-2xs font-semibold tracking-wide text-brand uppercase">
                      {t.purchasing.estimated}
                    </span>
                    <strong className="text-2xl font-extrabold text-brand tabular-nums">
                      {formatMoney(total, locale)}
                    </strong>
                  </div>
                  <p className="max-w-[52ch] text-xs leading-relaxed text-muted-foreground">
                    {fill(t.purchasing.explain, { days: data.window_days })}
                  </p>
                </section>

                <div className="flex flex-col gap-2.5">
                  {items.map((item) => (
                    <article
                      className="flex flex-col gap-3 rounded-xl border border-border bg-card px-4 py-3.5"
                      key={item.material_id}
                    >
                      <div className="flex items-start justify-between gap-4">
                        <div className="flex min-w-0 flex-col gap-0.5">
                          <strong className="text-base font-bold">
                            {item.material_name}
                          </strong>
                          <span className="text-xs text-ink-3">
                            {item.supplier_name
                              ? fill(
                                  item.lead_time_days > 0
                                    ? t.purchasing.lead
                                    : t.purchasing.leadImmediate,
                                  {
                                    supplier: item.supplier_name,
                                    days: item.lead_time_days,
                                  },
                                )
                              : t.purchasing.noSupplier}
                          </span>
                        </div>

                        <div className="text-right">
                          <span className="block text-2xs tracking-wide text-ink-3 uppercase">
                            {t.purchasing.buy}
                          </span>
                          <strong className="text-xl font-extrabold tabular-nums">
                            {formatQty(item.recommended_qty, locale)}{" "}
                            {item.unit_code}
                          </strong>
                        </div>
                      </div>

                      <dl className="grid grid-cols-2 gap-2.5 sm:grid-cols-4">
                        <Fact
                          label={t.purchasing.onHand}
                          value={formatQty(item.on_hand, locale)}
                        />
                        <Fact
                          label={t.purchasing.target}
                          value={formatQty(item.target_qty, locale)}
                        />
                        <Fact
                          label={t.purchasing.dailyUsage}
                          value={formatQty(item.average_daily_usage, locale)}
                        />
                        <Fact
                          label={t.purchasing.estimated}
                          value={formatMoney(Number(item.estimated_cost), locale)}
                        />
                      </dl>

                      {(item.insufficient_history ||
                        item.capped_by_shelf_life ||
                        !item.supplier_name) && (
                        <div className="flex flex-wrap gap-1.5">
                          {item.insufficient_history && (
                            <Badge variant="warning">
                              {t.purchasing.flags.noHistory}
                            </Badge>
                          )}
                          {item.capped_by_shelf_life && (
                            <Badge>
                              {fill(t.purchasing.flags.shelfLife, {
                                days: item.shelf_life_days ?? 0,
                              })}
                            </Badge>
                          )}
                          {!item.supplier_name && (
                            <Badge>{t.purchasing.flags.noSupplier}</Badge>
                          )}
                        </div>
                      )}
                    </article>
                  ))}
                </div>
              </>
            )}
          </div>
        </AppShell>
      )}
    </>
  );
}

function Fact({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex flex-col gap-0.5 rounded-md bg-surface-alt px-2.5 py-2">
      <dt className="text-2xs tracking-wide text-ink-3 uppercase">{label}</dt>
      <dd className="text-sm font-bold tabular-nums">{value}</dd>
    </div>
  );
}
