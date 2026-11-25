//! Copilot CLI usage, event provenance, and workspace metadata.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};
use serde_json::{Value, value::RawValue};
use walkdir::WalkDir;

use crate::record::{BillingUnit, EffortEvent, EffortKind, FileScanResult, UsageRecord};
use crate::scan_fields::{contains_bytes, has_component, modified_after};
use crate::time::{format_local_time, parse_rfc3339_timestamp_ms, system_time_to_unix_ms};

#[derive(Debug)]
struct CliUsageAggregate {
    source_path: PathBuf,
    source_line: usize,
    session_id: String,
    turn_index: Option<u32>,
    model: String,
    credits: f64,
    request_count: usize,
    started_at_ms: i64,
    completed_at_ms: i64,
    title: Option<String>,
    project: Option<String>,
}

#[derive(Debug)]
struct CliEventMessage {
    line: usize,
    timestamp_ms: i64,
    model: String,
    is_subagent: bool,
    api_call_id: String,
}

#[derive(Debug)]
struct CliUsageEvent {
    line: usize,
    agent_id: Option<String>,
    api_call_id: String,
}

#[derive(Debug)]
struct CliShutdownMetric {
    line: usize,
    timestamp_ms: i64,
    model: String,
    credits: f64,
    unit: BillingUnit,
}

#[derive(Debug, Default)]
struct CliSessionEvents {
    path: PathBuf,
    scanned_lines: usize,
    title: Option<String>,
    project: Option<String>,
    messages: Vec<CliEventMessage>,
    // A None entry marks an ambiguous match within this session.
    usage_events: HashMap<(i64, String, i64), Option<CliUsageEvent>>,
    used_api_calls: HashSet<String>,
    shutdown_metrics: HashMap<(Option<i64>, BillingUnit), Vec<CliShutdownMetric>>,
}

#[derive(Debug, Default)]
pub(crate) struct CliWorkspaceMetadata {
    pub title: Option<String>,
    pub project: Option<String>,
}

pub(crate) fn collect_cli_probe_files(root: &Path, cutoff: Option<&SystemTime>) -> Result<Vec<PathBuf>> {
    if !root.exists() {
        return Ok(Vec::new());
    }

    let files = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| {
            matches!(
                entry
                    .path()
                    .extension()
                    .and_then(|extension| extension.to_str()),
                Some("jsonl" | "json" | "log")
            )
        })
        .filter(|entry| {
            has_component(entry.path(), "session-state") || has_component(entry.path(), "logs")
        })
        .filter(|entry| modified_after(entry.path(), cutoff))
        .map(|entry| entry.into_path())
        .collect();

    Ok(files)
}

/// Correlates Copilot CLI `events.jsonl` traces with the precise usage ledger.
/// Newer CLI builds persist every model call in `assistant_usage_events`,
/// including calls from an active session, while JSONL remains the report's
/// traceable source path. Returning `None` means this is an older store and the
/// caller should use the generic JSONL/log probe instead.
pub(crate) fn scan_cli_usage_database(root: &Path, cutoff: Option<&SystemTime>) -> Result<Option<FileScanResult>> {
    let path = root.join("session-store.db");
    if !path.is_file() {
        return Ok(None);
    }

    let connection = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("open Copilot CLI database {}", path.display()))?;

    let has_usage_table: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'assistant_usage_events')",
            [],
            |row| row.get(0),
        )
        .with_context(|| format!("inspect Copilot CLI database {}", path.display()))?;
    if !has_usage_table {
        return Ok(None);
    }

    let cutoff_ms = cutoff.and_then(system_time_to_unix_ms);
    scan_cli_usage_connection(&connection, &path, cutoff_ms).map(Some)
}

