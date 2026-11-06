# CODEBUDDY.md

This file provides guidance to CodeBuddy Code when working with code in this repository.

## Repository state

Greenfield. `main` has no commits yet; the only artifacts are `flake.nix` and the product plan. Do not assume code exists — verify before referencing modules.

Two documents are the source of truth:

- `Rencana Produk PPIC + POS Ringan untuk UMKM F&B.md` — product spec, schema draft, pricing, stack decisions.
- `docs/ROADMAP.md` — execution order, task IDs, gates, open decisions.

Read both before starting work. Implement only inside the current roadmap phase; do not pull deferred scope (full POS, CRM, multi-outlet) forward.

## Commands

Everything runs inside the Nix dev shell. The flake currently provisions only the Rust toolchain; Bun, Postgres, and sqlx-cli are added by roadmap task P0-02. Use Bun for everything JavaScript/TypeScript — no npm, pnpm, or yarn.

```bash
nix develop          # enter dev shell
direnv allow         # optional, .envrc already uses flake
```

Rust:

```bash
cargo build --workspace
cargo test --workspace
cargo test -p takar-core fefo              # single module
cargo test -p takar-core -- --nocapture
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

Database:

```bash
sqlx database create
sqlx migrate run
sqlx migrate add <name>                    # forward-only, numbered
cargo sqlx prepare --workspace             # refresh .sqlx after query changes
```

Web and shell (after `apps/web` and `apps/shell` exist):

```bash
bun install
bun run --filter web dev
bun run --filter web build                 # must stay a static export
bun run --filter web lint
bun run --filter web typecheck
bun run --filter shell tauri android dev
```

## Architecture

Three runtimes, one calculation core:

- `apps/web` — Next.js static export; the UI for both browser and Tauri WebView.
- `apps/shell` — Tauri 2 wrapper. Android first, iOS after Android is stable.
- `apps/api` — Axum. The only place with server logic (auth, jobs, HTTP).
- `crates/core` — pure functions: FEFO allocation, HPP, unit conversion, reorder planning. No IO, no sqlx, no async. Used by `api` and `shell` so client and server produce identical numbers.
- `crates/domain` — domain types and the API contract (utoipa/specta). Generates `packages/api-client`.
- `crates/db` — sqlx queries and data access.
- `migrations/` — Postgres DDL, including RLS policies.

Data flow: UI → generated TypeScript client → Axum → `takar-db`/`takar-core` → Postgres. Core may also run inside Tauri for local previews, but the server stays the source of truth.

### Stock model

Stock is a batch ledger, not a scalar. `material_batches.qty_remaining` is on-hand per batch; `stock_movements` is the audit trail. Never add a single `stock` column to `materials`. Finished goods currently rely on `products.stock_qty` — see the open decision about batch-tracking finished goods before extending it.

### Tenancy

Every business row carries `tenant_id`. Postgres RLS is the enforcement layer; app-level filters are defense in depth. The JWT carries `user_id` plus the active `tenant_id`, and each request transaction sets `app.tenant_id` via `SET LOCAL`. Child tables without `tenant_id` (`recipe_items`, `sale_items`, `production_consumptions`, `unit_conversions`) need either their own column or an `EXISTS` policy against the parent — decide before writing policies.

### Business invariants

1. On-hand = `sum(qty_remaining)` over active batches.
2. Production consumes FEFO: earliest `expires_at` first, nulls last, then `received_at`.
3. HPP per unit = (material cost from `production_consumptions` + labor + allocated overhead) ÷ `actual_qty`.
4. Material requirement = `qty × (1 + waste_pct) × (production qty ÷ yield_qty)`.
5. The daily job fills `alerts` for expiring batches and below-min-stock materials, and must be idempotent.
6. Costs are snapshotted (`production_consumptions.unit_cost`, `production_orders.unit_cost`). Never recompute history from current prices.

Use `rust_decimal` and Postgres `NUMERIC` end to end. Never `f64` for money or quantities.

## Gotchas

- Next static export: no SSR, route handlers, server actions, middleware, or ISR. Set `images.unoptimized = true`. Any server behavior belongs in Axum.
- Tauri mobile runs an old WebView on cheap Android phones. Test on low-end devices from the first build, not at the end.
- Server push (FCM/APNs) is out of MVP scope; alerts go through WhatsApp.
- sqlx compile-time checked queries need `DATABASE_URL` at build time or a committed `.sqlx` cache. CI must use one of them.
- Offline sync is not designed yet. Do not invent an outbox or conflict policy ad hoc; it is an explicit roadmap decision.
- The draft schema has no `warehouse_id` although Pro promises two warehouses. Do not add ad-hoc warehouse columns in phase 1 — follow the roadmap decision.
- Dates: `expires_at` is a `date`, not a timestamp; avoid timezone drift when comparing.

## Conventions

- English everywhere: identifiers, comments, test names, API/CLI messages, UI copy, and documentation. Indonesian is reserved for conversation.
- Migrations are forward-only and numbered; never edit an applied migration.
- Core functions are pure and covered by golden tests (rounding, null expiry, shortfall, conversion chains).
- Integration tests hit a real Postgres with RLS enabled. Mocked DB tests are not acceptable for tenancy or stock math.
- Workflow: create a branch, commit, push, open a PR. Do not merge; the user merges.

## Where to look first

| Question | File |
| --- | --- |
| What to build next, acceptance criteria, open decisions | `docs/ROADMAP.md` |
| Why the product exists, schema draft, pricing, stack rationale | `Rencana Produk PPIC + POS Ringan untuk UMKM F&B.md` |
| Dev environment | `flake.nix`, `.envrc` |
