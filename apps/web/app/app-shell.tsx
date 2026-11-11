"use client";

import type { MeResponse } from "@takar/api-client";
import {
  BarChart3,
  Boxes,
  CalendarClock,
  ChefHat,
  LayoutDashboard,
  MessageCircle,
  Package,
  Settings,
  ShoppingCart,
  Truck,
  Users,
} from "lucide-react";
import Link from "next/link";
import type { ComponentType } from "react";

import { useI18n } from "@/lib/i18n";

import { LocaleSwitch } from "./locale-switch";

type Active = "overview" | "stock" | "production" | "products" | "settings";

type Item = {
  key: string;
  label: string;
  icon: ComponentType<{ size?: number; strokeWidth?: number }>;
  href?: string;
};

type Props = {
  me: MeResponse;
  active: Active;
  onSignOut: () => void;
  children: React.ReactNode;
};

function initials(name: string): string {
  return name
    .split(" ")
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0]?.toUpperCase() ?? "")
    .join("");
}

export function AppShell({ me, active, onSignOut, children }: Props) {
  const { t } = useI18n();

  const groups: { label: string; items: Item[] }[] = [
    {
      label: t.nav.operations,
      items: [
        {
          key: "overview",
          label: t.nav.overview,
          icon: LayoutDashboard,
          href: "/dashboard",
        },
        {
          key: "production",
          label: t.nav.production,
          icon: ChefHat,
          href: "/production",
        },
        {
          key: "products",
          label: t.nav.products,
          icon: Boxes,
          href: "/products",
        },
        { key: "stock", label: t.nav.stock, icon: Package, href: "/stock" },
        { key: "batches", label: t.nav.batches, icon: CalendarClock },
      ],
    },
    {
      label: t.nav.sales,
      items: [
        { key: "pos", label: t.nav.pos, icon: ShoppingCart },
        { key: "purchasing", label: t.nav.purchasing, icon: Truck },
        { key: "reports", label: t.nav.reports, icon: BarChart3 },
      ],
    },
    {
      label: t.nav.other,
      items: [
        { key: "alerts", label: t.nav.alerts, icon: MessageCircle },
        { key: "team", label: t.nav.team, icon: Users },
        {
          key: "settings",
          label: t.nav.settings,
          icon: Settings,
          href: "/settings",
        },
      ],
    },
  ];

  return (
    <div className="shell">
      <aside className="shell__sidebar">
        <div className="shell__brand">
          <span className="shell__logo">
            <ChefHat size={17} />
          </span>
          <span className="shell__brand-text">
            <strong>Takar</strong>
            <span>PPIC + POS</span>
          </span>
        </div>

        <div className="shell__tenant">
          <div>
            <strong>{me.tenant.name}</strong>
            <span>{t.dashboard.roles[me.role]}</span>
          </div>
        </div>

        <nav className="shell__nav" aria-label={t.nav.main}>
          {groups.map((group) => (
            <div className="shell__group" key={group.label}>
              <p className="shell__group-label">{group.label}</p>
              {group.items.map((item) =>
                item.href ? (
                  <Link
                    key={item.key}
                    href={item.href}
                    className={`shell__nav-item${
                      item.key === active ? " shell__nav-item--active" : ""
                    }`}
                    aria-current={item.key === active ? "page" : undefined}
                  >
                    <item.icon size={16} />
                    <span>{item.label}</span>
                  </Link>
                ) : (
                  <span
                    key={item.key}
                    className="shell__nav-item shell__nav-item--disabled"
                    title={t.nav.soon}
                    aria-disabled="true"
                  >
                    <item.icon size={16} />
                    <span>{item.label}</span>
                  </span>
                ),
              )}
            </div>
          ))}
        </nav>

        <div className="shell__footer">
          <div className="shell__user">
            <span className="shell__avatar">{initials(me.user.name)}</span>
            <div>
              <strong>{me.user.name}</strong>
              <span>{t.dashboard.roles[me.role]}</span>
            </div>
          </div>
          <div className="shell__footer-actions">
            <LocaleSwitch />
            <button className="btn btn--ghost" type="button" onClick={onSignOut}>
              {t.nav.signOut}
            </button>
          </div>
        </div>
      </aside>

      <div className="shell__main">
        <header className="shell__topbar">
          <div className="shell__topbar-tenant">
            <span>{t.dashboard.tenant}</span>
            <strong>{me.tenant.name}</strong>
          </div>
          <LocaleSwitch />
        </header>

        <div className="shell__content">{children}</div>
      </div>

      <nav className="tabbar" aria-label={t.nav.main}>
        <Link
          href="/dashboard"
          className={`tabbar__item${
            active === "overview" ? " tabbar__item--active" : ""
          }`}
        >
          <LayoutDashboard size={18} />
          <span>{t.nav.mobileHome}</span>
        </Link>
        <Link
          href="/stock"
          className={`tabbar__item${
            active === "stock" ? " tabbar__item--active" : ""
          }`}
        >
          <Package size={18} />
          <span>{t.nav.mobileStock}</span>
        </Link>
        <Link
          href="/production"
          className={`tabbar__item${
            active === "production" ? " tabbar__item--active" : ""
          }`}
        >
          <ChefHat size={18} />
          <span>{t.nav.mobileProduction}</span>
        </Link>
        <span className="tabbar__item tabbar__item--disabled" aria-disabled="true">
          <MessageCircle size={18} />
          <span>{t.nav.mobileAlerts}</span>
        </span>
      </nav>
    </div>
  );
}
