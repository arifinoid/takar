//! Daily alert job.
//!
//! Rules are per tenant and the job is idempotent: every insert is guarded by
//! an open-alert unique index, and each run also resolves alerts whose
//! condition no longer holds.
//!
//! Tenants are claimed with `for update skip locked`, so two instances can run
//! side by side and split the work instead of duplicating alerts.

use chrono::{NaiveDate, Utc};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct AlertRun {
    pub tenants: u64,
    pub created: u64,
    pub resolved: u64,
}

/// Inserts the rules every tenant starts with: expiry warnings at H-7 and
/// H-3, plus low stock. New tenants get these when they are created.
pub async fn seed_default_rules(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "insert into alert_rules (tenant_id, type, days_before)
         values ($1, 'expiry', 7), ($1, 'expiry', 3), ($1, 'low_stock', null)",
    )
    .bind(tenant_id)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

/// Runs the job for the current UTC date.
pub async fn run(pool: &PgPool) -> Result<AlertRun, sqlx::Error> {
    run_on(pool, Utc::now().date_naive()).await
}

/// Runs the job as if today were `today`. Tests use this to pin the H-7/H-3
/// boundaries.
pub async fn run_on(pool: &PgPool, today: NaiveDate) -> Result<AlertRun, sqlx::Error> {
    let mut report = AlertRun::default();
    let mut claim = pool.begin().await?;
    let mut after = Uuid::nil();

    loop {
        // The cursor matters: a transaction does not conflict with its own
        // locks, so without `id > after` SKIP LOCKED would return the same
        // tenant forever. NO KEY UPDATE, not UPDATE: inserting an alert takes a
        // key-share lock on its tenant, and FOR UPDATE would block that.
        let tenant: Option<(Uuid,)> = sqlx::query_as(
            "select id from tenants
             where id > $1
             order by id
             for no key update skip locked
             limit 1",
        )
        .bind(after)
        .fetch_optional(&mut *claim)
        .await?;

        let Some((tenant_id,)) = tenant else {
            break;
        };

        after = tenant_id;

        // Each tenant runs in its own transaction so RLS is scoped to it and a
        // failure cannot leave half a tenant's alerts behind.
        let mut tx = crate::begin_scoped(pool, Some(tenant_id), None).await?;
        let (created, resolved) = refresh_tenant(&mut tx, tenant_id, today).await?;
        tx.commit().await?;

        report.tenants += 1;
        report.created += created;
        report.resolved += resolved;
    }

    claim.commit().await?;

    Ok(report)
}

async fn refresh_tenant(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    today: NaiveDate,
) -> Result<(u64, u64), sqlx::Error> {
    let mut resolved = sqlx::query(
        "update alerts a set resolved_at = now()
         where a.tenant_id = $1
           and a.resolved_at is null
           and not exists (
             select 1 from alert_rules r where r.id = a.rule_id and r.is_active
           )",
    )
    .bind(tenant_id)
    .execute(&mut **tx)
    .await?
    .rows_affected();

    let rules: Vec<(Uuid, String, Option<i32>)> = sqlx::query_as(
        "select id, type, days_before from alert_rules
         where tenant_id = $1 and is_active
         order by type, days_before nulls first",
    )
    .bind(tenant_id)
    .fetch_all(&mut **tx)
    .await?;

    let mut created = 0;

    for (rule_id, kind, days_before) in rules {
        let (created_now, resolved_now) = match kind.as_str() {
            "expiry" => {
                expiry_rule(tx, tenant_id, rule_id, days_before.unwrap_or(0), today).await?
            }
            "low_stock" => low_stock_rule(tx, tenant_id, rule_id).await?,
            _ => (0, 0),
        };

        created += created_now;
        resolved += resolved_now;
    }

    Ok((created, resolved))
}

/// Warns for every active batch that expires within `days_before` days. An
/// already-expired batch still warns: it needs action now.
async fn expiry_rule(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    rule_id: Uuid,
    days_before: i32,
    today: NaiveDate,
) -> Result<(u64, u64), sqlx::Error> {
    let created = sqlx::query(
        "insert into alerts (tenant_id, rule_id, type, material_id, batch_id)
         select $1, $2, 'expiry', b.material_id, b.id
         from material_batches b
         where b.tenant_id = $1
           and b.qty_remaining > 0
           and b.expires_at is not null
           and b.expires_at <= $3::date + $4::int
         on conflict do nothing",
    )
    .bind(tenant_id)
    .bind(rule_id)
    .bind(today)
    .bind(days_before)
    .execute(&mut **tx)
    .await?
    .rows_affected();

    // A batch that was consumed or written off needs no warning any more.
    let resolved = sqlx::query(
        "update alerts a set resolved_at = now()
         where a.tenant_id = $1
           and a.rule_id = $2
           and a.resolved_at is null
           and not exists (
             select 1 from material_batches b
             where b.id = a.batch_id and b.qty_remaining > 0
           )",
    )
    .bind(tenant_id)
    .bind(rule_id)
    .execute(&mut **tx)
    .await?
    .rows_affected();

    Ok((created, resolved))
}

/// Warns for every active material whose on-hand stock is below its minimum.
async fn low_stock_rule(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    rule_id: Uuid,
) -> Result<(u64, u64), sqlx::Error> {
    let created = sqlx::query(
        "insert into alerts (tenant_id, rule_id, type, material_id)
         select $1, $2, 'low_stock', m.id
         from materials m
         where m.tenant_id = $1
           and m.is_active
           and m.min_stock > 0
           and coalesce((
                 select sum(b.qty_remaining)
                 from material_batches b
                 where b.tenant_id = m.tenant_id
                   and b.material_id = m.id
                   and b.qty_remaining > 0
               ), 0) < m.min_stock
         on conflict do nothing",
    )
    .bind(tenant_id)
    .bind(rule_id)
    .execute(&mut **tx)
    .await?
    .rows_affected();

    // Restocked or deactivated: the warning is no longer true. A later drop
    // below the minimum opens a fresh alert.
    let resolved = sqlx::query(
        "update alerts a set resolved_at = now()
         where a.tenant_id = $1
           and a.rule_id = $2
           and a.resolved_at is null
           and not exists (
             select 1 from materials m
             where m.id = a.material_id
               and m.tenant_id = a.tenant_id
               and m.is_active
               and m.min_stock > 0
               and coalesce((
                     select sum(b.qty_remaining)
                     from material_batches b
                     where b.tenant_id = m.tenant_id
                       and b.material_id = m.id
                       and b.qty_remaining > 0
                   ), 0) < m.min_stock
           )",
    )
    .bind(tenant_id)
    .bind(rule_id)
    .execute(&mut **tx)
    .await?
    .rows_affected();

    Ok((created, resolved))
}
