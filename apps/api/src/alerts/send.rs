//! Delivers alert digests through the configured transports.
//!
//! One message per tenant and channel. Every attempt is recorded, so a provider
//! outage leaves a trail instead of a hole, and the next run retries what is
//! still pending.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use sqlx::PgPool;
use takar_db::alert_delivery::{self, Outcome, PendingAlert};
use uuid::Uuid;

use crate::notify::{Channel, Router};

use super::message::{self, Digest};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SendReport {
    pub sent: u64,
    pub failed: u64,
    pub skipped: u64,
}

pub async fn send_pending(
    pool: &PgPool,
    router: &Router,
    app_url: &str,
    today: NaiveDate,
) -> anyhow::Result<SendReport> {
    let pending = alert_delivery::pending(pool).await?;

    if pending.is_empty() {
        return Ok(SendReport::default());
    }

    let mut groups: BTreeMap<(Uuid, String, Option<String>), Vec<PendingAlert>> = BTreeMap::new();

    for alert in pending {
        let key = (
            alert.tenant_id,
            alert.channel.clone(),
            alert.recipient().map(str::to_string),
        );

        groups.entry(key).or_default().push(alert);
    }

    let mut report = SendReport::default();

    for ((tenant_id, channel_name, recipient), alerts) in groups {
        let ids: Vec<Uuid> = alerts.iter().map(|alert| alert.id).collect();
        let count = ids.len() as u64;

        let Some(recipient) = recipient else {
            tracing::warn!(%tenant_id, channel = channel_name, "no recipient, alerts skipped");
            alert_delivery::record(
                pool,
                &ids,
                &channel_name,
                None,
                Outcome::Skipped("no_recipient"),
            )
            .await?;
            report.skipped += count;
            continue;
        };

        let Some(channel) = Channel::parse(&channel_name) else {
            alert_delivery::record(
                pool,
                &ids,
                &channel_name,
                Some(&recipient),
                Outcome::Skipped("unknown_channel"),
            )
            .await?;
            report.skipped += count;
            continue;
        };

        let digest = Digest {
            tenant: &alerts[0].tenant_name,
            today,
            app_url,
            alerts: &alerts,
        };

        match router
            .send(
                channel,
                &recipient,
                &message::subject(&digest),
                &message::compose(&digest),
            )
            .await
        {
            Ok(()) => {
                tracing::info!(%tenant_id, channel = channel_name, alerts = count, "alert digest sent");
                alert_delivery::record(pool, &ids, &channel_name, Some(&recipient), Outcome::Sent)
                    .await?;
                report.sent += count;
            }
            Err(error) => {
                tracing::error!(%tenant_id, channel = channel_name, error = %error, "alert digest failed");
                alert_delivery::record(
                    pool,
                    &ids,
                    &channel_name,
                    Some(&recipient),
                    Outcome::Failed(&error.message),
                )
                .await?;
                report.failed += count;
            }
        }
    }

    Ok(report)
}
