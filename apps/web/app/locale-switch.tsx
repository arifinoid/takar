"use client";

import { useI18n, type Locale } from "@/lib/i18n";

const LABELS: Record<Locale, string> = { id: "ID", en: "EN" };

export function LocaleSwitch() {
  const { locale, setLocale, t } = useI18n();

  return (
    <div className="locale-switch" role="group" aria-label={t.common.language}>
      {(Object.keys(LABELS) as Locale[]).map((value) => (
        <button
          key={value}
          type="button"
          className={
            value === locale
              ? "locale-switch__option locale-switch__option--active"
              : "locale-switch__option"
          }
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
