//! Whether a tenant may still write.
//!
//! Access is derived from dates, never from a stored flag: `trial_ends_at` for
//! the free trial and `paid_until` for a verified payment. Deriving it means a
//! support extension or a late verification cannot leave a stale status behind.
//!
//! A tenant keeps writing for `GRACE_DAYS` after its entitlement runs out. Only
//! after that does the API refuse writes; reads stay open and no data is ever
//! deleted, so paying again restores the account exactly as it was.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Days a tenant keeps full access after the trial or paid period ends.
pub const GRACE_DAYS: i64 = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    /// Inside the trial or a paid period.
    Active,
    /// Past the end date but inside the grace window.
    Grace,
    /// Past the grace window: reads only.
    ReadOnly,
}

impl Access {
    pub const fn as_str(self) -> &'static str {
        match self {
            Access::Active => "active",
            Access::Grace => "grace",
            Access::ReadOnly => "read_only",
        }
    }

    /// Whether the tenant may still mutate data.
    pub const fn allows_writes(self) -> bool {
        !matches!(self, Access::ReadOnly)
    }
}

/// The moment access runs out: the later of the trial and a paid period.
///
/// Taking the later date means an early payment, or support extending a trial,
/// can only ever lengthen access — never cut it short.
pub fn ends_at(
    trial_ends_at: Option<DateTime<Utc>>,
    paid_until: Option<DateTime<Utc>>,
) -> Option<DateTime<Utc>> {
    match (trial_ends_at, paid_until) {
        (Some(trial), Some(paid)) => Some(trial.max(paid)),
        (Some(date), None) | (None, Some(date)) => Some(date),
        (None, None) => None,
    }
}

/// Access at `now`.
///
/// A tenant with no date at all stays active: locking out an account we cannot
/// date would be a bug, not a policy.
pub fn access(
    trial_ends_at: Option<DateTime<Utc>>,
    paid_until: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Access {
    let Some(end) = ends_at(trial_ends_at, paid_until) else {
        return Access::Active;
    };

    if now < end {
        Access::Active
    } else if now < end + Duration::days(GRACE_DAYS) {
        Access::Grace
    } else {
        Access::ReadOnly
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(day: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 3, day, 9, 0, 0).unwrap()
    }

    #[test]
    fn a_tenant_without_a_date_is_never_locked_out() {
        assert_eq!(access(None, None, at(31)), Access::Active);
    }

    #[test]
    fn the_last_second_inside_the_period_is_still_active() {
        assert_eq!(
            access(Some(at(10)), None, at(10) - Duration::seconds(1)),
            Access::Active
        );
    }

    #[test]
    fn the_end_date_itself_starts_the_grace_window() {
        assert_eq!(access(Some(at(10)), None, at(10)), Access::Grace);
    }

    #[test]
    fn grace_lasts_exactly_seven_days() {
        let end = at(10);

        assert_eq!(
            access(Some(end), None, end + Duration::days(6)),
            Access::Grace
        );
        assert_eq!(
            access(Some(end), None, end + Duration::days(7)),
            Access::ReadOnly
        );
    }

    #[test]
    fn a_paid_period_wins_over_an_earlier_trial() {
        // Trial ended long ago, payment covers the queried day.
        assert_eq!(access(Some(at(1)), Some(at(28)), at(20)), Access::Active);
    }

    #[test]
    fn the_later_date_always_decides() {
        // Support extended the trial past the paid period.
        assert_eq!(ends_at(Some(at(30)), Some(at(10))), Some(at(30)));
        assert_eq!(ends_at(Some(at(10)), Some(at(30))), Some(at(30)));
        assert_eq!(ends_at(None, Some(at(30))), Some(at(30)));
    }

    #[test]
    fn only_read_only_refuses_writes() {
        assert!(Access::Active.allows_writes());
        assert!(Access::Grace.allows_writes());
        assert!(!Access::ReadOnly.allows_writes());
    }

    #[test]
    fn access_names_round_trip_through_serde() {
        for access in [Access::Active, Access::Grace, Access::ReadOnly] {
            let encoded = serde_json::to_string(&access).expect("encode");
            assert_eq!(encoded, format!("\"{}\"", access.as_str()));
        }
    }
}
