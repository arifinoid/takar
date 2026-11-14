"use client";

import { ApiError, type TenantSummary } from "@takar/api-client";
import { Check } from "lucide-react";
import { useRouter } from "next/navigation";
import { useEffect, useState } from "react";
import type { FormEvent } from "react";

import { Button } from "@/components/ui/button";
import { Field, FormError } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { api } from "@/lib/api";
import { fill, useI18n } from "@/lib/i18n";
import { writeToken } from "@/lib/session";

import { LocaleSwitch } from "../locale-switch";
import { OTP_LENGTH, OtpInput } from "./otp-input";

type Step = "contact" | "code" | "tenant";
type ContactMode = "phone" | "email";

const EMPTY_CODE = Array.from({ length: OTP_LENGTH }, () => "");
const RESEND_SECONDS = 60;

export default function LoginPage() {
  const { t } = useI18n();
  const router = useRouter();

  const [step, setStep] = useState<Step>("contact");
  const [mode, setMode] = useState<ContactMode>("phone");
  const [contact, setContact] = useState("");
  const [code, setCode] = useState<string[]>(EMPTY_CODE);
  const [tenants, setTenants] = useState<TenantSummary[]>([]);
  const [selectionToken, setSelectionToken] = useState<string | null>(null);
  const [pendingInvitation, setPendingInvitation] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [seconds, setSeconds] = useState(0);

  useEffect(() => {
    if (seconds <= 0) {
      return;
    }

    const timer = window.setTimeout(() => setSeconds((value) => value - 1), 1000);
    return () => window.clearTimeout(timer);
  }, [seconds]);

  function messageFor(cause: unknown): string {
    if (cause instanceof ApiError) {
      switch (cause.code) {
        case "invalid_email":
        case "invalid_phone":
        case "contact_required":
          return t.login.errors.invalidContact;
        case "invalid_code":
          return t.login.errors.invalidCode;
        case "too_many_attempts":
          return t.login.errors.tooManyAttempts;
        case "rate_limited":
          return t.login.errors.rateLimited;
      }
    }

    return t.login.errors.generic;
  }

  async function sendCode() {
    if (!contact.trim()) {
      return;
    }

    setBusy(true);
    setError(null);

    try {
      await api.requestOtp(contact.trim());
      setCode(EMPTY_CODE);
      setStep("code");
      setSeconds(RESEND_SECONDS);
    } catch (cause) {
      setError(messageFor(cause));
    } finally {
      setBusy(false);
    }
  }

  async function verify() {
    if (code.some((digit) => !digit)) {
      return;
    }

    setBusy(true);
    setError(null);

    try {
      const result = await api.verifyOtp({
        contact: contact.trim(),
        code: code.join(""),
      });

      if (result.tenants.length > 1) {
        setTenants(result.tenants);
        setSelectionToken(result.access_token);
        setStep("tenant");
        return;
      }

      if (result.tenants.length === 1) {
        writeToken(result.access_token);
        router.replace("/dashboard");
        return;
      }

      setPendingInvitation(true);
    } catch (cause) {
      setError(messageFor(cause));
    } finally {
      setBusy(false);
    }
  }

  async function chooseTenant(tenantId: string) {
    if (!selectionToken) {
      return;
    }

    setBusy(true);
    setError(null);

    try {
      const result = await api.selectTenant(selectionToken, tenantId);
      writeToken(result.access_token);
      router.replace("/dashboard");
    } catch (cause) {
      setError(messageFor(cause));
    } finally {
      setBusy(false);
    }
  }

  function switchMode() {
    setMode(mode === "phone" ? "email" : "phone");
    setStep("contact");
    setError(null);
  }

  function onSubmitContact(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    void sendCode();
  }

  function onSubmitCode(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    void verify();
  }

  const isPhone = mode === "phone";

  return (
    <main className="grid min-h-dvh lg:grid-cols-[772fr_668fr]">
      <aside className="hidden flex-col justify-between gap-7 bg-brand-dark p-[52px] text-white lg:flex">
        <div className="flex items-center gap-2.5">
          <span className="flex size-[34px] items-center justify-center rounded-lg bg-white text-[17px] font-extrabold text-brand-dark">
            T
          </span>
          <div className="flex flex-col leading-[1.2]">
            <strong className="text-lg font-bold">{t.common.appName}</strong>
            <span className="text-[10.5px] font-semibold text-[#8fb3a6]">
              {t.common.tagline}
            </span>
          </div>
        </div>

        <div className="flex flex-col gap-4">
          <p className="text-[10px] font-bold tracking-[0.1em] text-[#7fc0a5]">
            {t.login.brand.label}
          </p>
          <h1 className="max-w-[15ch] text-[42px] leading-[1.08] font-bold tracking-[-0.02em]">
            {t.login.brand.title}
          </h1>
          <p className="max-w-[56ch] text-[13px] leading-[1.6] text-[#a9c7bc]">
            {t.login.brand.subtitle}
          </p>
        </div>

        <ul className="flex flex-col gap-3.5 pt-1.5">
          {[
            t.login.brand.features.batch,
            t.login.brand.features.hpp,
            t.login.brand.features.alerts,
          ].map((feature) => (
            <li className="flex items-start gap-3" key={feature.title}>
              <span className="flex size-8 flex-none items-center justify-center rounded-[9px] bg-[#1b4a3e] text-[#7fc0a5]">
                <Check size={13} strokeWidth={3} aria-hidden="true" />
              </span>
              <span>
                <strong className="block text-sm font-bold">{feature.title}</strong>
                <span className="text-[11px] leading-[1.5] text-[#a9c7bc]">
                  {feature.detail}
                </span>
              </span>
            </li>
          ))}
        </ul>

        <div className="flex items-center justify-between gap-3 rounded-xl bg-[#143a2f] p-4">
          <div>
            <strong className="block text-[11px] font-bold">
              {t.login.brand.noteTitle}
            </strong>
            <span className="text-[10px] text-[#8fb3a6]">
              {t.login.brand.noteSubtitle}
            </span>
          </div>
          <span className="text-[9.5px] font-semibold text-[#5f8a7a]">
            {t.login.brand.version}
          </span>
        </div>
      </aside>

      <section className="flex flex-col gap-6 bg-surface px-5 py-6 sm:px-10 lg:px-14 lg:py-10">
        <p className="self-center text-[11px] text-ink-3 lg:self-end">
          {t.login.help}{" "}
          <a
            className="font-semibold text-brand no-underline hover:underline"
            href="mailto:halo@takar.id"
          >
            {t.login.contactSupport}
          </a>
        </p>

        <div className="m-auto flex w-full max-w-[392px] flex-col gap-4 rounded-2xl border border-border p-[26px]">
          <header className="flex flex-col gap-1.5">
            <p className="text-[9.5px] font-bold tracking-[0.12em] text-ink-3">
              {t.login.eyebrow}
            </p>
            <h1 className="text-[21px] leading-[1.25] font-bold tracking-[-0.01em]">
              {step === "tenant"
                ? t.login.tenantTitle
                : step === "code"
                  ? t.login.codeTitle
                  : isPhone
                    ? t.login.titlePhone
                    : t.login.titleEmail}
            </h1>
            <p className="text-[11.5px] leading-[1.55] text-ink-3">
              {step === "tenant"
                ? t.login.tenantSubtitle
                : step === "code"
                  ? fill(t.login.codeSubtitle, { contact })
                  : isPhone
                    ? t.login.subtitlePhone
                    : t.login.subtitleEmail}
            </p>
          </header>

          <ol className="flex items-center gap-2">
            <li
              className={
                step === "contact"
                  ? "flex items-center gap-1.5 text-[10.5px] font-semibold text-ink"
                  : "flex items-center gap-1.5 text-[10.5px] font-semibold text-ink-3"
              }
            >
              <span
                className={
                  step === "contact"
                    ? "flex size-4 items-center justify-center rounded-full bg-brand text-[8.5px] font-bold text-white"
                    : "flex size-4 items-center justify-center rounded-full bg-brand-soft text-[8.5px] font-bold text-brand"
                }
              >
                1
              </span>
              <span>{t.login.steps.contact}</span>
            </li>
            <li className="h-px w-5 bg-brand" aria-hidden="true" />
            <li
              className={
                step === "contact"
                  ? "flex items-center gap-1.5 text-[10.5px] font-semibold text-ink-3"
                  : "flex items-center gap-1.5 text-[10.5px] font-semibold text-ink"
              }
            >
              <span
                className={
                  step === "contact"
                    ? "flex size-4 items-center justify-center rounded-full bg-brand-soft text-[8.5px] font-bold text-brand"
                    : "flex size-4 items-center justify-center rounded-full bg-brand text-[8.5px] font-bold text-white"
                }
              >
                2
              </span>
              <span>{t.login.steps.code}</span>
            </li>
          </ol>

          {step === "contact" && (
            <form className="flex flex-col gap-3" onSubmit={onSubmitContact}>
              <Field label={isPhone ? t.login.phoneLabel : t.login.emailLabel}>
                <Input
                  value={contact}
                  onChange={(event) => setContact(event.target.value)}
                  placeholder={isPhone ? t.login.phonePlaceholder : t.login.emailPlaceholder}
                  type={isPhone ? "tel" : "email"}
                  inputMode={isPhone ? "tel" : "email"}
                  autoComplete={isPhone ? "tel" : "email"}
                  autoFocus
                  required
                />
              </Field>

              <Button type="submit" className="w-full" disabled={busy || !contact.trim()}>
                {busy ? t.login.sending : t.login.sendCode}
              </Button>

              <Button variant="ghost" className="w-full" type="button" onClick={switchMode}>
                {isPhone ? t.login.switchToEmail : t.login.switchToPhone}
              </Button>
            </form>
          )}

          {step === "code" && (
            <form className="flex flex-col gap-3" onSubmit={onSubmitCode}>
              <div className="flex items-center gap-2.5 rounded-lg border border-border bg-surface-alt px-3 py-2.5">
                <span className="flex size-6 flex-none items-center justify-center rounded-full bg-brand-soft text-brand">
                  <Check size={12} strokeWidth={3} aria-hidden="true" />
                </span>
                <span className="flex min-w-0 flex-col gap-px">
                  <strong className="truncate text-sm font-bold">{contact}</strong>
                  <span className="text-[9.5px] text-ink-3">
                    {isPhone ? t.login.phoneLabel : t.login.emailLabel}
                  </span>
                </span>
                <button
                  className="ml-auto p-0 font-semibold text-brand hover:underline"
                  type="button"
                  onClick={() => {
                    setStep("contact");
                    setError(null);
                  }}
                >
                  {t.login.change}
                </button>
              </div>

              <div className="flex flex-col gap-1.5">
                <span className="text-xs font-semibold text-muted-foreground select-none">
                  {t.login.codeLabel}
                </span>
                <OtpInput
                  value={code}
                  onChange={setCode}
                  label={t.login.codeLabel}
                  disabled={busy}
                />
              </div>

              <p className="text-center text-[10.5px] text-ink-3">
                {seconds > 0 ? (
                  fill(t.login.resendIn, { seconds })
                ) : (
                  <button
                    className="p-0 font-semibold text-brand hover:underline"
                    type="button"
                    onClick={() => void sendCode()}
                  >
                    {t.login.resend}
                  </button>
                )}
              </p>

              <Button
                type="submit"
                className="w-full"
                disabled={busy || code.some((digit) => !digit)}
              >
                {busy ? t.login.verifying : t.login.verify}
              </Button>

              <p className="text-center text-[10.5px] text-ink-3">{t.login.or}</p>

              <Button variant="ghost" className="w-full" type="button" onClick={switchMode}>
                {isPhone ? t.login.switchToEmail : t.login.switchToPhone}
              </Button>
            </form>
          )}

          {step === "tenant" && (
            <ul className="flex flex-col gap-2">
              {tenants.map((tenant) => (
                <li key={tenant.id}>
                  <button
                    className="flex w-full items-center justify-between gap-3 rounded-lg border border-border bg-surface px-3.5 py-3.5 text-left transition-colors hover:border-brand disabled:opacity-55"
                    type="button"
                    onClick={() => void chooseTenant(tenant.id)}
                    disabled={busy}
                  >
                    <span className="text-sm font-bold">{tenant.name}</span>
                    <span className="text-[10.5px] text-ink-3">
                      {t.dashboard.roles[tenant.role]}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          )}

          {pendingInvitation && (
            <p
              className="rounded-lg bg-warning-soft px-3 py-2.5 text-xs leading-[1.45] text-warning"
              role="status"
            >
              {t.login.invitationPending}
            </p>
          )}

          {error && <FormError>{error}</FormError>}

          <p className="text-[10px] leading-[1.5] text-ink-3">{t.login.terms}</p>
        </div>

        <footer className="mx-auto flex w-full max-w-[392px] items-center justify-between gap-3.5 text-[10px] text-ink-3">
          <span>{t.login.foot.platforms}</span>
          <span>{t.login.foot.offline}</span>
          <LocaleSwitch />
        </footer>
      </section>
    </main>
  );
}