fn scan_cli_usage_connection(
    connection: &Connection,
    source_path: &Path,
    cutoff_ms: Option<i64>,
) -> Result<FileScanResult> {
    const NANO_AIU_PER_CREDIT: f64 = 1_000_000_000.0;
    let mut result = FileScanResult::default();
    let cli_root = source_path.parent().unwrap_or_else(|| Path::new(""));
    let mut session_events = load_cli_session_events(cli_root);
    result.scanned_lines += session_events
        .values()
        .map(|events| events.scanned_lines)
        .sum::<usize>();
    let query =
        "SELECT u.session_id, u.turn_index, u.model, u.total_nano_aiu,
                u.duration_ms, u.created_at, u.agent_id
         FROM assistant_usage_events u
         WHERE u.total_nano_aiu IS NOT NULL AND u.total_nano_aiu > 0
         ORDER BY u.session_id, u.turn_index, u.model, u.created_at, u.id";
    let mut statement = connection.prepare(query)?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<i64>>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, Option<i64>>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, Option<String>>(6)?,
        ))
    })?;

    let mut aggregates: BTreeMap<(String, Option<u32>, String), CliUsageAggregate> =
        BTreeMap::new();
    let mut sessions_with_database_usage = HashSet::new();
    let mut unmatched_calls = 0;
    for row in rows {
        result.scanned_lines += 1;
        result.json_candidate_lines += 1;
        let (
            session_id,
            zero_based_turn,
            model,
            total_nano_aiu,
            duration_ms,
            created_at,
            agent_id,
        ) = row?;
        let Some(completed_at_ms) = parse_rfc3339_timestamp_ms(&created_at) else {
            result.parse_errors += 1;
            continue;
        };
        let Some((record_source_path, record_source_line, event_timestamp_ms)) =
            match_cli_event_message(
            &mut session_events,
            &session_id,
            completed_at_ms,
            &model,
            agent_id_is_present(&agent_id),
        )
            .or_else(|| {
                match_cli_usage_event(
                    &mut session_events,
                    &session_id,
                    completed_at_ms,
                    &model,
                    total_nano_aiu,
                    agent_id.as_deref().filter(|id| !id.is_empty()),
                )
            })
        else {
            if cutoff_ms.is_none_or(|cutoff| completed_at_ms >= cutoff) {
                unmatched_calls += 1;
            }
            continue;
        };
        sessions_with_database_usage.insert(session_id.clone());
        if cutoff_ms.is_some_and(|cutoff| event_timestamp_ms < cutoff) {
            continue;
        }

        let (title, project) = session_events
            .get(&session_id)
            .map(|events| (events.title.clone(), events.project.clone()))
            .unwrap_or_default();
        let turn_index = zero_based_turn.and_then(|turn| {
            turn.checked_add(1)
                .and_then(|turn| u32::try_from(turn).ok())
        });
        let key = (session_id.clone(), turn_index, model.clone());
        let duration_ms = duration_ms.unwrap_or_default().max(0);
        let started_at_ms = event_timestamp_ms.saturating_sub(duration_ms);

        let entry = aggregates.entry(key).or_insert_with(|| CliUsageAggregate {
            source_path: record_source_path.clone(),
            source_line: record_source_line,
            session_id,
            turn_index,
            model,
            credits: 0.0,
            request_count: 0,
            started_at_ms,
            completed_at_ms: event_timestamp_ms,
            title: title.clone(),
            project: project.clone(),
        });
        if record_source_line < entry.source_line {
            entry.source_path = record_source_path;
            entry.source_line = record_source_line;
        }
        entry.credits += total_nano_aiu as f64 / NANO_AIU_PER_CREDIT;
        entry.request_count += 1;
        entry.started_at_ms = entry.started_at_ms.min(started_at_ms);
        entry.completed_at_ms = entry.completed_at_ms.max(event_timestamp_ms);
        if entry.title.is_none() {
            entry.title = title;
        }
        if entry.project.is_none() {
            entry.project = project;
        }
    }
    if unmatched_calls > 0 {
        eprintln!("warning: {unmatched_calls} Copilot CLI usage rows could not be matched to a JSONL event");
    }

    let mut session_totals: HashMap<String, u32> = HashMap::new();
    for aggregate in aggregates.values() {
        if let Some(turn) = aggregate.turn_index {
            session_totals
                .entry(aggregate.session_id.clone())
                .and_modify(|total| *total = (*total).max(turn))
                .or_insert(turn);
        }
    }

    for aggregate in aggregates.into_values() {
        let duration_ms = aggregate
            .completed_at_ms
            .saturating_sub(aggregate.started_at_ms);
        let details = format!(
            "Copilot CLI {} ({} model call{}) • {} credits",
            aggregate.model,
            aggregate.request_count,
            if aggregate.request_count == 1 {
                ""
            } else {
                "s"
            },
            aggregate.credits
        );
        result.records.push(UsageRecord {
            source: "copilot.cli.events".to_owned(),
            hostname: String::new(),
            timestamp_ms: Some(aggregate.completed_at_ms),
            local_time_hint: format_local_time(aggregate.completed_at_ms),
            chat_title: aggregate.title,
            model: aggregate.model.clone(),
            model_id: Some(aggregate.model),
            credits: aggregate.credits,
            request_count: None,
            premium_requests: None,
            unclassified_usage: None,
            details,
            session_id: Some(aggregate.session_id.clone()),
            request_id: None,
            response_id: None,
            agent_id: None,
            project: aggregate.project,
            duration_ms: (duration_ms > 0).then_some(duration_ms),
            turn_index: aggregate.turn_index,
            session_total: session_totals.get(&aggregate.session_id).copied(),
            file: aggregate.source_path.to_string_lossy().into_owned(),
            line: aggregate.source_line,
        });
    }

    append_legacy_cli_shutdown_records(
        &mut result,
        &session_events,
        &sessions_with_database_usage,
        cutoff_ms,
    );

    Ok(result)
}

