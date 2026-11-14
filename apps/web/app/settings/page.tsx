"use client";

import {
  ApiError,
  type Access,
  type Billing,
  type Subscription,
} from "@takar/api-client";
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
import { Select } from "@/components/ui/select";
import { api } from "@/lib/api";
import {
  formatDate,
  formatDateTime,
  formatMoney,
  normalizeDecimal,
} from "@/lib/format";
import { fill, useI18n, type Dictionary } from "@/lib/i18n";
import { readToken } from "@/lib/session";
import { useMe } from "@/lib/use-me";
import { cn } from "@/lib/utils";

import { AppShell } from "../app-shell";

type Load = { status: "loading" } | { status: "ready" } | { status: "error" };

const MIN_WINDOW_DAYS = 1;
const MAX_WINDOW_DAYS = 365;

const MIN_MONTHS = 1;
const MAX_MONTHS = 12;

const PAYMENT_METHODS = ["transfer", "qris", "cash", "lainnya"] as const;

export default function SettingsPage() {
  const { t, locale } = useI18n();
  const { state: session, reload: reloadSession, signOut } = useMe();

  const [load, setLoad] = useState<Load>({ status: "loading" });
  const [rate, setRate] = useState("");
  const [windowDays, setWindowDays] = useState("");
  const [subscription, setSubscription] = useState<Subscription | null>(null);
  const [billing, setBilling] = useState<Billing | null>(null);
  const [payPlan, setPayPlan] = useState("starter");
  const [payMonths, setPayMonths] = useState("1");
  const [payMethod, setPayMethod] = useState<string>("transfer");
  const [payReference, setPayReference] = useState("");
  const [paying, setPaying] = useState(false);
  const [paymentError, setPaymentError] = useState<string | null>(null);
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

  // Billing is owner-only, and a failure here must not take the screen down:
  // the card simply stays hidden and the rest of the settings still work.
  useEffect(() => {
    if (session.status !== "ready") return;
    if (!session.me.permissions.includes("billing_manage")) return;

    const token = readToken();
    if (!token) return;

    let active = true;

    api
      .getBilling(token)
      .then((data) => {
        if (active) setBilling(data);
      })
      .catch(() => {});

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

  const months = Number(payMonths) || 0;
  const planPrice = billing?.plans.find((plan) => plan.id === payPlan)?.price ?? 0;
  const total = planPrice * months;

  async function recordPayment(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();

    const token = readToken();
    if (!token || paying) return;

    if (
      !Number.isInteger(months) ||
      months < MIN_MONTHS ||
      months > MAX_MONTHS
    ) {
      setPaymentError(t.settings.billing.errors.invalidMonths);
      return;
    }

    setPaying(true);
    setPaymentError(null);

    try {
      const payment = await api.createPayment(token, {
        plan: payPlan,
        method: payMethod,
        months,
        reference: payReference.trim() || undefined,
      });

      setBilling((current) =>
        current
          ? { ...current, payments: [payment, ...current.payments] }
          : current,
      );
      setPayReference("");
      setNotice(t.settings.billing.recorded);
    } catch {
      setPaymentError(t.settings.billing.failed);
    } finally {
      setPaying(false);
    }
  }

  async function withdraw(id: string) {
    const token = readToken();
    if (!token) return;

    setPaymentError(null);

    try {
      await api.deletePayment(token, id);
      setBilling((current) =>
        current
          ? {
              ...current,
              payments: current.payments.filter((payment) => payment.id !== id),
            }
          : current,
      );
    } catch {
      setPaymentError(t.settings.billing.withdrawFailed);
    }
  }

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

                {subscription.access === "read_only" && (
                  <p className="rounded-lg bg-warning-soft px-3 py-2.5 text-xs text-warning">
                    {t.settings.billing.readOnlyHint}
                  </p>
                )}

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

            {billing && (
              <section className="flex max-w-[520px] flex-col gap-4 rounded-xl border border-border bg-card p-4.5">
                <Intro
                  title={t.settings.billing.title}
                  body={t.settings.billing.body}
                />

                <div className="flex flex-wrap items-center gap-2.5">
                  <Badge
                    variant={accessVariant(billing.access)}
                    className="px-2.5 py-1 text-xs"
                  >
                    {t.settings.billing.access[billing.access]}
                  </Badge>
                  <span className="text-xs text-ink-3">
                    {billing.ends_at
                      ? fill(t.settings.billing.endsAt, {
                          date: formatDateTime(billing.ends_at, locale),
                        })
                      : t.settings.billing.noEnd}
                  </span>
                </div>

                {billing.access === "read_only" && (
                  <p className="rounded-lg bg-warning-soft px-3 py-2.5 text-xs text-warning">
                    {t.settings.billing.readOnlyHint}
                  </p>
                )}

                <form className="flex flex-col gap-3" onSubmit={recordPayment}>
                  <div className="grid gap-3 sm:grid-cols-2">
                    <div className="flex flex-col gap-1.5">
                      <Label htmlFor="billing-plan">
                        {t.settings.billing.plan}
                      </Label>
                      <Select
                        id="billing-plan"
                        value={payPlan}
                        onChange={(event) => setPayPlan(event.target.value)}
                      >
                        {billing.plans.map((option) => (
                          <option key={option.id} value={option.id}>
                            {planName(t, option.id)} ·{" "}
                            {formatMoney(option.price, locale)}
                          </option>
                        ))}
                      </Select>
                    </div>

                    <div className="flex flex-col gap-1.5">
                      <Label htmlFor="billing-months">
                        {t.settings.billing.months}
                      </Label>
                      <Input
                        id="billing-months"
                        type="number"
                        min={MIN_MONTHS}
                        max={MAX_MONTHS}
                        step="1"
                        value={payMonths}
                        onChange={(event) => setPayMonths(event.target.value)}
                        required
                      />
                    </div>

                    <div className="flex flex-col gap-1.5">
                      <Label htmlFor="billing-method">
                        {t.settings.billing.method}
                      </Label>
                      <Select
                        id="billing-method"
                        value={payMethod}
                        onChange={(event) => setPayMethod(event.target.value)}
                      >
                        {PAYMENT_METHODS.map((method) => (
                          <option key={method} value={method}>
                            {t.settings.billing.methods[method]}
                          </option>
                        ))}
                      </Select>
                    </div>

                    <div className="flex flex-col gap-1.5">
                      <Label htmlFor="billing-reference">
                        {t.settings.billing.reference}
                      </Label>
                      <Input
                        id="billing-reference"
                        value={payReference}
                        onChange={(event) => setPayReference(event.target.value)}
                        placeholder={t.settings.billing.referenceHint}
                        autoComplete="off"
                      />
                    </div>
                  </div>

                  {paymentError && (
                    <p
                      role="alert"
                      className="rounded-lg bg-danger-soft px-3 py-2.5 text-xs text-destructive"
                    >
                      {paymentError}
                    </p>
                  )}

                  <div className="flex flex-wrap items-center justify-between gap-3">
                    <span className="text-sm font-bold tabular-nums">
                      {t.settings.billing.total}: {formatMoney(total, locale)}
                    </span>
                    <Button type="submit" disabled={paying}>
                      {paying
                        ? t.settings.billing.recording
                        : t.settings.billing.record}
                    </Button>
                  </div>
                </form>

                {billing.payments.length > 0 && (
                  <ul className="flex flex-col gap-2">
                    {billing.payments.map((payment) => (
                      <li
                        key={payment.id}
                        className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-border bg-surface-alt px-3 py-2"
                      >
                        <div className="flex flex-col">
                          <span className="text-sm font-bold tabular-nums">
                            {formatMoney(Number(payment.amount), locale)}
                          </span>
                          <span className="text-[10.5px] text-ink-3">
                            {planName(t, payment.plan)} ·{" "}
                            {formatDate(payment.period_start, locale)} –{" "}
                            {formatDate(payment.period_end, locale)} ·{" "}
                            {methodName(t, payment.method)}
                          </span>
                        </div>
                        <div className="flex items-center gap-2">
                          <Badge variant={statusVariant(payment.status)}>
                            {t.settings.billing.status[payment.status]}
                          </Badge>
                          {payment.status === "pending" && (
                            <Button
                              variant="ghost"
                              size="sm"
                              type="button"
                              onClick={() => void withdraw(payment.id)}
                            >
                              {t.settings.billing.withdraw}
                            </Button>
                          )}
                        </div>
                      </li>
                    ))}
                  </ul>
                )}
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

function accessVariant(access: Access): "brand" | "warning" | "danger" {
  if (access === "active") return "brand";
  if (access === "grace") return "warning";

  return "danger";
}

function statusVariant(
  status: "pending" | "verified" | "rejected",
): "neutral" | "brand" | "danger" {
  if (status === "verified") return "brand";
  if (status === "rejected") return "danger";

  return "neutral";
}

function methodName(t: Dictionary, method: string): string {
  const methods = t.settings.billing.methods as Record<string, string | undefined>;

  return methods[method] ?? method;
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
