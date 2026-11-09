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
      {session.status !== "ready" && (
        <main className="page">
          {session.status === "loading" ? (
            <p className="lede">{t.dashboard.loading}</p>
          ) : (
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
          )}
        </main>
      )}

      {session.status === "ready" && (
        <AppShell me={session.me} active="overview" onSignOut={signOut}>
          <div className="dash">
            <header className="dash__header">
              <div>
                <h1>{t.dashboard.title}</h1>
                <p className="dash__sub">
                  {fill(t.dashboard.subtitle, {
                    tenant: session.me.tenant.name,
                    date: formatLongDate(new Date(), locale),
                  })}
                </p>
              </div>
              <Link className="btn btn--ghost dash__open" href="/stock">
                {t.dashboard.openStock}
                <ChevronRight size={15} />
              </Link>
            </header>

            {load.status === "loading" && <Skeleton />}

            {load.status === "error" && (
              <div className="card">
                <p>{t.dashboard.error}</p>
                <button
                  className="btn btn--secondary"
                  type="button"
                  onClick={() => void fetchData()}
                >
                  {t.dashboard.retry}
                </button>
              </div>
            )}

            {data && data.summary.materials === 0 && (
              <div className="card dash__empty">
                <h2>{t.dashboard.emptyTenant.title}</h2>
                <p>{t.dashboard.emptyTenant.body}</p>
                <Link className="btn btn--primary" href="/onboarding">
                  {t.dashboard.emptyTenant.cta}
                </Link>
              </div>
            )}

            {data && data.summary.materials > 0 && (
              <>
                <section className="metrics" aria-label={t.dashboard.title}>
                  <article
                    className={`metric${data.summary.below_min > 0 ? " metric--danger" : ""}`}
                  >
                    <div className="metric__top">
                      <AlertTriangle size={14} />
                      <span>{t.dashboard.metrics.belowMin}</span>
                    </div>
                    <strong>{data.summary.below_min}</strong>
                    <span className="metric__unit">
                      {fill(t.dashboard.metrics.belowMinHint, {
                        count: data.summary.out_of_stock,
                      })}
                    </span>
                  </article>

                  <article
                    className={`metric${data.summary.expiring > 0 ? " metric--warning" : ""}`}
                  >
                    <div className="metric__top">
                      <Clock3 size={14} />
                      <span>
                        {fill(t.dashboard.metrics.expiring, {
                          days: data.summary.horizon_days,
                        })}
                      </span>
                    </div>
                    <strong>{data.summary.expiring}</strong>
                    <span className="metric__unit">
                      {t.dashboard.metrics.expiringUnit}
                    </span>
                  </article>

                  <article
                    className={`metric${data.summary.expired > 0 ? " metric--danger" : ""}`}
                  >
                    <div className="metric__top">
                      <CalendarX2 size={14} />
                      <span>{t.dashboard.metrics.expired}</span>
                    </div>
                    <strong>{data.summary.expired}</strong>
                    <span className="metric__unit">
                      {t.dashboard.metrics.expiredUnit}
                    </span>
                  </article>

                  <article className="metric">
                    <div className="metric__top">
                      <Wallet size={14} />
                      <span>{t.dashboard.metrics.value}</span>
                    </div>
                    <strong className="metric__money">
                      {formatMoney(Number(data.summary.stock_value), locale)}
                    </strong>
                    <span className="metric__unit">
                      {t.dashboard.metrics.valueUnit}
                    </span>
                  </article>
                </section>

                <section className="dash__grid">
                  <article className="panel">
                    <header className="panel__head">
                      <div>
                        <h2>{t.dashboard.critical.title}</h2>
                        <p>{t.dashboard.critical.subtitle}</p>
                      </div>
                      <Link className="panel__link" href="/stock">
                        {t.dashboard.critical.seeAll}
                        <ChevronRight size={13} />
                      </Link>
                    </header>

                    {data.critical.length === 0 ? (
                      <div className="panel__empty">
                        <CheckCircle2 size={18} />
                        <strong>{t.dashboard.critical.emptyTitle}</strong>
                        <span>{t.dashboard.critical.empty}</span>
                      </div>
                    ) : (
                      <ul className="dlist">
                        {data.critical.map((row) => {
                          const empty = Number(row.on_hand) === 0;

                          return (
                            <li key={row.material_id}>
                              <Link
                                className="drow"
                                href={`/stock?material=${row.material_id}`}
                              >
                                <div className="drow__main">
                                  <div className="drow__line">
                                    <strong>{row.name}</strong>
                                    <span
                                      className={`pill pill--${empty ? "critical" : "warning"}`}
                                    >
                                      {empty
                                        ? t.dashboard.critical.out
                                        : t.dashboard.critical.low}
                                    </span>
                                  </div>
                                  <span className="drow__meta">
                                    {fill(t.dashboard.critical.detail, {
                                      onHand: formatQty(row.on_hand, locale),
                                      min: formatQty(row.min_stock, locale),
                                      unit: row.base_unit.code,
                                    })}
                                  </span>
                                </div>
                                <div className="drow__side">
                                  <strong className="drow__amount">
                                    {fill(t.dashboard.critical.short, {
                                      qty: formatQty(row.shortfall, locale),
                                      unit: row.base_unit.code,
                                    })}
                                  </strong>
                                  {row.nearest_expiry && (
                                    <span className="drow__date">
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

                  <article className="panel">
                    <header className="panel__head">
                      <div>
                        <h2>{t.dashboard.expiringList.title}</h2>
                        <p>
                          {fill(t.dashboard.expiringList.subtitle, {
                            days: data.summary.horizon_days,
                          })}
                        </p>
                      </div>
                      <Link className="panel__link" href="/stock">
                        {t.dashboard.critical.seeAll}
                        <ChevronRight size={13} />
                      </Link>
                    </header>

                    {data.expiring.length === 0 ? (
                      <div className="panel__empty">
                        <CheckCircle2 size={18} />
                        <strong>{t.dashboard.expiringList.emptyTitle}</strong>
                        <span>
                          {fill(t.dashboard.expiringList.empty, {
                            days: data.summary.horizon_days,
                          })}
                        </span>
                      </div>
                    ) : (
                      <ul className="dlist">
                        {data.expiring.map((row) => (
                          <li key={row.batch_id}>
                            <Link
                              className="drow"
                              href={`/stock?material=${row.material_id}`}
                            >
                              <div className="drow__main">
                                <div className="drow__line">
                                  <strong>{row.material_name}</strong>
                                </div>
                                <span className="drow__meta">
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
                              <div className="drow__side">
                                <span
                                  className={`pill pill--${row.days_left < 0 ? "critical" : "warning"}`}
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
                                </span>
                                <span className="drow__date">
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

/** Placeholder shapes, so a slow connection does not flash an empty screen. */
function Skeleton() {
  return (
    <div className="dash__skeleton" aria-hidden="true">
      <div className="metrics">
        {[0, 1, 2, 3].map((index) => (
          <article className="metric" key={index}>
            <span className="skeleton skeleton--label" />
            <span className="skeleton skeleton--value" />
            <span className="skeleton skeleton--unit" />
          </article>
        ))}
      </div>
      <div className="dash__grid">
        {[0, 1].map((index) => (
          <article className="panel" key={index}>
            <header className="panel__head">
              <div>
                <span className="skeleton skeleton--label" />
                <span className="skeleton skeleton--unit" />
              </div>
            </header>
            {[0, 1, 2].map((row) => (
              <div className="drow" key={row}>
                <div className="drow__main">
                  <span className="skeleton skeleton--value" />
                  <span className="skeleton skeleton--unit" />
                </div>
              </div>
            ))}
          </article>
        ))}
      </div>
    </div>
  );
}