fn agent_id_is_present(agent_id: &Option<String>) -> bool {
    agent_id.as_ref().is_some_and(|agent| !agent.is_empty())
}

fn load_cli_session_events(root: &Path) -> HashMap<String, CliSessionEvents> {
    let session_root = root.join("session-state");
    let Ok(entries) = fs::read_dir(session_root) else {
        return HashMap::new();
    };
    let mut sessions = HashMap::new();
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let session_id = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path().join("events.jsonl");
        if let Some(events) = read_cli_session_events(&path) {
            sessions.insert(session_id, events);
        }
    }
    sessions
}

fn read_cli_session_events(path: &Path) -> Option<CliSessionEvents> {
    let file = File::open(path).ok()?;
    let workspace = path
        .parent()
        .map(read_cli_workspace_metadata)
        .unwrap_or_default();
    let mut events = CliSessionEvents {
        path: path.to_path_buf(),
        title: workspace.title,
        project: workspace.project,
        ..CliSessionEvents::default()
    };
    let reader = BufReader::with_capacity(1024 * 1024, file);
    for (index, line) in reader.split(b'\n').enumerate() {
        events.scanned_lines += 1;
        let Ok(line) = line else {
            continue;
        };
        let Ok(value) = serde_json::from_slice::<Value>(&line) else {
            continue;
        };
        let Some(object) = value.as_object() else {
            continue;
        };
        let Some(timestamp_ms) = object
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(parse_rfc3339_timestamp_ms)
        else {
            continue;
        };
        let line_number = index + 1;
        let event_type = object.get("type").and_then(Value::as_str);
        let data = object.get("data").and_then(Value::as_object);
        if event_type == Some("user.message") && events.title.is_none() {
            let is_root_user_message = object
                .get("agentId")
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
                && data
                    .and_then(|data| data.get("source"))
                    .is_none_or(Value::is_null);
            if is_root_user_message {
                events.title = data
                    .and_then(|data| data.get("content"))
                    .and_then(Value::as_str)
                    .and_then(|content| normalized_cli_title(Some(content)));
            }
        } else if event_type == Some("assistant.message") {
            let Some(data) = data else {
                continue;
            };
            let Some(model) = data.get("model").and_then(Value::as_str) else {
                continue;
            };
            let Some(api_call_id) = data.get("apiCallId").and_then(Value::as_str) else {
                continue;
            };
            events.messages.push(CliEventMessage {
                line: line_number,
                timestamp_ms,
                model: model.to_owned(),
                is_subagent: object
                    .get("agentId")
                    .and_then(Value::as_str)
                    .is_some_and(|agent| !agent.is_empty()),
                api_call_id: api_call_id.to_owned(),
            });
        } else if event_type == Some("session.usage_record") {
            // CLI can bill a call under its requested model while the assistant
            // message is attributed to a different model (e.g. a fusion phase).
            let Some(usage) = data.and_then(|data| data.get("usage")) else {
                continue;
            };
            let Some(model) = usage.get("model").and_then(Value::as_str) else {
                continue;
            };
            let Some(total_nano_aiu) = usage.pointer("/copilotUsage/totalNanoAiu")
                .and_then(Value::as_i64).filter(|&credits| credits > 0)
            else {
                continue;
            };
            let Some(api_call_id) = usage.get("apiCallId").and_then(Value::as_str)
                .filter(|id| !id.is_empty())
            else {
                continue;
            };
            let agent_id = object.get("agentId").and_then(Value::as_str)
                .filter(|id| !id.is_empty()).map(ToOwned::to_owned);
            events.usage_events.entry((timestamp_ms, model.to_owned(), total_nano_aiu))
                .and_modify(|entry| *entry = None)
                .or_insert_with(|| Some(CliUsageEvent {
                    line: line_number,
                    agent_id,
                    api_call_id: api_call_id.to_owned(),
                }));
        } else if event_type == Some("session.shutdown") {
            if let Some(data) = data {
                let amounts = cli_shutdown_amounts(data);
                for unit in [BillingUnit::Credits, BillingUnit::RequestCount, BillingUnit::Unclassified] {
                    let amounts: Vec<_> = amounts.iter().filter(|(_, kind, _)| *kind == unit).collect();
                    if amounts.is_empty() { continue }
                    let start = data.get("sessionStartTime").and_then(Value::as_i64);
                    let previous = events.shutdown_metrics.entry((start, unit)).or_default();
                    // A resumed conversation can replay a cumulative shutdown
                    // or report a smaller partial total. Neither is a delta.
                    if amounts.iter().map(|(_, _, n)| n).sum::<f64>() > previous.iter().map(|m| m.credits).sum::<f64>() {
                        *previous = amounts.into_iter().map(|(model, unit, credits)| CliShutdownMetric {
                            line: line_number, timestamp_ms, model: model.clone(), unit: *unit, credits: *credits,
                        }).collect();
                    }
                }
            }
        }
    }
    Some(events)
}

