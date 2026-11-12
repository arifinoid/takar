"use client";

import type { BusinessReport } from "@takar/api-client";
import { BarChart3 } from "lucide-react";
import { useEffect, useState } from "react";

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
        <main className="page">
          <p className="lede">{t.reports.loading}</p>
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
        <AppShell me={session.me} active="reports" onSignOut={signOut}>
          <div className="reports">
            <header className="reports__header">
              <div>
                <h1>{t.reports.title}</h1>
                <p className="reports__sub">
                  {data
                    ? fill(t.reports.subtitle, { days: data.window_days })
                    : t.reports.loading}
                </p>
              </div>
            </header>

            {load.status === "loading" && (
              <p className="lede">{t.reports.loading}</p>
            )}

            {load.status === "error" && (
              <div className="card">
                <p>{t.reports.error}</p>
                <button
                  className="btn btn--secondary"
                  type="button"
                  onClick={() => window.location.reload()}
                >
                  {t.reports.retry}
                </button>
              </div>
            )}

            {data && (
              <>
                <section className="reports__cards">
                  <article className="report-card">
                    <span className="report-card__label">
                      {t.reports.stock.title}
                    </span>
                    <strong className="report-card__value">
                      {formatMoney(Number(data.stock.total), locale)}
                    </strong>

                    <dl className="report-card__split">
                      <div>
                        <dt>{t.reports.stock.materials}</dt>
                        <dd>
                          {formatMoney(Number(data.stock.materials), locale)}
                        </dd>
                      </div>
                      <div>
                        <dt>{t.reports.stock.products}</dt>
                        <dd>
                          {formatMoney(Number(data.stock.products), locale)}
                        </dd>
                      </div>
                    </dl>
                  </article>

                  <article className="report-card">
                    <span className="report-card__label">
                      {t.reports.waste.title}
                    </span>
                    <strong className="report-card__value">
                      {formatMoney(Number(data.waste.value), locale)}
                    </strong>
                    <p className="report-card__hint">
                      {fill(t.reports.waste.movements, {
                        count: data.waste.movements,
                      })}
                    </p>

                    {data.waste.by_material.length > 0 ? (
                      <ul className="report-card__list">
                        {data.waste.by_material.map((row) => (
                          <li key={row.material_id}>
                            <span>{row.material_name}</span>
                            <span>
                              {formatQty(row.qty, locale)} {row.unit_code}
                            </span>
                            <strong>
                              {formatMoney(Number(row.value), locale)}
                            </strong>
                          </li>
                        ))}
                      </ul>
                    ) : (
                      <p className="report-card__hint">
                        {t.reports.waste.empty}
                      </p>
                    )}
                  </article>
                </section>

                <section className="reports__products">
                  <div className="reports__products-head">
                    <h2>{t.reports.products.title}</h2>
                    <p>{t.reports.products.costNote}</p>
                  </div>

                  {data.products.length === 0 ? (
                    <div className="reports__empty">
                      <BarChart3 size={24} />
                      <p>{t.reports.products.empty}</p>
                    </div>
                  ) : (
                    <div className="report-table__wrap">
                      <table className="report-table">
                        <thead>
                          <tr>
                            <th>{t.reports.products.product}</th>
                            <th>{t.reports.products.price}</th>
                            <th>{t.reports.products.cost}</th>
                            <th>{t.reports.products.unitMargin}</th>
                            <th>{t.reports.products.sold}</th>
                            <th>{t.reports.products.revenue}</th>
                            <th>{t.reports.products.margin}</th>
                          </tr>
                        </thead>
                        <tbody>
                          {data.products.map((product) => (
                            <tr key={product.product_id}>
                              <td>
                                <strong>{product.name}</strong>
                                {product.cost_incomplete && (
                                  <span className="pill pill--warning">
                                    {t.reports.products.incomplete}
                                  </span>
                                )}
                              </td>
                              <td>
                                {formatMoney(Number(product.sell_price), locale)}
                              </td>
                              <td>
                                {product.unit_cost ? (
                                  formatMoney(Number(product.unit_cost), locale)
                                ) : (
                                  <span className="pill pill--neutral">
                                    {t.reports.products.noCost}
                                  </span>
                                )}
                              </td>
                              <td>
                                {product.margin_per_unit ? (
                                  <span className="report-table__margin">
                                    {formatMoney(
                                      Number(product.margin_per_unit),
                                      locale,
                                    )}
                                    {product.margin_pct && (
                                      <span className="report-table__pct">
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
                              </td>
                              <td>{formatQty(product.sold_qty, locale)}</td>
                              <td>
                                {formatMoney(Number(product.revenue), locale)}
                              </td>
                              <td>
                                {product.margin
                                  ? formatMoney(Number(product.margin), locale)
                                  : "—"}
                              </td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
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
