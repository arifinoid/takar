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

Local sign-in: the API loads `.env` at startup. With `DEV_OTP_CODE` set (default `000000`), debug builds use that fixed code for any contact instead of a random one, so authenticated routes are reachable without reading the log. Release builds always randomize.

OTP delivery is picked from the environment, never from code: `EMAIL_PROVIDER=brevo` with `BREVO_API_KEY`, `EMAIL_FROM`, `EMAIL_FROM_NAME`, and `WHATSAPP_PROVIDER=fonnte` with `FONNTE_TOKEN`. With none set, debug builds write the message to the log and release builds answer `otp_delivery_failed`. Transports live in `apps/api/src/notify/` and are reused for alerts in P1-07; adding a provider is one module, not a change to the OTP path. A contact may request five codes per ten minutes.

Database:

```bash
sqlx database create
sqlx migrate run
sqlx migrate add <name>                    # forward-only, numbered
cargo sqlx prepare --workspace             # refresh .sqlx after query changes
```

Local Postgres for development and RLS tests:

```bash
docker compose up -d db
docker compose down                    # stop, keeps the volume

DATABASE_URL=postgres://takar:takar@127.0.0.1:5432/takar cargo test -p takar-db
```

Daily jobs (run from cron in deployment):

```bash
cargo run -p takar-api --bin alert_job     # fills, resolves, and sends alerts; safe to re-run
```

The job claims tenants with `skip locked`, so it needs a database role that can see every tenant — the migration role or a role with `bypassrls`. Per-tenant work still runs in a tenant-scoped transaction.

Delivery goes through the same `notify::Router` as OTP. Each tenant gets one digest per channel, not one message per alert; every attempt is recorded (`delivery_status`, `attempts`, `last_error`), a failure is retried on the next run up to three attempts, and a tenant without an owner contact is marked `skipped` instead of retrying forever. If the rule's channel has no contact but the owner has the other one, the digest uses that channel; a tenant with no owner at all is the only case that is skipped. Links in the message point at `APP_URL`.

`#[sqlx::test]` creates a throwaway database per test, so the URL must point at a server where the user can create databases. The compose user is the cluster superuser, which satisfies that requirement for development only.

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

### Recipes

Recipes are versioned, never edited in place. Creating a recipe version deactivates the previous one, and every version keeps its own `recipe_items`; production will point at the exact version it consumed so historical HPP never changes. A partial unique index enforces one active version per product, and the database rejects a recipe whose product belongs to another tenant. `products.current_recipe_id` follows the newest version and is null when the product has no recipe. Products share the `recipe_read`/`recipe_write` permissions — the RBAC matrix has no separate product permission.

### Production

A production order is the only writer of `production_use` movements. It runs in one transaction: load the product's current recipe version, compute each material requirement, allocate batches FEFO with `takar_core::allocate`, and reject the whole run with `409 insufficient_stock` plus the shortage list (D3) before writing anything. On success it decrements `material_batches.qty_remaining`, appends a negative movement and a `production_consumptions` row per batch, and adds `actual_qty` to `products.stock_qty`.

`POST /production-orders/preview` runs the same plan read-only and returns each material's requirement, the batches FEFO would take, and the HPP with margin. It returns null cost fields when any material is short, because a partial allocation would understate the HPP.

The order stores the `recipe_id` it consumed; creating a newer recipe version never rewrites it. Requirements round up to three decimals to match the batch ledger.

Cost is snapshotted in the same transaction: `material_cost` is the sum of `production_consumptions.qty × unit_cost`, and `unit_cost` is the total divided by `actual_qty` (four decimals, D7), so a later purchase at a different price never changes a past HPP.

Labor is priced from `tenants.labor_rate_per_hour` (set through `PUT /settings`): a recipe's `labor_minutes` covers one batch, so a run of `qty` units costs `rate × minutes / 60 × (qty ÷ yield_qty)`. The rate is read once per plan and stored in the order, so changing it never rewrites history. A zero rate means labor is left out of the HPP. `overhead_cost` stays zero until a tenant overhead input exists; `takar_core::production_cost` already accepts the component.