pub(crate) fn read_cli_workspace_metadata(session_dir: &Path) -> CliWorkspaceMetadata {
    let Ok(workspace) = fs::read_to_string(session_dir.join("workspace.yaml")) else {
        return CliWorkspaceMetadata::default();
    };
    let mut name = None;
    let mut cwd = None;
    for line in workspace.lines().filter(|line| !line.starts_with(char::is_whitespace)) {
        if let Some(value) = line.strip_prefix("name:").and_then(cli_yaml_scalar) {
            name = Some(value);
        } else if let Some(value) = line.strip_prefix("cwd:").and_then(cli_yaml_scalar) {
            cwd = Some(value);
        }
    }
    CliWorkspaceMetadata {
        title: name.as_deref().and_then(|name| normalized_cli_title(Some(name))),
        project: cwd.as_deref().and_then(project_name_from_cli_path),
    }
}

fn cli_yaml_scalar(raw: &str) -> Option<String> {
    let value = raw.trim();
    if value.is_empty() {
        return None;
    }
    if value.starts_with('"') && value.ends_with('"') {
        return serde_json::from_str::<String>(value).ok();
    }
    let value = value
        .strip_prefix('\'')
        .and_then(|value| value.strip_suffix('\''))
        .unwrap_or(value)
        .replace("''", "'");
    (!value.is_empty()).then_some(value)
}

fn match_cli_event_message(
    sessions: &mut HashMap<String, CliSessionEvents>,
    session_id: &str,
    timestamp_ms: i64,
    model: &str,
    is_subagent: bool,
) -> Option<(PathBuf, usize, i64)> {
    const MAX_TIMESTAMP_DELTA_MS: i64 = 1_000;
    let events = sessions.get_mut(session_id)?;
    let find_nearest = |require_agent_match: bool, events: &CliSessionEvents| {
        events
            .messages
            .iter()
            .enumerate()
            .filter(|(_, message)| {
                message.model == model
                    && !events.used_api_calls.contains(&message.api_call_id)
                    && (!require_agent_match || message.is_subagent == is_subagent)
            })
            .min_by_key(|(_, message)| message.timestamp_ms.abs_diff(timestamp_ms))
            .map(|(index, message)| (index, message.timestamp_ms.abs_diff(timestamp_ms)))
    };
    let (index, delta) = find_nearest(true, events).or_else(|| find_nearest(false, events))?;
    if delta > u64::try_from(MAX_TIMESTAMP_DELTA_MS).ok()? {
        return None;
    }
    let message = &events.messages[index];
    events.used_api_calls.insert(message.api_call_id.clone());
    Some((events.path.clone(), message.line, message.timestamp_ms))
}

