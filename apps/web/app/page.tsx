"use client";

import { useI18n } from "@/lib/i18n";

import { HealthBadge } from "./health-badge";
import { LocaleSwitch } from "./locale-switch";

export default function HomePage() {
  const { t } = useI18n();

  const cards = [
    t.home.cards.web,
    t.home.cards.shell,
    t.home.cards.api,
    t.home.cards.core,
  ];

  return (
    <main className="mx-auto max-w-[960px] px-[18px] pt-8 pb-10 sm:px-6 sm:pt-14">
      <header className="flex flex-col items-start gap-6 border-b border-border pb-7 sm:flex-row sm:items-start sm:justify-between">
        <div>
          <p className="mt-0 mb-2 text-[12px] font-semibold tracking-[0.12em] text-muted-foreground uppercase">
            {t.home.eyebrow}
          </p>
          <h1 className="m-0 text-[34px] leading-[1.05] tracking-[-0.03em] sm:text-[44px]">
            {t.common.appName}
          </h1>
          <p className="mt-3 mb-0 max-w-[46ch] text-[16px] leading-[1.55] text-muted-foreground">
            {t.home.lede}
          </p>
        </div>
        <div className="flex flex-col items-start gap-2.5 sm:items-end">
          <HealthBadge />
          <LocaleSwitch />
        </div>
      </header>

      <section className="mt-8 grid grid-cols-[repeat(auto-fit,minmax(220px,1fr))] gap-4">
        {cards.map((card) => (
          <article
            className="rounded-xl border border-border bg-surface p-5"
            key={card.title}
          >
            <h2 className="mt-0 mb-2 text-[16px] font-bold">{card.title}</h2>
            <p className="m-0 text-[14px] leading-[1.55] text-muted-foreground">
              {card.detail}
            </p>
          </article>
        ))}
      </section>

      <footer className="mt-10 flex flex-col justify-between gap-4 border-t border-border pt-4 text-[12px] text-muted-foreground sm:flex-row">
        <span>{t.home.footer.phase}</span>
        <span>{t.home.footer.api}</span>
      </footer>
    </main>
  );
}
