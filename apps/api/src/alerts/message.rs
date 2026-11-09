//! Alert message composition.
//!
//! Alerts store structured data; this is the only place that turns them into
//! prose. Pure on purpose, so the format is covered by tests instead of by
//! reading a phone.

use std::collections::HashSet;

use chrono::{Datelike, NaiveDate};
use rust_decimal::Decimal;
use takar_db::alert_delivery::PendingAlert;

/// Long digests get cut; nobody reads twenty lines on a lock screen.
const MAX_ITEMS_PER_SECTION: usize = 5;

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "Mei", "Jun", "Jul", "Agu", "Sep", "Okt", "Nov", "Des",
];

pub struct Digest<'a> {
    pub tenant: &'a str,
    pub today: NaiveDate,
    pub app_url: &'a str,
    pub alerts: &'a [PendingAlert],
}

/// One message per tenant and channel, not one per alert: five notifications
/// about five ingredients is noise, and every message costs quota.
pub fn compose(digest: &Digest<'_>) -> String {
    let mut text = format!("Takar · {}\n{}\n", digest.tenant, long_date(digest.today));

    let (expiry, low_stock) = sections(digest.alerts);

    if !expiry.is_empty() {
        text.push_str("\nKedaluwarsa\n");
        text.push_str(&section(&expiry, expiry_line));
    }

    if !low_stock.is_empty() {
        text.push_str("\nStok minimum\n");
        text.push_str(&section(&low_stock, low_stock_line));
    }

    text.push_str(&format!(
        "\nBuka stok: {}/stock\n",
        digest.app_url.trim_end_matches('/')
    ));

    text
}

pub fn subject(digest: &Digest<'_>) -> String {
    let (expiry, low_stock) = sections(digest.alerts);

    format!("Takar · {} peringatan stok", expiry.len() + low_stock.len())
}

/// Splits alerts into sections, dropping repeats.
///
/// The same batch warns at H-7 and again at H-3, under two different rules. The
/// text does not name the rule, so listing it twice would read as a duplicate.
fn sections(alerts: &[PendingAlert]) -> (Vec<&PendingAlert>, Vec<&PendingAlert>) {
    let mut seen = HashSet::new();
    let mut expiry = Vec::new();
    let mut low_stock = Vec::new();

    for alert in alerts {
        let key = (
            alert.alert_type.as_str(),
            alert.material_name.as_str(),
            alert.expires_at,
            alert.qty_remaining,
        );

        if !seen.insert(key) {
            continue;
        }

        match alert.alert_type.as_str() {
            "expiry" => expiry.push(alert),
            "low_stock" => low_stock.push(alert),
            _ => {}
        }
    }

    (expiry, low_stock)
}

fn section(alerts: &[&PendingAlert], line: fn(&PendingAlert) -> String) -> String {
    let mut text = String::new();

    for alert in alerts.iter().take(MAX_ITEMS_PER_SECTION) {
        text.push_str("• ");
        text.push_str(&line(alert));
        text.push('\n');
    }

    if alerts.len() > MAX_ITEMS_PER_SECTION {
        text.push_str(&format!(
            "…dan {} lainnya\n",
            alerts.len() - MAX_ITEMS_PER_SECTION
        ));
    }

    text
}

fn expiry_line(alert: &PendingAlert) -> String {
    let qty = alert
        .qty_remaining
        .map(quantity)
        .unwrap_or_else(|| "-".to_string());

    let expires = alert
        .expires_at
        .map(short_date)
        .unwrap_or_else(|| "-".to_string());

    format!(
        "{} {} {} · {} ({})",
        alert.material_name,
        qty,
        alert.unit_code,
        expires,
        countdown(alert.days_left)
    )
}

fn low_stock_line(alert: &PendingAlert) -> String {
    let on_hand = alert
        .on_hand
        .map(quantity)
        .unwrap_or_else(|| "0".to_string());

    format!(
        "{} {} {} · minimum {} {}",
        alert.material_name,
        on_hand,
        alert.unit_code,
        quantity(alert.min_stock),
        alert.unit_code
    )
}

fn countdown(days_left: Option<i32>) -> String {
    match days_left {
        Some(days) if days < 0 => format!("lewat {} hari", days.abs()),
        Some(0) => "hari ini".to_string(),
        Some(days) => format!("H-{days}"),
        None => "tanpa tanggal".to_string(),
    }
}

