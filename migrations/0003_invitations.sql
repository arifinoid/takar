-- Invitation lookups that run before a tenant is known.

create function auth_find_invitation(token_hash text)
returns table (
    id uuid,
    tenant_id uuid,
    email text,
    phone text,
    role text,
    status text,
    expires_at timestamptz,
    invited_by uuid
)
language sql
stable
security definer
set search_path = public, pg_temp
as $$
    select i.id, i.tenant_id, i.email, i.phone, i.role, i.status, i.expires_at, i.invited_by
    from invitations i
    where i.token_hash = $1
$$;

create function auth_has_pending_invitation(contact text)
returns boolean
language sql
stable
security definer
set search_path = public, pg_temp
as $$
    select exists (
        select 1
        from invitations i
        where i.status = 'pending'
          and i.expires_at > now()
          and (lower(i.email) = lower(contact) or i.phone = contact)
    )
$$;

revoke all on function auth_find_invitation(text) from public;
revoke all on function auth_has_pending_invitation(text) from public;

-- Deployment must grant execute to the application role, because the API
-- connects as takar_app and both calls happen before a tenant is known:
--   grant execute on function auth_find_invitation(text) to takar_app;
--   grant execute on function auth_has_pending_invitation(text) to takar_app;
--
-- Invitations are created and revoked under normal RLS: both require an
-- active membership, so app.tenant_id is already set.
