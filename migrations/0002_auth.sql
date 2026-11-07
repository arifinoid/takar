-- OTP challenges and the lookups needed before a tenant is known.
-- otp_codes is global (not tenant-scoped) and stores only hashed codes.

create table otp_codes (
    id uuid primary key default gen_random_uuid(),
    contact text not null,
    code_hash text not null,
    attempts int not null default 0 check (attempts >= 0),
    expires_at timestamptz not null,
    consumed_at timestamptz,
    created_at timestamptz not null default now()
);

create index otp_codes_contact_idx on otp_codes (contact, created_at desc);
create index otp_codes_active_idx on otp_codes (expires_at) where consumed_at is null;

-- Resolve a user by exact contact before `app.user_id` is known.
-- Deployment must grant execute to the application role:
--   grant execute on function auth_find_user_id(text) to takar_app;
create function auth_find_user_id(contact text) returns uuid
language sql
stable
security definer
set search_path = public, pg_temp
as $$
    select id
    from users
    where (email is not null and lower(email) = lower(contact))
       or (phone is not null and phone = contact)
    limit 1
$$;

revoke all on function auth_find_user_id(text) from public;

-- Tenant switcher: a user must see the names of tenants they belong to.
create policy tenants_member_select on tenants
    for select using (
        exists (
            select 1
            from memberships m
            where m.tenant_id = tenants.id
              and m.user_id = app_user_id()
              and m.status = 'active'
        )
    );
