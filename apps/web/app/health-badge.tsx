"use client";

import type { HealthResponse } from "@takar/api-client";
import { useEffect, useState } from "react";

import { api } from "@/lib/api";
import { fill, useI18n } from "@/lib/i18n";
import { cn } from "@/lib/utils";

type HealthState =
  | { status: "loading" }
  | { status: "online"; health: HealthResponse }
  | { status: "offline" };

const PILL =
  "rounded-full border bg-surface px-3 py-1.5 text-[13px] font-semibold whitespace-nowrap";

export function HealthBadge() {
  const { t } = useI18n();
  const [state, setState] = useState<HealthState>({ status: "loading" });

  useEffect(() => {
    const controller = new AbortController();

    api
      .health(controller.signal)
      .then((health) => setState({ status: "online", health }))
      .catch((error: unknown) => {
        if (error instanceof DOMException && error.name === "AbortError") {
          return;
        }
        setState({ status: "offline" });
      });

    return () => controller.abort();
  }, []);

  if (state.status === "loading") {
    return (
      <span className={cn(PILL, "border-border text-muted-foreground")}>
        {t.health.checking}
      </span>
    );
  }

  if (state.status === "offline") {
    return (
      <span className={cn(PILL, "border-[#f3c3bd] text-destructive")}>
        {t.health.offline}
      </span>
    );
  }

  return (
    <span className={cn(PILL, "border-[#b7e0d8] text-brand")}>
      {fill(t.health.online, { version: state.health.version })}
    </span>
  );
}
