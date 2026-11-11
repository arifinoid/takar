-- Client-generated reference for stock-in.
--
-- The Tauri shell records a receipt locally while offline and replays it after
-- reconnecting. The reference makes that replay idempotent: the same entry can
-- be sent any number of times and creates exactly one batch. Online writes and
-- rows that predate this column leave it null.

alter table material_batches
    add column client_ref uuid;

-- Nulls stay distinct, so any number of online rows can omit the reference.
create unique index material_batches_client_ref
    on material_batches (tenant_id, client_ref);
