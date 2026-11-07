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
    <main className="page">
      <header className="header">
        <div>
          <p className="eyebrow">{t.home.eyebrow}</p>
          <h1>{t.common.appName}</h1>
          <p className="lede">{t.home.lede}</p>
        </div>
        <div className="header__aside">
          <HealthBadge />
          <LocaleSwitch />
        </div>
      </header>

      <section className="grid">
        {cards.map((card) => (
          <article className="card" key={card.title}>
            <h2>{card.title}</h2>
            <p>{card.detail}</p>
          </article>
        ))}
      </section>

      <footer className="footer">
        <span>{t.home.footer.phase}</span>
        <span>{t.home.footer.api}</span>
      </footer>
    </main>
  );
}
