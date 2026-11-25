use std::collections::BTreeMap;
use std::path::Path;

use crate::record::{BillingUnit, ScanStats, UsageRecord};
use crate::time::format_local_day;

// Layout: outer width 58, inner content width 54.
pub(crate) const INNER: usize = 54;

pub(crate) fn row(content: String) -> String {
    let len = content.chars().count();
    let padded = if len < INNER {
        format!("{content}{}", " ".repeat(INNER - len))
    } else {
        content.chars().take(INNER).collect()
    };
    format!("| {padded} |\n")
}

pub(crate) fn header(title: &str) -> String {
    let prefix = format!("- {title} ");
    let pad = (INNER + 2).saturating_sub(prefix.chars().count());
    format!("+{prefix}{}+\n", "-".repeat(pad))
}

pub(crate) fn plain_border() -> String {
    format!("+{}+\n", "-".repeat(INNER + 2))
}

pub(crate) fn build_summary_report(
    output: &Path,
    html: Option<&Path>,
    records: &[UsageRecord],
    stats: &ScanStats,
    total_ms: u128,
) -> String {
    let total_credits = normalize_zero(records.iter().map(|record| record.credits).sum());
    let unclassified = records.iter().filter(|r| r.unclassified_usage.is_some()).count();
    let mut daily: BTreeMap<String, (usize, f64)> = BTreeMap::new();

    for record in records {
        if record.amount().0 != BillingUnit::Credits { continue }
        let day = record
            .timestamp_ms
            .and_then(format_local_day)
            .unwrap_or_else(|| "unknown-date".to_owned());
        let entry = daily.entry(day).or_default();
        entry.0 += 1;
        entry.1 += record.credits;
    }

    let active_days = daily
        .keys()
        .filter(|day| day.as_str() != "unknown-date")
        .count();
    let average_per_active_day = if active_days > 0 {
        normalize_zero(total_credits / active_days as f64)
    } else {
        0.0
    };

    let kv2 = |lk: &str, lv: String, rk: &str, rv: String| -> String {
        // 14 label + 12 value + 2 sep + 14 label + 12 value = 54
        row(format!("{:<14}{:>12}  {:<14}{:>12}", lk, lv, rk, rv))
    };

    let mut report = String::new();
    report.push_str(&header("GitHub Copilot Usage"));
    report.push_str(&kv2(
        "records",
        records.len().to_string(),
        "scanned files",
        stats.scanned_files.to_string(),
    ));
    report.push_str(&kv2(
        "total credits",
        format!("{:.1}", total_credits),
        "candidate lines",
        stats.json_candidate_lines.to_string(),
    ));
    report.push_str(&kv2(
        "active days",
        active_days.to_string(),
        "parse errors",
        stats.parse_errors.to_string(),
    ));
    report.push_str(&kv2(
        "avg / day",
        format!("{:.1}", average_per_active_day),
        "total time",
        format!("{:.2} s", total_ms as f64 / 1000.0),
    ));
    for summary in historical_request_summary(records) { report.push_str(&row(summary)); }
    if unclassified > 0 {
        report.push_str(&row(format!("unit unverified: {unclassified} rows")));
    }

    if !daily.is_empty() {
        report.push_str(&header("Daily credits"));
        for (day, (count, credits)) in &daily {
            // 12 date + 6 records right + 11 " records   " + 12 credits right + 13 " credits   " = 54
            report.push_str(&row(format!(
                "{:<12}{:>5} records   {:>10.2} credits     ",
                day, count, credits
            )));
        }
    }

    report.push_str(&header("Files"));
    report.push_str(&row(format!("csv   {}", clip(&output.display().to_string(), INNER - 6))));
    if let Some(html) = html {
        report.push_str(&row(format!("html  {}", clip(&html.display().to_string(), INNER - 6))));
    } else {
        report.push_str(&row("html  (disabled via --no-html)".to_owned()));
    }
    report.push_str(&plain_border());
    report
}

pub(crate) fn historical_request_summary(records: &[UsageRecord]) -> Vec<String> {
    let mut counts = [0u64; 2];
    let mut seen = [false; 2];
    for record in records {
        if let Some(count) = record.request_count {
            let source = usize::from(record.source.starts_with("copilot.cli"));
            counts[source] = counts[source].saturating_add(count);
            seen[source] = true;
        }
    }
    ["VS Code requests", "CLI model calls"].into_iter().enumerate()
        .filter(|(i, _)| seen[*i]).map(|(i, label)| format!("history — {label}: {}", counts[i])).collect()
}

pub(crate) fn clip(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        return s.to_owned();
    }
    // Keep tail (filename) visible; replace head with "…".
    let skip = count - (max - 1);
    let mut out = String::from("…");
    out.extend(s.chars().skip(skip));
    out
}

pub(crate) fn normalize_zero(value: f64) -> f64 {
    if value.abs() < f64::EPSILON {
        0.0
    } else {
        value
    }
}