fn match_cli_usage_event(
    sessions: &mut HashMap<String, CliSessionEvents>,
    session_id: &str,
    timestamp_ms: i64,
    model: &str,
    total_nano_aiu: i64,
    agent_id: Option<&str>,
) -> Option<(PathBuf, usize, i64)> {
    let events = sessions.get_mut(session_id)?;
    let event = events.usage_events.get(&(timestamp_ms, model.to_owned(), total_nano_aiu))?
        .as_ref()?;
    if event.agent_id.as_deref() != agent_id || !events.used_api_calls.insert(event.api_call_id.clone()) {
        return None;
    }
    Some((events.path.clone(), event.line, timestamp_ms))
}

fn append_legacy_cli_shutdown_records(
    result: &mut FileScanResult,
    session_events: &HashMap<String, CliSessionEvents>,
    sessions_with_database_usage: &HashSet<String>,
    cutoff_ms: Option<i64>,
) {
    for (session_id, events) in session_events {
        for metric in events.shutdown_metrics.values().flatten() {
            if metric.unit != BillingUnit::RequestCount && sessions_with_database_usage.contains(session_id) {
                continue;
            }
            if cutoff_ms.is_some_and(|cutoff| metric.timestamp_ms < cutoff) {
                continue;
            }
            result.json_candidate_lines += 1;
            let mut record = UsageRecord {
                source: "copilot.cli.events".to_owned(),
                hostname: String::new(),
                timestamp_ms: Some(metric.timestamp_ms),
                local_time_hint: format_local_time(metric.timestamp_ms),
                chat_title: events.title.clone(),
                model: metric.model.clone(),
                model_id: Some(metric.model.clone()),
                credits: metric.credits,
                request_count: None,
                premium_requests: None,
                unclassified_usage: None,
                details: shutdown_details(&metric.model, metric.unit, metric.credits),
                session_id: Some(session_id.clone()),
                request_id: None,
                response_id: None,
                agent_id: None,
                project: events.project.clone(),
                duration_ms: None,
                turn_index: None,
                session_total: None,
                file: events.path.to_string_lossy().into_owned(),
                line: metric.line,
            };
            record.set_amount(metric.unit, metric.credits);
            result.records.push(record);
        }
    }
}

/// Fallback without SQLite still uses the same shutdown replay semantics.
pub(crate) fn scan_legacy_session(path: &Path) -> Option<FileScanResult> {
    let session = cli_session_id_from_event_path(path)?;
    let events = read_cli_session_events(path)?;
    let mut result = FileScanResult { scanned_lines: events.scanned_lines, ..FileScanResult::default() };
    append_legacy_cli_shutdown_records(&mut result, &HashMap::from([(session, events)]), &HashSet::new(), None);
    Some(result)
}

pub(crate) fn cli_session_id_from_event_path(path: &Path) -> Option<String> {
    if !path
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("events.jsonl"))
    {
        return None;
    }
    let session_dir = path.parent()?;
    let session_root = session_dir.parent()?;
    if !session_root
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("session-state"))
    {
        return None;
    }
    session_dir
        .file_name()
        .map(|session_id| session_id.to_string_lossy().into_owned())
}

fn project_name_from_cli_path(path: &str) -> Option<String> {
    path.trim()
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .find(|part| !part.trim().is_empty())
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned)
}

fn normalized_cli_title(value: Option<&str>) -> Option<String> {
    let value = value?.lines().find(|line| !line.trim().is_empty())?.trim();
    if value.is_empty() {
        return None;
    }
    const MAX_CHARS: usize = 120;
    let mut title: String = value.chars().take(MAX_CHARS).collect();
    if value.chars().count() > MAX_CHARS {
        title.push('…');
    }
    Some(title)
}

pub(crate) fn extract_cli_user_title_from_line(line: &[u8]) -> Option<String> {
    if !contains_bytes(line, b"user.message") || !contains_bytes(line, b"content") {
        return None;
    }
    let value = serde_json::from_slice::<Value>(line).ok()?;
    let object = value.as_object()?;
    if object.get("type").and_then(Value::as_str) != Some("user.message")
        || object
            .get("agentId")
            .and_then(Value::as_str)
            .is_some_and(|agent| !agent.is_empty())
    {
        return None;
    }
    let data = object.get("data")?.as_object()?;
    if data.get("source").is_some_and(|source| !source.is_null()) {
        return None;
    }
    normalized_cli_title(data.get("content").and_then(Value::as_str))
}

