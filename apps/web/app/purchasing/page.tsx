"use client";

import type { RecommendationList } from "@takar/api-client";
import { ShoppingBasket } from "lucide-react";
import { useEffect, useState } from "react";

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
        <main className="page">
          <p className="lede">{t.purchasing.loading}</p>
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
        <AppShell me={session.me} active="purchasing" onSignOut={signOut}>
          <div className="purchasing">
            <header className="purchasing__header">
              <div>
                <h1>{t.purchasing.title}</h1>
                <p className="purchasing__sub">
                  {data
                    ? fill(t.purchasing.subtitle, {
                        count: items.length,
                        days: data.window_days,
                      })
                    : t.purchasing.loading}
                </p>
              </div>
            </header>

            {load.status === "loading" && (
              <p className="lede">{t.purchasing.loading}</p>
            )}

            {load.status === "error" && (
              <div className="card">
                <p>{t.purchasing.error}</p>
                <button
                  className="btn btn--secondary"
                  type="button"
                  onClick={() => window.location.reload()}
                >
                  {t.purchasing.retry}
                </button>
              </div>
            )}

            {data && items.length === 0 && (
              <div className="purchasing__empty">
                <ShoppingBasket size={26} />
                <h2>{t.purchasing.empty.title}</h2>
                <p>{t.purchasing.empty.body}</p>
              </div>
            )}

            {data && items.length > 0 && (
              <>
                <section className="purchasing__summary">
                  <div>
                    <span>{t.purchasing.estimated}</span>
                    <strong>{formatMoney(total, locale)}</strong>
                  </div>
                  <p>
                    {fill(t.purchasing.explain, { days: data.window_days })}
                  </p>
                </section>

                <div className="buy-list">
                  {items.map((item) => (
                    <article className="buy" key={item.material_id}>
                      <div className="buy__head">
                        <div className="buy__name">
                          <strong>{item.material_name}</strong>
                          <span>
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

                        <div className="buy__qty">
                          <span>{t.purchasing.buy}</span>
                          <strong>
                            {formatQty(item.recommended_qty, locale)}{" "}
                            {item.unit_code}
                          </strong>
                        </div>
                      </div>

                      <div className="buy__facts">
                        <div>
                          <span>{t.purchasing.onHand}</span>
                          <strong>{formatQty(item.on_hand, locale)}</strong>
                        </div>
                        <div>
                          <span>{t.purchasing.target}</span>
                          <strong>{formatQty(item.target_qty, locale)}</strong>
                        </div>
                        <div>
                          <span>{t.purchasing.dailyUsage}</span>
                          <strong>
                            {formatQty(item.average_daily_usage, locale)}
                          </strong>
                        </div>
                        <div>
                          <span>{t.purchasing.estimated}</span>
                          <strong>
                            {formatMoney(Number(item.estimated_cost), locale)}
                          </strong>
                        </div>
                      </div>

                      {(item.insufficient_history ||
                        item.capped_by_shelf_life ||
                        !item.supplier_name) && (
                        <div className="buy__flags">
                          {item.insufficient_history && (
                            <span className="pill pill--warning">
                              {t.purchasing.flags.noHistory}
                            </span>
                          )}
                          {item.capped_by_shelf_life && (
                            <span className="pill pill--neutral">
                              {fill(t.purchasing.flags.shelfLife, {
                                days: item.shelf_life_days ?? 0,
                              })}
                            </span>
                          )}
                          {!item.supplier_name && (
                            <span className="pill pill--neutral">
                              {t.purchasing.flags.noSupplier}
                            </span>
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
