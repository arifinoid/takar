"use client";

import { ApiError, type Subscription } from "@takar/api-client";
import { useEffect, useState } from "react";

import {
  LoadingRegion,
  SkeletonForm,
  SkeletonList,
} from "@/components/skeletons";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { api } from "@/lib/api";
import { formatDateTime, normalizeDecimal } from "@/lib/format";
import { fill, useI18n, type Dictionary } from "@/lib/i18n";
import { readToken } from "@/lib/session";
import { useMe } from "@/lib/use-me";
import { cn } from "@/lib/utils";

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
        <AppShell active="settings">
          <div className="flex max-w-[720px] flex-col gap-4">
            <LoadingRegion
              label={t.settings.loading}
              className="flex flex-col gap-4"
            >
              <SkeletonForm fields={2} />
              <SkeletonList rows={2} />
            </LoadingRegion>
          </div>
        </AppShell>
      )}

      {session.status === "error" && (
        <main className="mx-auto max-w-[720px] p-6">
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
        <AppShell me={session.me} active="settings" onSignOut={signOut}>
          <div className="flex max-w-[720px] flex-col gap-4">
            <header>
              <h1 className="text-xl font-extrabold tracking-tight sm:text-3xl">
                {t.settings.title}
              </h1>
              <p className="mt-1 text-sm text-muted-foreground">
                {t.settings.subtitle}
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
                label={t.settings.loading}
                className="flex flex-col gap-4"
              >
                <SkeletonForm fields={2} />
                <SkeletonList rows={2} />
              </LoadingRegion>
            )}

            {load.status === "error" && (
              <div className="flex flex-col gap-3 rounded-xl border border-border bg-card p-5">
                <p className="text-sm text-muted-foreground">
                  {t.settings.error}
                </p>
                <Button
                  variant="secondary"
                  size="sm"
                  className="self-start"
                  type="button"
                  onClick={() => window.location.reload()}
                >
                  {t.settings.retry}
                </Button>
              </div>
            )}

            {load.status === "ready" && (
              <form className="flex flex-col gap-4" onSubmit={save}>
                <section className="flex flex-col gap-4 rounded-xl border border-border bg-card p-4.5">
                  <Intro title={t.settings.labor.title} body={t.settings.labor.body} />

                  <div className="flex max-w-[320px] flex-col gap-1.5">
                    <Label htmlFor="settings-rate">
                      {t.settings.labor.label}
                    </Label>
                    <Input
                      id="settings-rate"
                      type="text"
                      inputMode="decimal"
                      autoComplete="off"
                      placeholder="0"
                      value={rate}
                      onChange={(event) => setRate(event.target.value)}
                      readOnly={!canManage}
                      required
                    />
                    <span className="text-2xs leading-snug text-ink-3">
                      {canManage
                        ? t.settings.labor.hint
                        : t.settings.labor.readOnly}
                    </span>
                  </div>
                </section>

                <section className="flex flex-col gap-4 rounded-xl border border-border bg-card p-4.5">
                  <Intro
                    title={t.settings.usage.title}
                    body={t.settings.usage.body}
                  />

                  <div className="flex max-w-[320px] flex-col gap-1.5">
                    <Label htmlFor="settings-window">
                      {t.settings.usage.label}
                    </Label>
                    <Input
                      id="settings-window"
                      type="number"
                      min={MIN_WINDOW_DAYS}
                      max={MAX_WINDOW_DAYS}
                      step="1"
                      value={windowDays}
                      onChange={(event) => setWindowDays(event.target.value)}
                      readOnly={!canManage}
                      required
                    />
                    <span className="text-2xs leading-snug text-ink-3">
                      {canManage
                        ? t.settings.usage.hint
                        : t.settings.labor.readOnly}
                    </span>
                  </div>
                </section>

                {error && (
                  <p
                    role="alert"
                    className="rounded-lg bg-danger-soft px-3 py-2.5 text-xs text-destructive"
                  >
                    {error}
                  </p>
                )}

                {canManage && (
                  <div className="flex justify-end">
                    <Button type="submit" disabled={saving}>
                      {saving ? t.settings.saving : t.settings.save}
                    </Button>
                  </div>
                )}
              </form>
            )}

            {load.status === "ready" && subscription && (
              <section className="flex max-w-[520px] flex-col gap-4 rounded-xl border border-border bg-card p-4.5">
                <Intro
                  title={t.settings.plan.title}
                  body={t.settings.plan.hint}
                />

                <div className="flex flex-col items-start gap-2.5 sm:flex-row sm:items-center">
                  <Badge variant="brand" className="px-2.5 py-1 text-xs">
                    {planName(t, subscription.plan)}
                  </Badge>
                  {subscription.trial_ends_at && (
                    <span className="text-xs text-ink-3">
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

                <dl className="flex flex-col gap-2">
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

function Intro({ title, body }: { title: string; body: string }) {
  return (
    <div>
      <h2 className="text-base font-bold">{title}</h2>
      <p className="mt-1.5 max-w-[60ch] text-sm leading-relaxed text-muted-foreground">
        {body}
      </p>
    </div>
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
    <div className="grid grid-cols-[110px_minmax(0,1fr)] items-center gap-2.5">
      <dt className="text-xs text-ink-2">{label}</dt>
      <dd className="m-0 flex items-center gap-2.5">
        <span className="min-w-[72px] text-xs tabular-nums">
          {limit === null ? fill(unlimited, { used }) : fill(of, { used, limit })}
        </span>
        {limit !== null && (
          <span
            className="block h-1.5 flex-1 overflow-hidden rounded-full bg-surface-alt"
            aria-hidden="true"
          >
            <span
              className={cn("block h-full", full ? "bg-warning" : "bg-brand")}
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
