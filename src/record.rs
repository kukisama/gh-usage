//! Usage and submission records shared by scanners and exporters.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct UsageRecord {
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub hostname: String,
    #[serde(default)]
    pub timestamp_ms: Option<i64>,
    #[serde(default)]
    pub local_time_hint: Option<String>,
    #[serde(default)]
    pub chat_title: Option<String>,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub credits: f64,
    #[serde(default)]
    pub request_count: Option<u64>,
    // Read for compatibility with the transitional weighted CSV schema.
    #[serde(default)]
    pub premium_requests: Option<f64>,
    #[serde(default)]
    pub unclassified_usage: Option<f64>,
    #[serde(default)]
    pub details: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub response_id: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
    /// Workspace folder name this session belongs to (derived from workspace.json).
    #[serde(default)]
    pub project: Option<String>,
    /// Wall-clock duration of this single exchange in milliseconds (timings.totalElapsed).
    #[serde(default)]
    pub duration_ms: Option<i64>,
    /// 1-based position of this exchange within its parent session.
    #[serde(default)]
    pub turn_index: Option<u32>,
    /// Total number of exchanges recorded for the parent session.
    #[serde(default)]
    pub session_total: Option<u32>,
    #[serde(default)]
    pub file: String,
    #[serde(default)]
    pub line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum BillingUnit {
    Credits,
    RequestCount,
    Unclassified,
}

impl UsageRecord {
    pub(crate) fn amount(&self) -> (BillingUnit, f64) {
        if let Some(value) = self.request_count { (BillingUnit::RequestCount, value as f64) }
        else if let Some(value) = self.unclassified_usage { (BillingUnit::Unclassified, value) }
        else { (BillingUnit::Credits, self.credits) }
    }

    pub(crate) fn set_amount(&mut self, unit: BillingUnit, value: f64) {
        self.credits = if unit == BillingUnit::Credits { value } else { 0.0 };
        self.request_count = (unit == BillingUnit::RequestCount).then_some(value as u64);
        self.premium_requests = None;
        self.unclassified_usage = (unit == BillingUnit::Unclassified).then_some(value);
    }
}

#[derive(Debug, Default)]
pub(crate) struct ScanStats {
    pub scanned_files: usize,
    pub scanned_lines: usize,
    pub json_candidate_lines: usize,
    pub parse_errors: usize,
}

#[derive(Debug, Default)]
pub(crate) struct FileScanResult {
    pub records: Vec<UsageRecord>,
    pub effort: Vec<EffortEvent>,
    pub scanned_lines: usize,
    pub json_candidate_lines: usize,
    pub parse_errors: usize,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ContextFields {
    pub session_id: Option<String>,
    pub request_id: Option<String>,
    pub response_id: Option<String>,
    pub model_id: Option<String>,
    pub agent_id: Option<String>,
    pub timestamp_ms: Option<i64>,
    pub total_elapsed_ms: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EffortKind {
    Human,
    Automatic,
    Unverified,
    Queued,
}

#[derive(Debug)]
pub(crate) struct EffortEvent {
    pub source: &'static str,
    pub session: String,
    pub id: Option<String>,
    pub project: Option<String>,
    pub timestamp_ms: Option<i64>,
    pub file: PathBuf,
    pub line: usize,
    pub kind: EffortKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct EffortRow {
    pub hostname: String,
    pub timestamp_ms: i64,
    pub source: String,
    pub project: Option<String>,
    pub session_id: String,
    pub event_id: Option<String>,
    #[serde(default)]
    pub file: String,
    #[serde(default)]
    pub line: usize,
}

/// Assigns a 1-based `turn_index` within each parent session plus the session's
/// total exchange count. Records must already be ordered chronologically within
/// each `(hostname, session_id)` group, which both the scan and merge sorts
/// guarantee.
pub(crate) fn assign_turn_indices(records: &mut [UsageRecord]) {
    // Replayed VS Code and SQLite-backed CLI records carry user-facing turns.
    // Preserve those values; only synthesize sequential positions for legacy
    // VS Code/CLI records where no turn was recorded at all.
    let mut explicit_totals: HashMap<(String, String), u32> = HashMap::new();
    let mut implicit_totals: HashMap<(String, String), u32> = HashMap::new();
    for record in records.iter() {
        if is_session_summary(record) {
            continue;
        }
        if let Some(session) = record.session_id.as_ref() {
            let key = (record.hostname.clone(), session.clone());
            if let Some(turn) = record.turn_index {
                let known_total = record.session_total.unwrap_or(turn).max(turn);
                explicit_totals
                    .entry(key)
                    .and_modify(|total| *total = (*total).max(known_total))
                    .or_insert(known_total);
            } else {
                *implicit_totals.entry(key).or_default() += 1;
            }
        }
    }
    let mut seen: HashMap<(String, String), u32> = HashMap::new();
    for record in records.iter_mut() {
        if is_session_summary(record) {
            record.turn_index = None;
            record.session_total = None;
            continue;
        }
        let Some(session) = record.session_id.clone() else {
            record.turn_index = None;
            record.session_total = None;
            continue;
        };
        let key = (record.hostname.clone(), session);
        if record.turn_index.is_some() {
            record.session_total = explicit_totals.get(&key).copied();
            continue;
        }
        let counter = seen.entry(key.clone()).or_default();
        *counter += 1;
        record.turn_index = Some(*counter);
        record.session_total = implicit_totals.get(&key).copied();
    }
}

fn is_session_summary(record: &UsageRecord) -> bool {
    record.source == "vscode.sessionCost" || record.source.starts_with("copilot.cli")
        && (record.details.contains("session shutdown")
            || (record.source == "copilot.cli.probe" && record.request_id.is_none()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_explicit_cli_turn_indices() {
        let mut records = vec![UsageRecord {
            source: "copilot.cli.sessionStore".to_owned(),
            hostname: "host".to_owned(),
            timestamp_ms: Some(1),
            local_time_hint: None,
            chat_title: None,
            model: "model".to_owned(),
            model_id: None,
            credits: 1.0,
            request_count: None,
            premium_requests: None,
            unclassified_usage: None,
            details: String::new(),
            session_id: Some("session".to_owned()),
            request_id: None,
            response_id: None,
            agent_id: None,
            project: None,
            duration_ms: None,
            turn_index: Some(3),
            session_total: Some(3),
            file: String::new(),
            line: 1,
        }];

        assign_turn_indices(&mut records);

        assert_eq!(records[0].turn_index, Some(3));
        assert_eq!(records[0].session_total, Some(3));
    }
}