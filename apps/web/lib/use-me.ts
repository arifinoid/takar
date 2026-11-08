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
 * Loads the signed-in user and redirects to the login screen when the session
 * is missing or rejected. Shared by every authenticated page.
 */
export function useMe() {
  const router = useRouter();
  const [state, setState] = useState<MeState>({ status: "loading" });

  const load = useCallback(async () => {
    const token = readToken();

    if (!token) {
      router.replace("/login");
      return;
    }

    setState({ status: "loading" });

    try {
      const me = await api.me(token);
      setState({ status: "ready", me });
    } catch (cause) {
      if (
        cause instanceof ApiError &&
        (cause.status === 401 || cause.status === 403)
      ) {
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
    clearToken();
    router.replace("/login");
  }, [router]);

  return { state, reload: load, signOut };
}