/// Indonesian formatting: comma decimals, no trailing zeros.
fn quantity(value: Decimal) -> String {
    value.normalize().to_string().replace('.', ",")
}

fn short_date(date: NaiveDate) -> String {
    format!("{} {}", date.day(), MONTHS[date.month0() as usize])
}

fn long_date(date: NaiveDate) -> String {
    format!(
        "{} {} {}",
        date.day(),
        MONTHS[date.month0() as usize],
        date.year()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn alert(alert_type: &str, name: &str) -> PendingAlert {
        PendingAlert {
            id: Uuid::nil(),
            tenant_id: Uuid::nil(),
            tenant_name: "Dapur Uji".to_string(),
            alert_type: alert_type.to_string(),
            material_name: name.to_string(),
            unit_code: "kg".to_string(),
            channel: "whatsapp".to_string(),
            owner_phone: Some("+6281234567890".to_string()),
            owner_email: None,
            qty_remaining: Some(Decimal::new(2500, 3)),
            expires_at: Some(NaiveDate::from_ymd_opt(2026, 11, 10).expect("date")),
            days_left: Some(2),
            on_hand: Some(Decimal::new(2000, 3)),
            min_stock: Decimal::new(6000, 3),
            attempts: 0,
        }
    }

    fn digest<'a>(alerts: &'a [PendingAlert]) -> Digest<'a> {
        Digest {
            tenant: "Dapur Uji",
            today: NaiveDate::from_ymd_opt(2026, 11, 9).expect("date"),
            app_url: "https://takar.arifinoid.web.id/",
            alerts,
        }
    }

    #[test]
    fn message_groups_expiry_and_stock_under_their_own_headings() {
        let alerts = vec![
            alert("expiry", "Cokelat compound"),
            alert("low_stock", "Tepung terigu"),
        ];
        let text = compose(&digest(&alerts));

        assert!(text.starts_with("Takar · Dapur Uji\n9 Nov 2026\n"));
        assert!(text.contains("Kedaluwarsa\n• Cokelat compound 2,5 kg · 10 Nov (H-2)"));
        assert!(text.contains("Stok minimum\n• Tepung terigu 2 kg · minimum 6 kg"));
        assert!(text.contains("Buka stok: https://takar.arifinoid.web.id/stock"));
    }

    #[test]
    fn message_omits_a_section_with_nothing_in_it() {
        let alerts = vec![alert("low_stock", "Tepung terigu")];
        let text = compose(&digest(&alerts));

        assert!(!text.contains("Kedaluwarsa"));
        assert!(text.contains("Stok minimum"));
    }

    #[test]
    fn message_cuts_a_long_section_and_says_how_many_are_left() {
        let alerts: Vec<PendingAlert> = (0..7)
            .map(|index| alert("expiry", &format!("Bahan {index}")))
            .collect();

        let text = compose(&digest(&alerts));

        assert!(text.contains("Bahan 4"));
        assert!(!text.contains("Bahan 5"), "only five items are listed");
        assert!(text.contains("…dan 2 lainnya"));
    }

    #[test]
    fn countdown_reads_naturally_in_every_direction() {
        assert_eq!(countdown(Some(3)), "H-3");
        assert_eq!(countdown(Some(0)), "hari ini");
        assert_eq!(countdown(Some(-2)), "lewat 2 hari");
        assert_eq!(countdown(None), "tanpa tanggal");
    }

    #[test]
    fn quantities_drop_trailing_zeros_and_use_a_comma() {
        assert_eq!(quantity(Decimal::new(2500, 3)), "2,5");
        assert_eq!(quantity(Decimal::new(2000, 3)), "2");
        assert_eq!(quantity(Decimal::new(1899100, 2)), "18991");
        assert_eq!(quantity(Decimal::ZERO), "0");
    }

    #[test]
    fn one_batch_warned_by_two_rules_is_listed_once() {
        let alerts = vec![
            alert("expiry", "Cokelat compound"),
            alert("expiry", "Cokelat compound"),
        ];
        let text = compose(&digest(&alerts));

        assert_eq!(text.matches("Cokelat compound").count(), 1);
        assert_eq!(subject(&digest(&alerts)), "Takar · 1 peringatan stok");
    }

    #[test]
    fn subject_counts_the_alerts() {
        let alerts = vec![alert("expiry", "Cokelat compound"), alert("expiry", "Keju")];
        assert_eq!(subject(&digest(&alerts)), "Takar · 2 peringatan stok");
    }
}
