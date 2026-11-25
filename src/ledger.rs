//! Validate each export independently before assembling the report.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;

use anyhow::{Result, ensure};

use crate::record::{BillingUnit, EffortRow, UsageRecord, assign_turn_indices};

pub(crate) fn prepare_usage(path: &Path, rows: &mut Vec<UsageRecord>) -> Result<()> {
    rows.sort_by(|a, b| {
        (
            &a.hostname,
            a.timestamp_ms,
            &a.file,
            a.line,
            &a.response_id,
            &a.model,
        )
            .cmp(&(
                &b.hostname,
                b.timestamp_ms,
                &b.file,
                b.line,
                &b.response_id,
                &b.model,
            ))
    });
    collapse_vscode_log_copies(rows);
    let mut requests = HashSet::new();
    let mut turns = HashSet::new();
    let mut summaries = HashSet::new();
    for row in rows.iter() {
        let Some(session) = row.session_id.as_deref().filter(|id| !id.trim().is_empty()) else {
            continue;
        };
        let key = (row.hostname.as_str(), session);
        let duplicate = match row.source.as_str() {
            "vscode.sessionCost" => !summaries.insert(key),
            "vscode.chatSessions" => {
                let duplicate_id = row
                    .request_id
                    .as_deref()
                    .filter(|id| !id.trim().is_empty())
                    .is_some_and(|id| !requests.insert((key, id)));
                let duplicate_turn = row
                    .turn_index
                    .is_some_and(|turn| !turns.insert((key, turn)));
                duplicate_id || duplicate_turn
            }
            _ => false,
        };
        ensure!(
            !duplicate,
            "conflicting usage rows in {} for machine {}, VS Code session {session} (re-export the original log)",
            path.display(),
            row.hostname
        );
    }
    assign_turn_indices(rows);
    Ok(())
}

fn collapse_vscode_log_copies(rows: &mut Vec<UsageRecord>) {
    let mut duplicates = HashSet::new();
    let mut totals = HashMap::new();
    let mut requests = HashMap::new();
    for (index, row) in rows.iter().enumerate() {
        if row.source != "vscode.chatSessions" || row.file.is_empty() {
            continue;
        }
        let (Some(session), Some(request)) = (row.session_id.as_deref(), row.request_id.as_deref())
        else {
            continue;
        };
        if session.trim().is_empty() || request.trim().is_empty() {
            continue;
        }
        let key = (row.hostname.as_str(), session, request);
        if let Some(&first) = requests.get(&key) {
            let original: &UsageRecord = &rows[first];
            if original.file != row.file {
                // A workspace copy can change the source coordinates and project, not what was billed.
                let mut copy = row.clone();
                copy.file.clone_from(&original.file);
                copy.line = original.line;
                copy.project.clone_from(&original.project);
                copy.session_total = original.session_total;
                if copy == *original {
                    duplicates.insert(index);
                    let total = totals.entry(first).or_insert(original.session_total);
                    *total = (*total).max(row.session_total);
                }
            }
        } else {
            requests.insert(key, index);
        }
    }
    if !duplicates.is_empty() {
        for (index, total) in totals {
            rows[index].session_total = total;
        }
        let mut index = 0;
        rows.retain(|_| {
            let keep = !duplicates.contains(&index);
            index += 1;
            keep
        });
    }
}

pub(crate) fn validate_totals(records: &[UsageRecord]) -> Result<()> {
    ensure!(
        records
            .iter()
            .filter(|r| r.amount().0 == BillingUnit::Credits)
            .map(|r| r.credits)
            .sum::<f64>()
            .is_finite(),
        "total AI credits exceed report precision"
    );
    ensure!(
        records
            .iter()
            .filter_map(|r| r.request_count.map(u128::from))
            .sum::<u128>()
            <= 9_007_199_254_740_991,
        "total request count exceeds exact report precision"
    );
    Ok(())
}

pub(crate) fn prepare_effort(path: &Path, mut rows: Vec<EffortRow>) -> Result<Vec<EffortRow>> {
    let mut identified = HashMap::new();
    let mut anonymous = HashSet::new();
    let mut unique = Vec::new();
    rows.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
    for row in rows {
        if let Some(id) = row.event_id.as_deref().filter(|id| !id.trim().is_empty()) {
            let key = (row.source.clone(), row.session_id.clone(), id.to_owned());
            let data = (row.timestamp_ms, row.project.clone());
            if let Some(existing) = identified.get(&key) {
                ensure!(
                    *existing == data,
                    "conflicting effort event for machine {}, session {}, event {id}: {}",
                    row.hostname,
                    row.session_id,
                    path.display()
                );
                continue;
            }
            identified.insert(key, data);
        } else if !anonymous.insert(row.clone()) {
            continue;
        }
        unique.push(row);
    }
    Ok(unique)
}

#[derive(Default)]
pub(crate) struct EffortData {
    pub rows: Vec<EffortRow>,
    pub hosts: BTreeSet<String>,
}

impl EffortData {
    pub fn sort(&mut self) {
        self.rows.sort_by(|a, b| {
            (
                a.timestamp_ms,
                &a.hostname,
                &a.source,
                &a.session_id,
                &a.file,
                a.line,
                &a.event_id,
            )
                .cmp(&(
                    b.timestamp_ms,
                    &b.hostname,
                    &b.source,
                    &b.session_id,
                    &b.file,
                    b.line,
                    &b.event_id,
                ))
        });
    }
}