pub(crate) fn cli_shutdown_amounts(data: &serde_json::Map<String, Value>) -> Vec<(String, BillingUnit, f64)> {
    let positive = |v: &Value| v.as_f64().filter(|n| n.is_finite() && *n > 0.0);
    // Some new builds retain a shadow premium counter. When nano-AIU is
    // present (including explicit zero), do not classify the log as history.
    if let Some(nano) = data.get("totalNanoAiu").and_then(Value::as_f64).filter(|n| n.is_finite() && *n >= 0.0) {
        return (nano > 0.0).then(|| ("Unattributed".into(), BillingUnit::Credits, nano / 1_000_000_000.0)).into_iter().collect();
    }
    let legacy = data.get("totalPremiumRequests").and_then(Value::as_f64).is_some_and(|n| n.is_finite() && n >= 0.0);
    let models = data.get("modelMetrics").and_then(Value::as_object).into_iter().flatten();
    let mut rows = Vec::new();
    for (model, data) in models {
        if legacy && let Some(count) = data.pointer("/requests/count").and_then(Value::as_u64) {
            if count > 0 { rows.push((model.clone(), BillingUnit::RequestCount, count as f64)); }
        } else if let Some(value) = data.pointer("/requests/cost").and_then(positive) {
            rows.push((model.clone(), BillingUnit::Unclassified, value));
        }
    }
    rows
}

pub(crate) fn shutdown_details(model: &str, unit: BillingUnit, n: f64) -> String {
    let label = match unit {
        BillingUnit::Credits => "AI credits (totalNanoAiu)",
        BillingUnit::RequestCount => "model requests (raw requests.count)",
        BillingUnit::Unclassified => "usage (unit not recorded)",
    };
    format!("Copilot CLI {model} (session shutdown) • {n} {label}")
}

pub(crate) fn scan_effort(root: &Path) -> Vec<EffortEvent> {
    let Ok(entries) = fs::read_dir(root.join("session-state")) else { return Vec::new() };
    let mut events = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path().join("events.jsonl");
        let Ok(file) = File::open(&path) else { continue };
        let session = entry.file_name().to_string_lossy().into_owned();
        let project = read_cli_workspace_metadata(&entry.path()).project;
        for (line, bytes) in BufReader::new(file).split(b'\n').enumerate() {
            let Ok(bytes) = bytes else { break };
            if !contains_bytes(&bytes, b"user.message") { continue }
            // Do not decode the message body. Some providers write JS strings
            // that cannot be decoded as Rust UTF-8 text.
            #[derive(serde::Deserialize)]
            struct UserEvent<'a> {
                #[serde(rename = "type")]
                event_type: String,
                id: Option<String>,
                timestamp: Option<String>,
                #[serde(rename = "agentId")]
                agent_id: Option<String>,
                #[serde(borrow)]
                data: Option<&'a RawValue>,
            }
            #[derive(serde::Deserialize)]
            struct Message<'a> {
                #[serde(borrow)]
                content: Option<&'a RawValue>,
                source: Option<Value>,
            }
            let Ok(event) = serde_json::from_slice::<UserEvent>(&bytes) else { continue };
            if event.event_type != "user.message" { continue }
            let data = event.data.and_then(|raw| serde_json::from_str::<Message>(raw.get()).ok());
            let kind = if event.agent_id.as_deref().is_some_and(|id| !id.is_empty())
                || data.as_ref().is_some_and(|data| data.source.is_some()) {
                EffortKind::Automatic
            } else if data.and_then(|data| data.content).is_some() {
                EffortKind::Human
            } else {
                EffortKind::Unverified
            };
            events.push(EffortEvent {
                source: "Copilot CLI", session: session.clone(), id: event.id,
                project: project.clone(), timestamp_ms: event.timestamp.as_deref().and_then(parse_rfc3339_timestamp_ms),
                file: path.clone(), line: line + 1, kind,
            });
        }
    }
    events
}

#[cfg(test)]
#[path = "cli_scan_tests.rs"]
mod tests;