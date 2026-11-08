import type { Locale } from "./i18n";

/** Tags used for number and date formatting, not for API calls. */
export function localeTag(locale: Locale): string {
  return locale === "id" ? "id-ID" : "en-US";
}

/** API decimals are strings; formatting is the only place they become numbers. */
export function formatQty(value: string, locale: Locale): string {
  return new Intl.NumberFormat(localeTag(locale), {
    maximumFractionDigits: 3,
  }).format(Number(value));
}

export function formatMoney(value: number, locale: Locale): string {
  return new Intl.NumberFormat(localeTag(locale), {
    style: "currency",
    currency: "IDR",
    maximumFractionDigits: 0,
  }).format(value);
}

/** Parses a date-only string as a local date, so the day never shifts. */
export function parseDate(value: string): Date {
  const [year, month, day] = value.split("-").map(Number);
  return new Date(year, month - 1, day);
}

export function formatDate(value: string, locale: Locale): string {
  return new Intl.DateTimeFormat(localeTag(locale), {
    day: "2-digit",
    month: "short",
  }).format(parseDate(value));
}

export function formatDateTime(value: string, locale: Locale): string {
  return new Intl.DateTimeFormat(localeTag(locale), {
    day: "2-digit",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(value));
}

export function todayIso(): string {
  const now = new Date();
  const month = String(now.getMonth() + 1).padStart(2, "0");
  const day = String(now.getDate()).padStart(2, "0");
  return `${now.getFullYear()}-${month}-${day}`;
}

export function addDaysIso(value: string, days: number): string {
  const date = parseDate(value);
  date.setDate(date.getDate() + days);
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${date.getFullYear()}-${month}-${day}`;
}

/** Whole days from today to a date-only value. Negative means overdue. */
export function daysUntil(value: string, today = todayIso()): number {
  const target = parseDate(value).getTime();
  const start = parseDate(today).getTime();
  return Math.round((target - start) / 86_400_000);
}

/**
 * Accepts "2,5" from an Indonesian keyboard and returns "2.5" for the API.
 * Returns null when the value is not a plain non-negative decimal.
 */
export function normalizeDecimal(value: string): string | null {
  const normalized = value.trim().replace(",", ".");

  if (!/^\d+(\.\d+)?$/.test(normalized)) {
    return null;
  }

  return normalized;
}
