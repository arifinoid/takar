"use client";

import { ApiError, type MeResponse } from "@takar/api-client";
import { useRouter } from "next/navigation";
import { useCallback, useEffect, useState } from "react";

import { api } from "./api";
import { clearToken, readToken } from "./session";

export type MeState =
  | { status: "loading" }
  | { status: "ready"; me: MeResponse }
  | { status: "error" };

/**
 * Last answer from /me, kept for the life of the tab. Every page mounts its
 * own `useMe`, so without this the sidebar and the header would blank out on
 * every navigation while the same request runs again.
 */
let cached: MeResponse | null = null;

/**
 * Loads the signed-in user and redirects to the login screen when the session
 * is missing or rejected. Shared by every authenticated page.
 */
export function useMe() {
  const router = useRouter();
  const [state, setState] = useState<MeState>(() =>
    cached ? { status: "ready", me: cached } : { status: "loading" },
  );

  const load = useCallback(async () => {
    const token = readToken();

    if (!token) {
      cached = null;
      router.replace("/login");
      return;
    }

    // A cached answer renders immediately; the request below only refreshes it.
    if (!cached) setState({ status: "loading" });

    try {
      const me = await api.me(token);
      cached = me;
      setState({ status: "ready", me });
    } catch (cause) {
      if (
        cause instanceof ApiError &&
        (cause.status === 401 || cause.status === 403)
      ) {
        cached = null;
        clearToken();
        router.replace("/login");
        return;
      }

      setState({ status: "error" });
    }
  }, [router]);

  useEffect(() => {
    void load();
  }, [load]);

  const signOut = useCallback(() => {
    cached = null;
    clearToken();
    router.replace("/login");
  }, [router]);

  return { state, reload: load, signOut };
}
