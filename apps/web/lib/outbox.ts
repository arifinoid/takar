import type { StockInRequest } from "@takar/api-client";

import { api } from "./api";

// Same file name as the migration in `apps/shell/src-tauri/src/lib.rs`.
const DATABASE = "sqlite:takar.db";

export type QueuedStockIn = {
  client_ref: string;
  payload: StockInRequest;
  status: "pending" | "failed";
  code: string | null;
  attempts: number;
  created_at: string;
};

type Row = {
  client_ref: string;
  payload: string;
  status: "pending" | "failed";
  code: string | null;
  attempts: number;
  created_at: string;
};

// The WebView may expose only part of the Web Crypto API.
type CryptoLike = {
  randomUUID?: () => string;
  getRandomValues?: (bytes: Uint8Array) => void;
};

// The slice of the Tauri SQL plugin this module uses.
type SqlDatabase = {
  execute(sql: string, params?: unknown[]): Promise<unknown>;
  select<T>(sql: string, params?: unknown[]): Promise<T>;
};

let connection: Promise<SqlDatabase | null> | null = null;

// True only inside the Tauri shell; the browser build has no local database.
export function offlineQueueAvailable(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

async function database(): Promise<SqlDatabase | null> {
  if (!offlineQueueAvailable()) return null;

  if (!connection) {
    connection = import("@tauri-apps/plugin-sql")
      .then(({ default: Database }) => Database.load(DATABASE) as Promise<SqlDatabase>)
      .catch(() => {
        // A failed load must not poison later attempts.
        connection = null;
        return null;
      });
  }

  return connection;
}

// UUID v4 for the idempotency key, with a fallback for WebViews that do not
// expose `crypto.randomUUID`.
export function createClientRef(): string {
  const source = (globalThis as { crypto?: CryptoLike }).crypto;

  if (source?.randomUUID) {
    return source.randomUUID();
  }

  const bytes = new Uint8Array(16);

  if (source?.getRandomValues) {
    source.getRandomValues(bytes);
  } else {
    for (let index = 0; index < bytes.length; index += 1) {
      bytes[index] = Math.floor(Math.random() * 256);
    }
  }

  bytes[6] = (bytes[6] & 0x0f) | 0x40;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;

  const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0"));

  return [
    hex.slice(0, 4).join(""),
    hex.slice(4, 6).join(""),
    hex.slice(6, 8).join(""),
    hex.slice(8, 10).join(""),
    hex.slice(10, 16).join(""),
  ].join("-");
}

// Stores a receipt locally. Returns false when there is no local database,
// so the caller can report that the entry was not saved anywhere.
export async function enqueueStockIn(payload: StockInRequest): Promise<boolean> {
  const db = await database();

  if (!db) return false;

  const clientRef = payload.client_ref ?? createClientRef();

  await db.execute(
    "insert or replace into stock_outbox (client_ref, payload, status) values ($1, $2, 'pending')",
    [clientRef, JSON.stringify({ ...payload, client_ref: clientRef })],
  );

  return true;
}

export async function listQueue(): Promise<QueuedStockIn[]> {
  const db = await database();

  if (!db) return [];

  const rows = await db.select<Row[]>(
    `select client_ref, payload, status, code, attempts, created_at
     from stock_outbox
     order by created_at, client_ref`,
  );

  return rows.map((row) => ({
    client_ref: row.client_ref,
    payload: JSON.parse(row.payload) as StockInRequest,
    status: row.status,
    code: row.code,
    attempts: row.attempts,
    created_at: row.created_at,
  }));
}

export type FlushSummary = { synced: number; failed: number };

// Sends every pending entry and keeps the rejected ones visible. Entries the
// server accepted leave the queue; rejected entries stay marked
// failed with the API code so the user can fix or dismiss them.
export async function flushQueue(token: string): Promise<FlushSummary> {
  const db = await database();

  if (!db) return { synced: 0, failed: 0 };

  const rows = await db.select<Row[]>(
    `select client_ref, payload, status, code, attempts, created_at
     from stock_outbox where status = 'pending' order by created_at, client_ref`,
  );

  if (rows.length === 0) return { synced: 0, failed: 0 };

  const response = await api.syncStockIn(
    token,
    rows.map((row) => JSON.parse(row.payload) as StockInRequest),
  );

  let synced = 0;
  let failed = 0;

  for (const result of response.results) {
    if (!result.client_ref) continue;

    if (result.status === "created" || result.status === "duplicate") {
      await db.execute("delete from stock_outbox where client_ref = $1", [
        result.client_ref,
      ]);
      synced += 1;
    } else {
      await db.execute(
        `update stock_outbox
         set status = 'failed', code = $2, attempts = attempts + 1
         where client_ref = $1`,
        [result.client_ref, result.code],
      );
      failed += 1;
    }
  }

  return { synced, failed };
}

// Puts a failed entry back in the pending queue for another attempt.
export async function retryEntry(clientRef: string): Promise<void> {
  const db = await database();

  if (!db) return;

  await db.execute(
    "update stock_outbox set status = 'pending', code = null where client_ref = $1",
    [clientRef],
  );
}

export async function dismissEntry(clientRef: string): Promise<void> {
  const db = await database();

  if (!db) return;

  await db.execute("delete from stock_outbox where client_ref = $1", [clientRef]);
}
