-- Accounts, memberships, and invitations.
--
-- Tenant isolation is enforced here with RLS. Role-based authorization is
-- enforced in the API (see docs/ROADMAP.md, "Authorization model"), not in
-- policies: RLS only answers which tenant owns a row.

create function app_tenant_id() returns uuid
language sql
stable
as $$
    select nullif(current_setting('app.tenant_id', true), '')::uuid
$$;

create function app_user_id() returns uuid
language sql
stable
as $$
    select nullif(current_setting('app.user_id', true), '')::uuid
$$;

create table tenants (
    id uuid primary key default gen_random_uuid(),
    name text not null check (length(btrim(name)) between 1 and 120),
    plan text not null default 'trial' check (plan in ('trial', 'starter', 'pro', 'business')),
    trial_ends_at timestamptz,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table users (
    id uuid primary key default gen_random_uuid(),
    email text,
    phone text,
    name text not null check (length(btrim(name)) between 1 and 120),
    email_verified_at timestamptz,
    phone_verified_at timestamptz,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    constraint users_contact_required check (email is not null or phone is not null)
);

create unique index users_email_unique on users (lower(email)) where email is not null;
create unique index users_phone_unique on users (phone) where phone is not null;

create table memberships (
    tenant_id uuid not null references tenants (id) on delete cascade,
    user_id uuid not null references users (id) on delete cascade,
    role text not null check (role in ('owner', 'admin', 'production', 'viewer')),
    status text not null default 'active' check (status in ('active', 'suspended')),
    invited_by uuid references users (id) on delete set null,
    joined_at timestamptz not null default now(),
    primary key (tenant_id, user_id)
);

create index memberships_user_active_idx on memberships (user_id) where status = 'active';

create table invitations (
    id uuid primary key default gen_random_uuid(),
    tenant_id uuid not null references tenants (id) on delete cascade,
    email text,
    phone text,
    role text not null check (role in ('admin', 'production', 'viewer')),
    token_hash text not null unique,
    status text not null default 'pending' check (status in ('pending', 'accepted', 'revoked', 'expired')),
    invited_by uuid not null references users (id) on delete cascade,
    expires_at timestamptz not null,
    accepted_by uuid references users (id) on delete set null,
    accepted_at timestamptz,
    created_at timestamptz not null default now(),
    constraint invitations_contact_required check (email is not null or phone is not null),
    constraint invitations_expiry_after_creation check (expires_at > created_at)
);

-- One live invitation per contact per tenant.
create unique index invitations_pending_contact_unique
    on invitations (tenant_id, coalesce(lower(email), phone))
    where status = 'pending';

alter table tenants enable row level security;
alter table tenants force row level security;
create policy tenants_select on tenants
    for select using (id = app_tenant_id());
create policy tenants_insert on tenants
    for insert with check (true);
create policy tenants_update on tenants
    for update using (id = app_tenant_id()) with check (id = app_tenant_id());

alter table users enable row level security;
alter table users force row level security;
-- OTP login resolves a user by email/phone before a user or tenant is known.
-- Do that through a narrow SECURITY DEFINER lookup in P0-06; do not widen
-- this policy.
create policy users_select on users
    for select using (
        id = app_user_id()
        or exists (
            select 1
            from memberships m
            where m.user_id = users.id
              and m.tenant_id = app_tenant_id()
              and m.status = 'active'
        )
    );
create policy users_insert on users
    for insert with check (true);
create policy users_update on users
    for update using (id = app_user_id()) with check (id = app_user_id());

alter table memberships enable row level security;
alter table memberships force row level security;
create policy memberships_tenant on memberships
    for all using (tenant_id = app_tenant_id())
    with check (tenant_id = app_tenant_id());

-- A user must be able to list their own memberships before choosing a tenant.
create policy memberships_self_select on memberships
    for select using (user_id = app_user_id());

alter table invitations enable row level security;
alter table invitations force row level security;
create policy invitations_tenant on invitations
    for all using (tenant_id = app_tenant_id())
    with check (tenant_id = app_tenant_id());

-- Invitation acceptance cannot know the tenant before the token is resolved.
-- Add a narrow SECURITY DEFINER lookup by token_hash in P0-11 instead of
-- widening this policy.
