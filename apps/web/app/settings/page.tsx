"use client";

import { ApiError } from "@takar/api-client";
import { useEffect, useState } from "react";

import { api } from "@/lib/api";
import { normalizeDecimal } from "@/lib/format";
import { useI18n } from "@/lib/i18n";
import { readToken } from "@/lib/session";
import { useMe } from "@/lib/use-me";

import { AppShell } from "../app-shell";

type Load = { status: "loading" } | { status: "ready" } | { status: "error" };

export default function SettingsPage() {
  const { t } = useI18n();
  const { state: session, reload: reloadSession, signOut } = useMe();

  const [load, setLoad] = useState<Load>({ status: "loading" });
  const [rate, setRate] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  useEffect(() => {
    if (session.status !== "ready") return;

    const token = readToken();

    if (!token) return;

    let active = true;

    api
      .getSettings(token)
      .then((settings) => {
        if (!active) return;
        setLoad({ status: "ready" });
        setRate(trimDecimal(settings.labor_rate_per_hour));
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

    setSaving(true);

    try {
      const saved = await api.updateSettings(token, {
        labor_rate_per_hour: value,
      });

      setRate(trimDecimal(saved.labor_rate_per_hour));
      setNotice(t.settings.saved);
    } catch (cause) {
      if (cause instanceof ApiError && cause.code === "invalid_labor_rate") {
        setError(t.settings.errors.invalidRate);
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
              <p className="lede">{t.settings.loading}</p>
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
              <section className="settings__card">
                <div className="settings__intro">
                  <h2>{t.settings.labor.title}</h2>
                  <p>{t.settings.labor.body}</p>
                </div>

                <form className="settings__form" onSubmit={save}>
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
                        {saving ? t.settings.labor.saving : t.settings.labor.save}
                      </button>
                    </div>
                  )}
                </form>
              </section>
            )}
          </div>
        </AppShell>
      )}
    </>
  );
}

// Drops trailing zeros so the input shows "60000", not "60000.00".
function trimDecimal(value: string): string {
  return value.includes(".")
    ? value.replace(/0+$/, "").replace(/\.$/, "")
    : value;
}
