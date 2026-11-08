use chrono::NaiveDate;
use sqlx::types::Decimal;
use sqlx::PgPool;
use takar_db::alerts::{run_on, seed_default_rules, AlertRun};
use uuid::Uuid;

fn date(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).expect("valid date")
}

async fn tenant_with_defaults(pool: &PgPool) -> Uuid {
    let tenant_id = Uuid::new_v4();

    sqlx::query("insert into tenants (id, name) values ($1, 'Test Tenant')")
        .bind(tenant_id)
        .execute(pool)
        .await
        .expect("tenant");

    let mut tx = pool.begin().await.expect("transaction");
    seed_default_rules(&mut tx, tenant_id)
        .await
        .expect("default rules");
    tx.commit().await.expect("commit");

    tenant_id
}

async fn add_material(pool: &PgPool, tenant_id: Uuid, name: &str, min_stock: &str) -> Uuid {
    let (unit_id,): (Uuid,) = sqlx::query_as("select id from units where code = 'kg'")
        .fetch_one(pool)
        .await
        .expect("unit");

    let id = Uuid::new_v4();
    let min_stock: Decimal = min_stock.parse().expect("decimal");

    sqlx::query(
        "insert into materials (id, tenant_id, name, base_unit_id, min_stock)
         values ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(name)
    .bind(unit_id)
    .bind(min_stock)
    .execute(pool)
    .await
    .expect("material");

    id
}

async fn add_batch(
    pool: &PgPool,
    tenant_id: Uuid,
    material_id: Uuid,
    qty: &str,
    expires_at: Option<NaiveDate>,
) -> Uuid {
    let id = Uuid::new_v4();
    let qty: Decimal = qty.parse().expect("decimal");

    sqlx::query(
        "insert into material_batches
             (id, tenant_id, material_id, received_at, expires_at, qty_received, qty_remaining, unit_cost)
         values ($1, $2, $3, $4, $5, $6, $6, 1000)",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(material_id)
    .bind(date(2026, 1, 1))
    .bind(expires_at)
    .bind(qty)
    .execute(pool)
    .await
    .expect("batch");

    id
}

/// Open alerts as (rule type, days before, batch id).
async fn open_alerts(pool: &PgPool, tenant_id: Uuid) -> Vec<(String, Option<i32>, Option<Uuid>)> {
    sqlx::query_as(
        "select r.type, r.days_before, a.batch_id
         from alerts a
         join alert_rules r on r.id = a.rule_id
         where a.tenant_id = $1 and a.resolved_at is null
         order by r.type, r.days_before, a.batch_id",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .expect("alerts")
}

async fn total_alerts(pool: &PgPool, tenant_id: Uuid) -> i64 {
    let (count,): (i64,) = sqlx::query_as("select count(*) from alerts where tenant_id = $1")
        .bind(tenant_id)
        .fetch_one(pool)
        .await
        .expect("count");

    count
}

#[sqlx::test(migrations = "../../migrations")]
async fn expiry_alerts_fire_at_the_configured_thresholds(pool: PgPool) {
    let today = date(2026, 1, 10);
    let tenant_id = tenant_with_defaults(&pool).await;
    let material_id = add_material(&pool, tenant_id, "Tepung", "0").await;

    let at_seven_days =
        add_batch(&pool, tenant_id, material_id, "1", Some(date(2026, 1, 17))).await;
    let at_three_days =
        add_batch(&pool, tenant_id, material_id, "1", Some(date(2026, 1, 13))).await;
    let later = add_batch(&pool, tenant_id, material_id, "1", Some(date(2026, 1, 21))).await;

    let report: AlertRun = run_on(&pool, today).await.expect("run");
    assert_eq!(
        report,
        AlertRun {
            tenants: 1,
            created: 3,
            resolved: 0
        }
    );

    let alerts = open_alerts(&pool, tenant_id).await;
    assert_eq!(alerts.len(), 3);
    assert!(alerts.contains(&("expiry".into(), Some(7), Some(at_seven_days))));
    assert!(alerts.contains(&("expiry".into(), Some(3), Some(at_three_days))));
    assert!(alerts.contains(&("expiry".into(), Some(7), Some(at_three_days))));
    assert!(
        alerts.iter().all(|(_, _, batch)| *batch != Some(later)),
        "a batch beyond every threshold is not alerted"
    );

    let again = run_on(&pool, today).await.expect("second run");
    assert_eq!(again.created, 0, "a second run must not duplicate");
    assert_eq!(total_alerts(&pool, tenant_id).await, 3);
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_already_expired_batch_still_alerts(pool: PgPool) {
    let today = date(2026, 1, 10);
    let tenant_id = tenant_with_defaults(&pool).await;
    let material_id = add_material(&pool, tenant_id, "Susu", "0").await;
    let batch_id = add_batch(&pool, tenant_id, material_id, "1", Some(date(2026, 1, 1))).await;

    let report = run_on(&pool, today).await.expect("run");

    assert_eq!(report.created, 2, "both the H-7 and H-3 rules fire");
    let alerts = open_alerts(&pool, tenant_id).await;
    assert_eq!(alerts.len(), 2);
    assert!(alerts.iter().all(|(_, _, batch)| *batch == Some(batch_id)));
}

#[sqlx::test(migrations = "../../migrations")]
async fn low_stock_alerts_resolve_and_reopen(pool: PgPool) {
    let today = date(2026, 1, 10);
    let tenant_id = tenant_with_defaults(&pool).await;
    let material_id = add_material(&pool, tenant_id, "Gula", "5").await;
    add_batch(&pool, tenant_id, material_id, "2", None).await;

    let first = run_on(&pool, today).await.expect("run");
    assert_eq!(first.created, 1);
    assert_eq!(open_alerts(&pool, tenant_id).await.len(), 1);

    let second = run_on(&pool, today).await.expect("second run");
    assert_eq!(second.created, 0);
    assert_eq!(open_alerts(&pool, tenant_id).await.len(), 1);

    let restock = add_batch(&pool, tenant_id, material_id, "10", None).await;
    let recovered = run_on(&pool, today).await.expect("run after restock");
    assert_eq!(recovered.resolved, 1);
    assert!(
        open_alerts(&pool, tenant_id).await.is_empty(),
        "stock above the minimum closes the warning"
    );

    sqlx::query("update material_batches set qty_remaining = 0 where id = $1")
        .bind(restock)
        .execute(&pool)
        .await
        .expect("deplete");

    let dropped_again = run_on(&pool, today).await.expect("run after a new drop");
    assert_eq!(dropped_again.created, 1);
    assert_eq!(open_alerts(&pool, tenant_id).await.len(), 1);
    assert_eq!(
        total_alerts(&pool, tenant_id).await,
        2,
        "history keeps the resolved alert"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn depleting_a_batch_resolves_its_expiry_alerts(pool: PgPool) {
    let today = date(2026, 1, 10);
    let tenant_id = tenant_with_defaults(&pool).await;
    let material_id = add_material(&pool, tenant_id, "Beras", "0").await;
    let batch_id = add_batch(&pool, tenant_id, material_id, "1", Some(date(2026, 1, 12))).await;

    run_on(&pool, today).await.expect("run");
    assert_eq!(open_alerts(&pool, tenant_id).await.len(), 2);

    sqlx::query("update material_batches set qty_remaining = 0 where id = $1")
        .bind(batch_id)
        .execute(&pool)
        .await
        .expect("deplete");

    let report = run_on(&pool, today).await.expect("run after depletion");
    assert_eq!(report.resolved, 2);
    assert!(open_alerts(&pool, tenant_id).await.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_disabled_rule_resolves_its_open_alerts(pool: PgPool) {
    let today = date(2026, 1, 10);
    let tenant_id = tenant_with_defaults(&pool).await;
    let material_id = add_material(&pool, tenant_id, "Cabai", "5").await;
    add_batch(&pool, tenant_id, material_id, "1", None).await;

    run_on(&pool, today).await.expect("run");
    assert_eq!(open_alerts(&pool, tenant_id).await.len(), 1);

    sqlx::query("update alert_rules set is_active = false where tenant_id = $1")
        .bind(tenant_id)
        .execute(&pool)
        .await
        .expect("disable rules");

    let report = run_on(&pool, today).await.expect("run after disabling");
    assert_eq!(report.resolved, 1);
    assert!(open_alerts(&pool, tenant_id).await.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn materials_at_or_above_the_minimum_do_not_alert(pool: PgPool) {
    let today = date(2026, 1, 10);
    let tenant_id = tenant_with_defaults(&pool).await;

    add_material(&pool, tenant_id, "Garam", "0").await;

    let exactly_minimum = add_material(&pool, tenant_id, "Minyak", "3").await;
    add_batch(&pool, tenant_id, exactly_minimum, "3", None).await;

    let report = run_on(&pool, today).await.expect("run");

    assert_eq!(report.created, 0);
    assert!(open_alerts(&pool, tenant_id).await.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn tenants_are_processed_independently(pool: PgPool) {
    let today = date(2026, 1, 10);
    let mut tenants = Vec::new();

    for (index, name) in ["First", "Second"].iter().enumerate() {
        let tenant_id = tenant_with_defaults(&pool).await;
        let material_id = add_material(&pool, tenant_id, name, "5").await;
        add_batch(
            &pool,
            tenant_id,
            material_id,
            &(index + 1).to_string(),
            None,
        )
        .await;
        tenants.push(tenant_id);
    }

    let report = run_on(&pool, today).await.expect("run");

    assert_eq!(report.tenants, 2);
    assert_eq!(report.created, 2);

    for tenant_id in tenants {
        assert_eq!(open_alerts(&pool, tenant_id).await.len(), 1);
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn parallel_runs_do_not_duplicate_alerts(pool: PgPool) {
    let today = date(2026, 1, 10);
    let tenant_id = tenant_with_defaults(&pool).await;
    let material_id = add_material(&pool, tenant_id, "Telur", "10").await;
    add_batch(&pool, tenant_id, material_id, "1", None).await;

    let (left, right) = tokio::join!(run_on(&pool, today), run_on(&pool, today));
    let left = left.expect("left run");
    let right = right.expect("right run");

    assert_eq!(
        left.created + right.created,
        1,
        "exactly one run creates the alert"
    );
    assert_eq!(open_alerts(&pool, tenant_id).await.len(), 1);
}
