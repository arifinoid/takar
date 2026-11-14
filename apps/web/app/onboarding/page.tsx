"use client";

import type { OnboardingResult, OnboardingTemplate } from "@takar/api-client";
import { ArrowRight, CheckCircle2, Download, Upload } from "lucide-react";
import Link from "next/link";
import { useCallback, useEffect, useRef, useState } from "react";

import {
  LoadingRegion,
  SkeletonList,
} from "@/components/skeletons";
import { Button } from "@/components/ui/button";
import { FormError } from "@/components/ui/field";
import { Skeleton } from "@/components/ui/skeleton";
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
        <main className="mx-auto max-w-[1000px] p-6">
          {session.status === "loading" ? (
            <LoadingRegion
              label={t.dashboard.loading}
              className="flex flex-col gap-4"
            >
              <SkeletonList rows={4} />
            </LoadingRegion>
          ) : (
            <div className="flex flex-col gap-3 rounded-xl border border-border bg-card p-5">
              <p className="text-sm text-muted-foreground">
                {t.dashboard.error}
              </p>
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
          )}
        </main>
      )}

      {session.status === "ready" && (
        <AppShell me={session.me} active="overview" onSignOut={signOut}>
          <div className="flex max-w-[1000px] flex-col gap-4">
            <header className="flex flex-col items-start gap-4 sm:flex-row sm:items-end sm:justify-between">
              <div>
                <h1 className="text-xl font-extrabold tracking-tight sm:text-3xl">
                  {t.onboarding.title}
                </h1>
                <p className="mt-1 text-sm text-muted-foreground">
                  {t.onboarding.subtitle}
                </p>
              </div>
              <Button variant="ghost" asChild>
                <Link href="/stock">
                  {t.onboarding.openStock}
                  <ArrowRight size={15} />
                </Link>
              </Button>
            </header>

            {!canWrite && (
              <p className="rounded-lg bg-brand-soft px-3 py-2.5 text-xs text-brand">
                {t.onboarding.noPermission}
              </p>
            )}

            {error && <FormError>{error}</FormError>}

            {result && <Result result={result} />}

            <section className="flex flex-col rounded-xl border border-border bg-card">
              <header className="border-b border-border px-4 py-3">
                <h2 className="text-base font-bold tracking-tight">
                  {t.onboarding.templates.title}
                </h2>
                <p className="mt-1 text-sm text-muted-foreground">
                  {t.onboarding.templates.subtitle}
                </p>
              </header>

              <div className="grid grid-cols-[repeat(auto-fit,minmax(200px,1fr))] gap-3 p-4">
                {templates === null &&
                  Array.from({ length: 4 }).map((_, index) => (
                    <div
                      key={index}
                      className="flex flex-col gap-2 rounded-xl border border-border bg-surface-alt p-3.5"
                    >
                      <Skeleton className="h-3 w-24" />
                      <Skeleton className="h-2.5 w-full" />
                      <Skeleton className="h-2.5 w-2/3" />
                      <Skeleton className="mt-auto h-9 w-full rounded-lg" />
                    </div>
                  ))}

                {(templates ?? []).map((template) => {
                  const copy = templateCopy(t, template.id);

                  return (
                    <article
                      className="flex flex-col gap-1 rounded-xl border border-border bg-surface-alt p-3.5"
                      key={template.id}
                    >
                      <strong className="text-sm font-bold">{copy.name}</strong>
                      <p className="min-h-8 text-[11px] leading-[1.45] text-muted-foreground">
                        {copy.description}
                      </p>
                      <span className="mb-2 text-[10.5px] text-ink-3">
                        {fill(t.onboarding.templates.items, {
                          count: template.item_count,
                        })}
                      </span>
                      <Button
                        variant="secondary"
                        size="sm"
                        className="mt-auto h-9"
                        type="button"
                        disabled={!canWrite || busy !== null}
                        onClick={() => void applyTemplate(template.id)}
                      >
                        {busy === template.id
                          ? t.onboarding.templates.applying
                          : t.onboarding.templates.apply}
                      </Button>
                    </article>
                  );
                })}
              </div>
            </section>

            <section className="flex flex-col rounded-xl border border-border bg-card">
              <header className="border-b border-border px-4 py-3">
                <h2 className="text-base font-bold tracking-tight">
                  {t.onboarding.import.title}
                </h2>
                <p className="mt-1 text-sm text-muted-foreground">
                  {t.onboarding.import.subtitle}
                </p>
              </header>

              <div className="flex flex-col gap-2 p-4">
                <code className="rounded-lg border border-border bg-surface-alt px-2.5 py-2 font-mono text-[11px] text-muted-foreground">
                  {t.onboarding.import.columns}
                </code>
                <p className="text-[10.5px] text-ink-3">
                  {fill(t.onboarding.import.accepted, { max: MAX_ROWS })}
                </p>

                <div className="mt-1 flex flex-wrap gap-2">
                  <input
                    ref={input}
                    className="hidden"
                    type="file"
                    accept=".xlsx,.xls,.ods,.csv"
                    onChange={(event) => {
                      const file = event.target.files?.[0];
                      if (file) void upload(file);
                    }}
                  />
                  <Button
                    className="h-10"
                    type="button"
                    disabled={!canWrite || busy !== null}
                    onClick={() => input.current?.click()}
                  >
                    <Upload size={15} />
                    {busy === "upload"
                      ? t.onboarding.import.uploading
                      : t.onboarding.import.pick}
                  </Button>
                  <Button
                    variant="ghost"
                    className="h-10"
                    type="button"
                    onClick={downloadSample}
                  >
                    <Download size={15} />
                    {t.onboarding.import.sample}
                  </Button>
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
    <div
      className="flex flex-col gap-2 rounded-xl bg-brand-soft px-3.5 py-3"
      role="status"
    >
      <div className="flex flex-wrap items-center gap-2.5 text-xs text-brand">
        <CheckCircle2 size={16} />
        <strong className="text-sm font-bold">
          {fill(t.onboarding.result.created, { count: result.created })}
        </strong>
        {result.skipped > 0 && (
          <span className="text-muted-foreground">
            {fill(t.onboarding.result.skipped, { count: result.skipped })}
          </span>
        )}
        {result.errors.length > 0 && (
          <span className="text-muted-foreground">
            {fill(t.onboarding.result.errors, { count: result.errors.length })}
          </span>
        )}
      </div>

      {result.errors.length > 0 && (
        <ul className="flex flex-col gap-1 text-[10.5px] text-destructive">
          {result.errors.map((error) => (
            <li key={error.row}>
              <span className="mr-1.5 font-semibold">
                {fill(t.onboarding.import.rowError, { row: error.row })}
              </span>
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
