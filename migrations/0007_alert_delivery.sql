-- Delivery tracking for alerts.
--
-- An alert is structured data; the message text is composed when it is sent.
-- Delivery state lives here so a failed send stays visible and is retried on
-- the next run instead of vanishing into a log line.

alter table alerts
    add column delivery_status text not null default 'pending'
        check (delivery_status in ('pending', 'sent', 'failed', 'skipped')),
    -- Snapshot of the rule channel at send time: a rule can change later.
    add column delivery_channel text check (delivery_channel in ('whatsapp', 'email')),
    add column delivery_recipient text,
    add column attempts int not null default 0 check (attempts >= 0),
    add column last_error text;

-- The sender scans for work by tenant; resolved alerts are never sent.
create index alerts_pending_delivery_idx on alerts (tenant_id, created_at)
    where resolved_at is null and delivery_status in ('pending', 'failed');
