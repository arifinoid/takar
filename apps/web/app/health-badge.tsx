"use client";

import { ApiClient, type HealthResponse } from "@takar/api-client";
import { useEffect, useState } from "react";

const api = new ApiClient({
  baseUrl: process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:3001",
});

type HealthState =
  | { status: "loading" }
  | { status: "online"; health: HealthResponse }
  | { status: "offline" };

export function HealthBadge() {
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
    return <span className="badge badge--idle">Checking API…</span>;
  }

  if (state.status === "offline") {
    return <span className="badge badge--offline">API offline</span>;
  }

  return <span className="badge badge--online">API {state.health.version}</span>;
}
