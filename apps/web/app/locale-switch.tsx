"use client";

import { useI18n, type Locale } from "@/lib/i18n";
import { cn } from "@/lib/utils";

const LABELS: Record<Locale, string> = { id: "ID", en: "EN" };

export function LocaleSwitch() {
  const { locale, setLocale, t } = useI18n();

  return (
    <div
      className="inline-flex rounded-full border border-border bg-surface p-0.5"
      role="group"
      aria-label={t.common.language}
    >
      {(Object.keys(LABELS) as Locale[]).map((value) => (
        <button
          key={value}
          type="button"
          className={cn(
            "rounded-full px-2.5 py-1 text-[12px] font-bold tracking-[0.06em] transition-colors outline-none focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand",
            value === locale
              ? "bg-brand text-white"
              : "text-muted-foreground hover:text-foreground",
          )}
          aria-pressed={value === locale}
          title={value === "id" ? t.common.localeId : t.common.localeEn}
          onClick={() => setLocale(value)}
        >
          {LABELS[value]}
        </button>
      ))}
    </div>
  );
}
