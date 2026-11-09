"use client";

import type { OnboardingResult, OnboardingTemplate } from "@takar/api-client";
import { ArrowRight, CheckCircle2, Download, Upload } from "lucide-react";
import Link from "next/link";
import { useCallback, useEffect, useRef, useState } from "react";

import { api } from "@/lib/api";
import { fill, useI18n, type Dictionary } from "@/lib/i18n";
import { readToken } from "@/lib/session";
import { useMe } from "@/lib/use-me";

import { AppShell } from "../app-shell";

/** Matches MAX_IMPORT_ROWS on the server. */
const MAX_ROWS = 500;

const TEMPLATE_IDS = ["bakery", "coffee", "rice_bowl", "dessert"] as const;
type TemplateId = (typeof TEMPLATE_IDS)[number];

function isTemplateId(id: string): id is TemplateId {
  return (TEMPLATE_IDS as readonly string[]).includes(id);
}

function templateCopy(t: Dictionary, id: string) {
  if (isTemplateId(id)) {
    return t.onboarding.templates[id];
  }

  return { name: id, description: "" };
}

export default function OnboardingPage() {
  const { t } = useI18n();
  const { state: session, reload: reloadSession, signOut } = useMe();

  const [templates, setTemplates] = useState<OnboardingTemplate[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [result, setResult] = useState<OnboardingResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const input = useRef<HTMLInputElement>(null);

  const token = useCallback(() => readToken(), []);

  useEffect(() => {
    if (session.status !== "ready") return;

    const stored = token();
    if (!stored) return;

    let cancelled = false;

    void api
      .listOnboardingTemplates(stored)
      .then((rows) => {
        if (!cancelled) setTemplates(rows);
      })
      .catch(() => {
        if (!cancelled) setTemplates([]);
      });

    return () => {
      cancelled = true;
    };
  }, [session.status, token]);

  const permissions = session.status === "ready" ? session.me.permissions : [];
  const canWrite = permissions.includes("material_write");

  async function applyTemplate(id: string) {
    const stored = token();
    if (!stored || busy) return;

    setBusy(id);
    setError(null);
    setResult(null);

    try {
      setResult(await api.applyOnboardingTemplate(stored, id));
    } catch {
      setError(t.onboarding.import.failed);
    } finally {
      setBusy(null);
    }
  }

  async function upload(file: File) {
    const stored = token();
    if (!stored || busy) return;

    setBusy("upload");
    setError(null);
    setResult(null);

    try {
      const bytes = new Uint8Array(await file.arrayBuffer());
      const format = file.name.toLowerCase().endsWith(".csv") ? "csv" : "xlsx";

      setResult(await api.importMaterials(stored, bytes, format));
    } catch {
      setError(t.onboarding.import.failed);
    } finally {
      setBusy(null);

      if (input.current) {
        input.current.value = "";
      }
    }
  }

  function downloadSample() {
    const blob = new Blob([t.onboarding.import.sampleFile], {
      type: "text/csv;charset=utf-8",
    });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");

    anchor.href = url;
    anchor.download = "takar-bahan.csv";
    anchor.click();
    URL.revokeObjectURL(url);
  }

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
          <div className="onboarding">
            <header className="dash__header">
              <div>
                <h1>{t.onboarding.title}</h1>
                <p className="dash__sub">{t.onboarding.subtitle}</p>
              </div>
              <Link className="btn btn--ghost dash__open" href="/stock">
                {t.onboarding.openStock}
                <ArrowRight size={15} />
              </Link>
            </header>

            {!canWrite && <p className="notice">{t.onboarding.noPermission}</p>}

            {error && (
              <p className="form__error" role="alert">
                {error}
              </p>
            )}

            {result && <Result result={result} />}

            <section className="panel">
              <header className="panel__head">
                <div>
                  <h2>{t.onboarding.templates.title}</h2>
                  <p>{t.onboarding.templates.subtitle}</p>
                </div>
              </header>

              <div className="tpl-grid">
                {(templates ?? []).map((template) => {
                  const copy = templateCopy(t, template.id);

                  return (
                    <article className="tpl" key={template.id}>
                      <strong className="tpl__name">{copy.name}</strong>
                      <p className="tpl__desc">{copy.description}</p>
                      <span className="tpl__count">
                        {fill(t.onboarding.templates.items, {
                          count: template.item_count,
                        })}
                      </span>
                      <button
                        className="btn btn--secondary tpl__apply"
                        type="button"
                        disabled={!canWrite || busy !== null}
                        onClick={() => void applyTemplate(template.id)}
                      >
                        {busy === template.id
                          ? t.onboarding.templates.applying
                          : t.onboarding.templates.apply}
                      </button>
                    </article>
                  );
                })}
              </div>
            </section>

            <section className="panel">
              <header className="panel__head">
                <div>
                  <h2>{t.onboarding.import.title}</h2>
                  <p>{t.onboarding.import.subtitle}</p>
                </div>
              </header>

              <div className="import">
                <code className="import__columns">{t.onboarding.import.columns}</code>
                <p className="import__hint">
                  {fill(t.onboarding.import.accepted, { max: MAX_ROWS })}
                </p>

                <div className="import__actions">
                  <input
                    ref={input}
                    className="import__file"
                    type="file"
                    accept=".xlsx,.xls,.ods,.csv"
                    onChange={(event) => {
                      const file = event.target.files?.[0];
                      if (file) void upload(file);
                    }}
                  />
                  <button
                    className="btn btn--primary"
                    type="button"
                    disabled={!canWrite || busy !== null}
                    onClick={() => input.current?.click()}
                  >
                    <Upload size={15} />
                    {busy === "upload"
                      ? t.onboarding.import.uploading
                      : t.onboarding.import.pick}
                  </button>
                  <button
                    className="btn btn--ghost"
                    type="button"
                    onClick={downloadSample}
                  >
                    <Download size={15} />
                    {t.onboarding.import.sample}
                  </button>
                </div>
              </div>
            </section>
          </div>
        </AppShell>
      )}
    </>
  );
}

function Result({ result }: { result: OnboardingResult }) {
  const { t } = useI18n();

  return (
    <div className="result" role="status">
      <div className="result__summary">
        <CheckCircle2 size={16} />
        <strong>
          {fill(t.onboarding.result.created, { count: result.created })}
        </strong>
        {result.skipped > 0 && (
          <span>{fill(t.onboarding.result.skipped, { count: result.skipped })}</span>
        )}
        {result.errors.length > 0 && (
          <span>{fill(t.onboarding.result.errors, { count: result.errors.length })}</span>
        )}
      </div>

      {result.errors.length > 0 && (
        <ul className="result__errors">
          {result.errors.map((error) => (
            <li key={error.row}>
              <span>{fill(t.onboarding.import.rowError, { row: error.row })}</span>
              {messageFor(t, error.code)}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function messageFor(t: Dictionary, code: string): string {
  const errors = t.onboarding.import.errors as Record<string, string | undefined>;

  return errors[code] ?? t.onboarding.import.errors.generic;
}
