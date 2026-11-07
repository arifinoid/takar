"use client";

import { ApiError, type MeResponse } from "@takar/api-client";
import { useRouter } from "next/navigation";
import { useCallback, useEffect, useState } from "react";

import { api } from "@/lib/api";
import { fill, useI18n } from "@/lib/i18n";
import { clearToken, readToken } from "@/lib/session";

type State =
  | { status: "loading" }
  | { status: "ready"; me: MeResponse }
  | { status: "error" };

export default function DashboardPage() {
  const { t } = useI18n();
  const router = useRouter();
  const [state, setState] = useState<State>({ status: "loading" });

  const load = useCallback(async () => {
    const token = readToken();

    if (!token) {
      router.replace("/login");
      return;
    }

    setState({ status: "loading" });

    try {
      const me = await api.me(token);
      setState({ status: "ready", me });
    } catch (cause) {
      if (cause instanceof ApiError && (cause.status === 401 || cause.status === 403)) {
        clearToken();
        router.replace("/login");
        return;
      }

      setState({ status: "error" });
    }
  }, [router]);

  useEffect(() => {
    void load();
  }, [load]);

  function signOut() {
    clearToken();
    router.replace("/login");
  }

  return (
    <main className="page">
      <header className="header">
        <div>
          <p className="eyebrow">{t.common.tagline}</p>
          <h1>{state.status === "ready" ? state.me.tenant.name : t.common.appName}</h1>
        </div>
        <div className="header__aside">
          {state.status === "ready" && (
            <button className="btn btn--ghost" type="button" onClick={signOut}>
              {t.dashboard.signOut}
            </button>
          )}
        </div>
      </header>

      {state.status === "loading" && <p className="lede">{t.dashboard.loading}</p>}

      {state.status === "error" && (
        <div className="card">
          <p>{t.dashboard.error}</p>
          <button className="btn btn--secondary" type="button" onClick={() => void load()}>
            {t.dashboard.retry}
          </button>
        </div>
      )}

      {state.status === "ready" && (
        <section className="dash">
          <article className="card">
            <h2>{fill(t.dashboard.greeting, { name: state.me.user.name })}</h2>
            <dl className="dash__facts">
              <div>
                <dt>{t.dashboard.tenant}</dt>
                <dd>{state.me.tenant.name}</dd>
              </div>
              <div>
                <dt>{t.dashboard.role}</dt>
                <dd>{t.dashboard.roles[state.me.role]}</dd>
              </div>
            </dl>
          </article>

          <article className="card">
            <h2>{t.dashboard.permissions}</h2>
            <p className="dash__count">
              {fill(t.dashboard.permissionsCount, { count: state.me.permissions.length })}
            </p>
            <ul className="chips">
              {state.me.permissions.map((permission) => (
                <li className="chip" key={permission}>
                  {permission}
                </li>
              ))}
            </ul>
          </article>
        </section>
      )}
    </main>
  );
}