`/settings` is readable by any member because it explains the HPP and the shopping list they see; `PUT /settings` requires `tenant_manage` (owner, admin). Fields are optional: an omitted field keeps its stored value.

### Sales

A sale is the only writer that removes finished goods. `POST /sales` runs in one transaction: it locks the products it sells (`select … for update`), rejects the whole document with `409 insufficient_stock` plus every shortage when any line is short, then inserts `sales` and `sale_items` and subtracts `products.stock_qty`. Nothing is written before the check passes, so stock never drifts from the recorded sales.

`sale_items.price` is snapshotted at sale time. Changing `products.sell_price` later never rewrites past revenue, the same way a new purchase never rewrites a past HPP.

`client_ref` (uuid) is the idempotency key for imports and retries: replaying the same document returns the first sale with `200` and moves stock once. `sale_items` carries `tenant_id` so the composite foreign key proves the product belongs to the same tenant; RLS isolates it directly rather than through an `EXISTS` policy.

Channels are validated in the API (`toko`, `reseller`, `online`, `lainnya`) and stored as text, so adding one is a code change rather than a migration. `GET /sales?limit=` returns newest first with their lines. Reads need `sales_read` (every role); writes need `sales_write` (owner, admin).

### Purchasing

`GET /purchasing/recommendations` turns the movement ledger into a shopping list. Usage is the sum of `production_use`, `waste`, and `expired` movements over the tenant's usage window (`tenants.usage_window_days`, default 30, allowed 1–365), averaged per day; purchases and adjustments are excluded because neither says anything about consumption.

`takar_core::reorder_plan` targets `average_daily_usage × lead_time_days + min_stock` and orders `max(0, target − on_hand)`, rounded up to three decimals. When the material has a shelf life, the order is capped at `average_daily_usage × shelf_life_days − on_hand` so a long lead time cannot push a tenant into buying stock that expires before it is used; the response flags that case. Lead time comes from the material's default supplier and is zero when none is set.

A material with no outbound movement in the window comes back with `insufficient_history: true` and only closes the gap to `min_stock`; the shelf-life cap is skipped because a zero usage would cap every order to zero. Materials whose recommendation is zero are omitted.

### Offline

The Tauri shell records stock-in while offline; the browser build stays online-only (D2). A receipt is queued in SQLite (`stock_outbox`, created by the shell's plugin migration) with a client-generated `client_ref` UUID, and `POST /sync/stock-in` replays the queue when the device reconnects.

`client_ref` is the idempotency key: `material_batches` has a unique `(tenant_id, client_ref)` index, and replaying a receipt returns the batch the first attempt created with `200` instead of `201`. The sync endpoint runs each entry in its own transaction, so one rejected receipt never rolls back the others; the response reports `created`, `duplicate`, or `rejected` with the API error code per entry. Rejected entries stay in the queue marked failed and appear on the stock page to retry or dismiss — they are never dropped silently and never merged last-write-wins. The online route accepts the same `client_ref`, so a retry after a timeout is safe outside the queue too.

### Tenancy

Every business row carries `tenant_id`. Postgres RLS is the enforcement layer; app-level filters are defense in depth. The JWT carries `user_id` plus the active `tenant_id`, and each request transaction sets `app.tenant_id` and `app.user_id` via `SET LOCAL`. RLS is forced on every account table, and tests must `SET LOCAL ROLE` to a non-owner role because the table owner bypasses policies. Child tables without `tenant_id` (`recipe_items`, `production_consumptions`, `unit_conversions`) need either their own column or an `EXISTS` policy against the parent — decide before writing policies. `sale_items` took the column because its product reference also has to be tenant-checked.

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
- Offline sync covers stock-in only (D2). Production and corrections need a live connection because they depend on current stock; do not widen the outbox without a new decision.
- The draft schema has no `warehouse_id` although Pro promises two warehouses. Do not add ad-hoc warehouse columns in phase 1 — follow the roadmap decision.
- Dates: `expires_at` is a `date`, not a timestamp; avoid timezone drift when comparing.

## Conventions

- English everywhere in code and docs: identifiers, comments, test names, API/CLI messages, and documentation. UI copy comes from i18n resources with `id` and `en` locales; never hardcode strings in components.
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
