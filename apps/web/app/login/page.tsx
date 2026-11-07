"use client";

import { ApiError, type TenantSummary } from "@takar/api-client";
import { useRouter } from "next/navigation";
import { useEffect, useState } from "react";
import type { FormEvent } from "react";

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
    <main className="auth">
      <aside className="auth__brand">
        <div className="brand-top">
          <span className="brand-mark">T</span>
          <div className="brand-name">
            <strong>{t.common.appName}</strong>
            <span>{t.common.tagline}</span>
          </div>
        </div>

        <div className="brand-hero">
          <p className="brand-label">{t.login.brand.label}</p>
          <h1>{t.login.brand.title}</h1>
          <p className="brand-sub">{t.login.brand.subtitle}</p>
        </div>

        <ul className="brand-features">
          {[
            t.login.brand.features.batch,
            t.login.brand.features.hpp,
            t.login.brand.features.alerts,
          ].map((feature) => (
            <li className="brand-feature" key={feature.title}>
              <span className="brand-feature__icon" aria-hidden="true" />
              <span>
                <strong>{feature.title}</strong>
                <span>{feature.detail}</span>
              </span>
            </li>
          ))}
        </ul>

        <div className="brand-note">
          <div>
            <strong>{t.login.brand.noteTitle}</strong>
            <span>{t.login.brand.noteSubtitle}</span>
          </div>
          <span className="brand-note__version">{t.login.brand.version}</span>
        </div>
      </aside>

      <section className="auth__panel">
        <p className="auth__help">
          {t.login.help} <a href="mailto:halo@takar.id">{t.login.contactSupport}</a>
        </p>

        <div className="auth__card">
          <header className="auth__head">
            <p className="auth__eyebrow">{t.login.eyebrow}</p>
            <h1>
              {step === "tenant"
                ? t.login.tenantTitle
                : step === "code"
                  ? t.login.codeTitle
                  : isPhone
                    ? t.login.titlePhone
                    : t.login.titleEmail}
            </h1>
            <p className="auth__sub">
              {step === "tenant"
                ? t.login.tenantSubtitle
                : step === "code"
                  ? fill(t.login.codeSubtitle, { contact })
                  : isPhone
                    ? t.login.subtitlePhone
                    : t.login.subtitleEmail}
            </p>
          </header>

          <ol className="stepper">
            <li className={step === "contact" ? "stepper__step stepper__step--active" : "stepper__step stepper__step--done"}>
              <span className="stepper__dot">1</span>
              <span>{t.login.steps.contact}</span>
            </li>
            <li className="stepper__line" aria-hidden="true" />
            <li className={step === "contact" ? "stepper__step" : "stepper__step stepper__step--active"}>
              <span className="stepper__dot">2</span>
              <span>{t.login.steps.code}</span>
            </li>
          </ol>

          {step === "contact" && (
            <form className="auth__form" onSubmit={onSubmitContact}>
              <label className="field">
                <span className="field__label">
                  {isPhone ? t.login.phoneLabel : t.login.emailLabel}
                </span>
                <input
                  className="field__input"
                  value={contact}
                  onChange={(event) => setContact(event.target.value)}
                  placeholder={isPhone ? t.login.phonePlaceholder : t.login.emailPlaceholder}
                  type={isPhone ? "tel" : "email"}
                  inputMode={isPhone ? "tel" : "email"}
                  autoComplete={isPhone ? "tel" : "email"}
                  autoFocus
                  required
                />
              </label>

              <button className="btn btn--primary" type="submit" disabled={busy || !contact.trim()}>
                {busy ? t.login.sending : t.login.sendCode}
              </button>

              <button className="btn btn--ghost" type="button" onClick={switchMode}>
                {isPhone ? t.login.switchToEmail : t.login.switchToPhone}
              </button>
            </form>
          )}

          {step === "code" && (
            <form className="auth__form" onSubmit={onSubmitCode}>
              <div className="contact-chip">
                <span className="contact-chip__mark" aria-hidden="true">
                  ✓
                </span>
                <span className="contact-chip__body">
                  <strong>{contact}</strong>
                  <span>{isPhone ? t.login.phoneLabel : t.login.emailLabel}</span>
                </span>
                <button
                  className="link"
                  type="button"
                  onClick={() => {
                    setStep("contact");
                    setError(null);
                  }}
                >
                  {t.login.change}
                </button>
              </div>

              <div className="field">
                <span className="field__label">{t.login.codeLabel}</span>
                <OtpInput
                  value={code}
                  onChange={setCode}
                  label={t.login.codeLabel}
                  disabled={busy}
                />
              </div>

              <p className="auth__resend">
                {seconds > 0 ? (
                  fill(t.login.resendIn, { seconds })
                ) : (
                  <button className="link" type="button" onClick={() => void sendCode()}>
                    {t.login.resend}
                  </button>
                )}
              </p>

              <button
                className="btn btn--primary"
                type="submit"
                disabled={busy || code.some((digit) => !digit)}
              >
                {busy ? t.login.verifying : t.login.verify}
              </button>

              <p className="auth__or">{t.login.or}</p>

              <button className="btn btn--ghost" type="button" onClick={switchMode}>
                {isPhone ? t.login.switchToEmail : t.login.switchToPhone}
              </button>
            </form>
          )}

          {step === "tenant" && (
            <ul className="tenant-list">
              {tenants.map((tenant) => (
                <li key={tenant.id}>
                  <button
                    className="tenant"
                    type="button"
                    onClick={() => void chooseTenant(tenant.id)}
                    disabled={busy}
                  >
                    <span className="tenant__name">{tenant.name}</span>
                    <span className="tenant__role">{t.dashboard.roles[tenant.role]}</span>
                  </button>
                </li>
              ))}
            </ul>
          )}

          {pendingInvitation && (
            <p className="auth__notice" role="status">
              {t.login.invitationPending}
            </p>
          )}

          {error && (
            <p className="auth__error" role="alert">
              {error}
            </p>
          )}

          <p className="auth__terms">{t.login.terms}</p>
        </div>

        <footer className="auth__foot">
          <span>{t.login.foot.platforms}</span>
          <span>{t.login.foot.offline}</span>
          <LocaleSwitch />
        </footer>
      </section>
    </main>
  );
}
