use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Local, Utc};

pub(crate) fn cutoff_system_time(days: u64) -> SystemTime {
    SystemTime::now() - Duration::from_secs(days.saturating_mul(24 * 60 * 60))
}

pub(crate) fn system_time_to_unix_ms(time: &SystemTime) -> Option<i64> {
    let millis = time.duration_since(UNIX_EPOCH).ok()?.as_millis();
    i64::try_from(millis).ok()
}

pub(crate) fn parse_rfc3339_timestamp_ms(timestamp: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(timestamp)
        .ok()
        .map(|time| time.timestamp_millis())
}

pub(crate) fn format_local_time(timestamp_ms: i64) -> Option<String> {
    let utc = DateTime::<Utc>::from_timestamp_millis(timestamp_ms)?;
    Some(
        utc.with_timezone(&Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string(),
    )
}

pub(crate) fn format_local_day(timestamp_ms: i64) -> Option<String> {
    let utc = DateTime::<Utc>::from_timestamp_millis(timestamp_ms)?;
    Some(utc.with_timezone(&Local).format("%Y-%m-%d").to_string())
}