mod common;

use common::{signup, tenant_of, test_app, CaptureOtp};
use sqlx::PgPool;

#[sqlx::test(migrations = "../../migrations")]
async fn a_new_tenant_starts_with_the_default_alert_rules(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let tenant_id = tenant_of(&app, token).await;

    let rules: Vec<(String, Option<i32>, String)> = sqlx::query_as(
        "select type, days_before, channel
         from alert_rules
         where tenant_id = $1
         order by type, days_before nulls first",
    )
    .bind(tenant_id)
    .fetch_all(&pool)
    .await
    .expect("rules");

    assert_eq!(
        rules,
        vec![
            ("expiry".to_string(), Some(3), "whatsapp".to_string()),
            ("expiry".to_string(), Some(7), "whatsapp".to_string()),
            ("low_stock".to_string(), None, "whatsapp".to_string()),
        ]
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn every_tenant_gets_its_own_rules(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());

    let mut tenants = Vec::new();
    for contact in ["first@example.test", "second@example.test"] {
        let owner = signup(&app, &otp, contact).await;
        let token = owner["access_token"].as_str().expect("token");
        tenants.push(tenant_of(&app, token).await);
    }

    for tenant_id in tenants {
        let (count,): (i64,) =
            sqlx::query_as("select count(*) from alert_rules where tenant_id = $1")
                .bind(tenant_id)
                .fetch_one(&pool)
                .await
                .expect("count");

        assert_eq!(count, 3);
    }
}
