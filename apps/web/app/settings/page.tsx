"use client";

import { ApiError, type Subscription } from "@takar/api-client";
import { useEffect, useState } from "react";

import {
  LoadingRegion,
  SkeletonForm,
  SkeletonList,
} from "@/components/skeletons";
import { api } from "@/lib/api";
import { formatDateTime, normalizeDecimal } from "@/lib/format";
import { fill, useI18n, type Dictionary } from "@/lib/i18n";
import { readToken } from "@/lib/session";
import { useMe } from "@/lib/use-me";

import { AppShell } from "../app-shell";

type Load = { status: "loading" } | { status: "ready" } | { status: "error" };

const MIN_WINDOW_DAYS = 1;
const MAX_WINDOW_DAYS = 365;

export default function SettingsPage() {
  const { t, locale } = useI18n();
  const { state: session, reload: reloadSession, signOut } = useMe();

  const [load, setLoad] = useState<Load>({ status: "loading" });
  const [rate, setRate] = useState("");
  const [windowDays, setWindowDays] = useState("");
  const [subscription, setSubscription] = useState<Subscription | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  useEffect(() => {
    if (session.status !== "ready") return;

    const token = readToken();

    if (!token) return;

    let active = true;

    Promise.all([api.getSettings(token), api.getSubscription(token)])
      .then(([settings, plan]) => {
        if (!active) return;
        setLoad({ status: "ready" });
        setRate(trimDecimal(settings.labor_rate_per_hour));
        setWindowDays(String(settings.usage_window_days));
        setSubscription(plan);
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

  const permissions = session.status === "ready" ? session.me.permissions : [];
  const canManage = permissions.includes("tenant_manage");

  async function save(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError(null);

    const token = readToken();

    if (!token) return;

    const value = rate.trim() === "" ? "0" : normalizeDecimal(rate);

    if (value === null || Number(value) < 0) {
      setError(t.settings.errors.invalidRate);
      return;
    }

    const days = Number(windowDays);

    if (
      !Number.isInteger(days) ||
      days < MIN_WINDOW_DAYS ||
      days > MAX_WINDOW_DAYS
    ) {
      setError(t.settings.errors.invalidWindow);
      return;
    }

    setSaving(true);

    try {
      const saved = await api.updateSettings(token, {
        labor_rate_per_hour: value,
        usage_window_days: days,
      });

      setRate(trimDecimal(saved.labor_rate_per_hour));
      setWindowDays(String(saved.usage_window_days));
      setNotice(t.settings.saved);
    } catch (cause) {
      if (cause instanceof ApiError && cause.code === "invalid_labor_rate") {
        setError(t.settings.errors.invalidRate);
      } else if (
        cause instanceof ApiError &&
        cause.code === "invalid_usage_window"
      ) {
        setError(t.settings.errors.invalidWindow);
      } else if (
        cause instanceof ApiError &&
        cause.code === "permission_denied"
      ) {
        setError(t.settings.errors.forbidden);
      } else {
        setError(t.settings.errors.generic);
      }
    } finally {
      setSaving(false);
    }
  }

  return (
    <>
      {session.status === "loading" && (
        <main className="page">
          <p className="lede">{t.settings.loading}</p>
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
        <AppShell me={session.me} active="settings" onSignOut={signOut}>
          <div className="settings">
            <header className="settings__header">
              <h1>{t.settings.title}</h1>
              <p className="settings__sub">{t.settings.subtitle}</p>
            </header>

            {notice && (
              <p className="notice" role="status">
                {notice}
              </p>
            )}

            {load.status === "loading" && (
              <LoadingRegion
                label={t.settings.loading}
                className="flex flex-col gap-4"
              >
                <SkeletonForm fields={2} />
                <SkeletonList rows={2} />
              </LoadingRegion>
            )}

            {load.status === "error" && (
              <div className="card">
                <p>{t.settings.error}</p>
                <button
                  className="btn btn--secondary"
                  type="button"
                  onClick={() => window.location.reload()}
                >
                  {t.settings.retry}
                </button>
              </div>
            )}

            {load.status === "ready" && (
              <form className="settings__form" onSubmit={save}>
                <section className="settings__card">
                  <div className="settings__intro">
                    <h2>{t.settings.labor.title}</h2>
                    <p>{t.settings.labor.body}</p>
                  </div>

                  <label className="field">
                    <span className="field__label">
                      {t.settings.labor.label}
                    </span>
                    <input
                      className="field__input"
                      type="text"
                      inputMode="decimal"
                      autoComplete="off"
                      placeholder="0"
                      value={rate}
                      onChange={(event) => setRate(event.target.value)}
                      readOnly={!canManage}
                      required
                    />
                    <span className="field__hint">
                      {canManage
                        ? t.settings.labor.hint
                        : t.settings.labor.readOnly}
                    </span>
                  </label>
                </section>

                <section className="settings__card">
                  <div className="settings__intro">
                    <h2>{t.settings.usage.title}</h2>
                    <p>{t.settings.usage.body}</p>
                  </div>

                  <label className="field">
                    <span className="field__label">
                      {t.settings.usage.label}
                    </span>
                    <input
                      className="field__input"
                      type="number"
                      min={MIN_WINDOW_DAYS}
                      max={MAX_WINDOW_DAYS}
                      step="1"
                      value={windowDays}
                      onChange={(event) => setWindowDays(event.target.value)}
                      readOnly={!canManage}
                      required
                    />
                    <span className="field__hint">
                      {canManage
                        ? t.settings.usage.hint
                        : t.settings.labor.readOnly}
                    </span>
                  </label>
                </section>

                {error && (
                  <p className="form__error" role="alert">
                    {error}
                  </p>
                )}

                {canManage && (
                  <div className="settings__actions">
                    <button
                      className="btn btn--primary"
                      type="submit"
                      disabled={saving}
                    >
                      {saving ? t.settings.saving : t.settings.save}
                    </button>
                  </div>
                )}
              </form>
            )}

            {load.status === "ready" && subscription && (
              <section className="settings__card plan">
                <div className="settings__intro">
                  <h2>{t.settings.plan.title}</h2>
                  <p>{t.settings.plan.hint}</p>
                </div>

                <div className="plan__head">
                  <span className="pill pill--safe">
                    {planName(t, subscription.plan)}
                  </span>
                  {subscription.trial_ends_at && (
                    <span className="plan__trial">
                      {fill(
                        isPast(subscription.trial_ends_at)
                          ? t.settings.plan.trialEnded
                          : t.settings.plan.trialEnds,
                        {
                          date: formatDateTime(
                            subscription.trial_ends_at,
                            locale,
                          ),
                        },
                      )}
                    </span>
                  )}
                </div>

                <dl className="plan__usage">
                  <Usage
                    label={t.settings.plan.users}
                    used={subscription.usage.users}
                    limit={subscription.limits.users}
                    of={t.settings.plan.of}
                    unlimited={t.settings.plan.unlimited}
                  />
                  <Usage
                    label={t.settings.plan.materials}
                    used={subscription.usage.materials}
                    limit={subscription.limits.materials}
                    of={t.settings.plan.of}
                    unlimited={t.settings.plan.unlimited}
                  />
                  <Usage
                    label={t.settings.plan.recipes}
                    used={subscription.usage.recipes}
                    limit={subscription.limits.recipes}
                    of={t.settings.plan.of}
                    unlimited={t.settings.plan.unlimited}
                  />
                </dl>
              </section>
            )}
          </div>
        </AppShell>
      )}
    </>
  );
}

type UsageProps = {
  label: string;
  used: number;
  limit: number | null;
  of: string;
  unlimited: string;
};

function Usage({ label, used, limit, of, unlimited }: UsageProps) {
  const full = limit !== null && used >= limit;

  return (
    <div className="plan__row">
      <dt>{label}</dt>
      <dd>
        <span>{limit === null ? fill(unlimited, { used }) : fill(of, { used, limit })}</span>
        {limit !== null && (
          <span
            className={full ? "plan__bar plan__bar--full" : "plan__bar"}
            aria-hidden="true"
          >
            <span
              style={{
                width: `${Math.min(100, Math.round((used / Math.max(limit, 1)) * 100))}%`,
              }}
            />
          </span>
        )}
      </dd>
    </div>
  );
}

function planName(t: Dictionary, plan: string): string {
  switch (plan) {
    case "trial":
      return t.settings.plan.trial;
    case "starter":
      return t.settings.plan.starter;
    case "pro":
      return t.settings.plan.pro;
    case "business":
      return t.settings.plan.business;
    default:
      return plan;
  }
}

function isPast(value: string): boolean {
  return new Date(value).getTime() < Date.now();
}

// Drops trailing zeros so the input shows "60000", not "60000.00".
function trimDecimal(value: string): string {
  return value.includes(".")
    ? value.replace(/0+$/, "").replace(/\.$/, "")
    : value;
}
