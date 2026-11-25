//! Replay the billing projection of VS Code's JSONL state journal.
//!
//! `copilotCredits` is a cumulative request total (including subagents), not
//! an increment. It can be persisted without `result.details`, or after it.
//! Never add toolSpecificData.credits to it, nor charge journal rewrites.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

use serde_json::{Value, json};
use serde_json::value::RawValue;

use crate::billing::{parse_credit_details, parse_premium_details};
use crate::record::{ContextFields, EffortEvent, EffortKind, FileScanResult, UsageRecord};
use crate::scan_fields::{display_model_id, merge_context_from_object};
use crate::time::format_local_time;

#[derive(Default)]
struct Session {
    state: Value,
    // A chat rewind removes UI requests, not their already incurred cost.
    // Keep the latest billing state of every stable user request ever seen.
    history: Vec<Value>,
    identities: HashMap<String, usize>,
    active: HashSet<usize>,
    peaks: HashMap<usize, (f64, u64)>,
    pending: HashMap<String, Value>,
}

// Retain only billing and attribution fields, never prompts, tool results or
// response text. Large parallel sessions otherwise consume excessive memory.
fn relevant(path: &[Value]) -> bool {
    let key = |i: usize| path.get(i).and_then(Value::as_str);
    if key(0) == Some("pendingRequests") {
        return match path.len() {
            1 => true,
            _ if path[1].as_u64().is_none() => false,
            2 => true,
            3 => matches!(key(2), Some("id" | "request" | "sendOptions")),
            4 => match key(2) {
                Some("request") => matches!(key(3), Some("requestId" | "timestamp" | "message" | "isSystemInitiated" | "hiddenFromTranscript" | "systemInitiatedLabel" | "terminalExecutionId" | "requestSource")),
                Some("sendOptions") => matches!(key(3), Some("isSystemInitiated" | "hideFromTranscript")),
                _ => false,
            },
            _ => false,
        };
    }
    if key(0) == Some("sessionCostInfo") {
        return path.len() == 1 || path.len() == 2 && key(1) == Some("credits");
    }
    match path.len() {
        0 => true,
        1 => matches!(key(0), Some("requests" | "sessionId" | "creationDate" | "customTitle" | "title")),
        _ if key(0) != Some("requests") || path[1].as_u64().is_none() => false,
        2 => true,
        3 => matches!(key(2), Some(
            "requestId" | "responseId" | "modelId" | "agentId" | "timestamp"
            | "sessionId" | "responseTimestamp" | "modelState" | "copilotCredits" | "sessionCopilotCredits" | "elapsedMs" | "result"
            | "message" | "isSystemInitiated" | "hiddenFromTranscript" | "systemInitiatedLabel" | "terminalExecutionId" | "requestSource"
        )),
        4 => match key(2) {
            Some("modelState") => key(3) == Some("completedAt"),
            Some("result") => matches!(key(3), Some(
                "details" | "metadata" | "timings" | "completedAt" | "timestamp"
                | "sessionId" | "requestId" | "responseId" | "modelId" | "agentId"
            )),
            _ => false,
        },
        5 => key(2) == Some("result") && match key(3) {
            Some("metadata") => matches!(key(4), Some(
                "sessionId" | "requestId" | "responseId" | "modelId" | "agentId"
            )),
            Some("timings") => key(4) == Some("totalElapsed"),
            _ => false,
        },
        _ => false,
    }
}

// Decode only the selected projection. Tool output can be enormous and may
// contain JS strings with unpaired UTF-16 surrogates; unrelated text must not
// prevent reading valid billing updates from the same journal.
fn project_raw(raw: &RawValue, path: &mut Vec<Value>) -> serde_json::Result<Value> {
    if path.last().and_then(Value::as_str) == Some("message") {
        let text = raw.get().trim_start();
        let fields = text.starts_with('{').then(|| serde_json::from_str::<HashMap<String, &RawValue>>(text)).transpose()?;
        let message = fields.as_ref().and_then(|f| f.get("text")).map(|v| v.get().trim_start()).unwrap_or(text);
        // Keep only a marker. A visible, terminal-linked completion message
        // can be system-generated even when isSystemInitiated is false.
        return Ok(if message.starts_with("\"[Terminal") { json!("terminal") }
            else if message.starts_with('"') { json!("message") } else { Value::Null });
    }
    match raw.get().trim_start().as_bytes().first() {
        Some(b'{') => {
            let fields: HashMap<String, &RawValue> = serde_json::from_str(raw.get())?;
            let mut out = serde_json::Map::new();
            for (key, value) in fields {
                path.push(json!(key));
                if relevant(path) {
                    out.insert(key, project_raw(value, path)?);
                }
                path.pop();
            }
            Ok(Value::Object(out))
        }
        Some(b'[') => {
            let items: Vec<&RawValue> = serde_json::from_str(raw.get())?;
            let mut out = Vec::with_capacity(items.len());
            for (index, item) in items.into_iter().enumerate() {
                path.push(json!(index));
                out.push(project_raw(item, path)?);
                path.pop();
            }
            Ok(Value::Array(out))
        }
        _ => serde_json::from_str(raw.get()),
    }
}

fn parse_entry(line: &[u8]) -> serde_json::Result<Option<Value>> {
    let fields: HashMap<String, &RawValue> = serde_json::from_slice(line)?;
    let mut entry = serde_json::Map::new();
    if !fields.contains_key("kind") && fields.contains_key("requests") {
        let raw: &RawValue = serde_json::from_slice(line)?;
        return project_raw(raw, &mut Vec::new()).map(Some);
    }
    for key in ["kind", "k", "i"] {
        if let Some(raw) = fields.get(key) {
            entry.insert(key.into(), serde_json::from_str(raw.get())?);
        }
    }
    let mut path = entry.get("k").and_then(Value::as_array).cloned().unwrap_or_default();
    if !relevant(&path) {
        return Ok(None);
    }
    if let Some(raw) = fields.get("v") {
        entry.insert("v".into(), project_raw(raw, &mut path)?);
    }
    Ok(Some(Value::Object(entry)))
}

fn project(value: &Value, path: &mut Vec<Value>, line: usize) -> Value {
    match value {
        Value::Object(object) => {
            let mut out = serde_json::Map::new();
            for (key, child) in object {
                path.push(json!(key));
                if relevant(path) {
                    out.insert(key.clone(), project(child, path, line));
                }
                path.pop();
            }
            if path.len() == 2 && out.contains_key("copilotCredits") {
                out.insert("_creditsLine".into(), json!(line));
            }
            if (path.len() == 2 && path[0] == "requests" || path.len() == 3 && path[0] == "pendingRequests" && path[2] == "request") && out.contains_key("timestamp") {
                out.insert("_requestLine".into(), json!(line));
            }
            if path.len() == 2 && out.contains_key("sessionCopilotCredits") {
                out.insert("_sessionCreditsLine".into(), json!(line));
            }
            if path.len() == 1 && path[0] == "sessionCostInfo" && out.contains_key("credits") {
                out.insert("_sessionCreditsLine".into(), json!(line));
            }
            if path.len() == 3 && path[2] == "result" && out.contains_key("details") {
                out.insert("_detailsLine".into(), json!(line));
            }
            Value::Object(out)
        }
        Value::Array(array) => Value::Array(array.iter().enumerate().map(|(i, child)| {
            path.push(json!(i));
            let projected = project(child, path, line);
            path.pop();
            projected
        }).collect()),
        _ => value.clone(),
    }
}

// Journals normally start with a snapshot. Creating missing containers also
// supports partial/older journals whose first surviving entry is a delta.
fn target<'a>(state: &'a mut Value, path: &[Value]) -> Option<&'a mut Value> {
    let mut node = state;
    for key in path {
        if let Some(key) = key.as_str() {
            if node.is_null() {
                *node = json!({});
            }
            node = node.as_object_mut()?.entry(key).or_insert(Value::Null);
        } else {
            let index = usize::try_from(key.as_u64()?).ok()?;
            // Do not allocate unbounded arrays for a corrupt journal index.
            if index > 1_000_000 {
                return None;
            }
            if node.is_null() {
                *node = json!([]);
            }
            let array = node.as_array_mut()?;
            if index >= array.len() {
                array.resize(index + 1, Value::Null);
            }
            node = &mut array[index];
        }
    }
    Some(node)
}

impl Session {
    fn apply(&mut self, entry: &Value, line: usize) -> bool {
        let kind = entry.get("kind").and_then(Value::as_u64);
        if kind.is_none() && entry.get("requests").is_some_and(Value::is_array) {
            self.state = project(entry, &mut Vec::new(), line);
            self.remember_requests();
            self.remember_pending();
            return true;
        }
        if kind == Some(0) {
            let Some(value) = entry.get("v").filter(|v| v.is_object()) else { return false };
            self.state = project(value, &mut Vec::new(), line);
            self.remember_requests();
            self.remember_pending();
            return true;
        }
        let Some(path) = entry.get("k").and_then(Value::as_array) else { return false };
        if !relevant(path) {
            return true;
        }
        let Some(node) = target(&mut self.state, path) else { return false };
        match kind {
            Some(1) => {
                // Undefined object properties are omitted by JSON.stringify.
                let value = entry.get("v").unwrap_or(&Value::Null);
                *node = project(value, &mut path.clone(), line);
            }
            Some(2) if path.len() == 1 && (path[0] == "requests" || path[0] == "pendingRequests") => {
                if node.is_null() {
                    *node = json!([]);
                }
                let Some(array) = node.as_array_mut() else { return false };
                let index = match entry.get("i") {
                    Some(value) => match value.as_u64().and_then(|i| usize::try_from(i).ok()) {
                        Some(i) if i <= array.len() => i,
                        _ => return false,
                    },
                    None => array.len(),
                };
                let values = match entry.get("v") {
                    Some(Value::Array(values)) => values.as_slice(),
                    None => &[],
                    _ => return false,
                };
                // kind 2 replaces the array tail from i; without i it appends.
                array.truncate(index);
                for (offset, value) in values.iter().enumerate() {
                    array.push(project(value, &mut vec![path[0].clone(), json!(index + offset)], line));
                }
            }
            _ => return false,
        }
        // Scalar deltas must update provenance too, not just full snapshots.
        if path.len() == 3 && path[2] == "copilotCredits"
            && let Some(request) = target(&mut self.state, &path[..2])
        {
            request["_creditsLine"] = json!(line);
        }
        if path.len() == 4 && path[2] == "result" && path[3] == "details"
            && let Some(result) = target(&mut self.state, &path[..3])
        {
            result["_detailsLine"] = json!(line);
        }
        if path.len() == 3 && (path[2] == "timestamp" || path[2] == "sessionCopilotCredits")
            && let Some(request) = target(&mut self.state, &path[..2]) {
            request[if path[2] == "timestamp" { "_requestLine" } else { "_sessionCreditsLine" }] = json!(line);
        }
        if path.len() == 2 && path[0] == "sessionCostInfo" && path[1] == "credits" {
            self.state["sessionCostInfo"]["_sessionCreditsLine"] = json!(line);
        }
        if path.first().is_some_and(|v| v == "pendingRequests") {
            self.remember_pending();
        } else {
            self.remember_requests();
        }
        true
    }

    fn remember_pending(&mut self) {
        let Some(entries) = self.state.get("pendingRequests").and_then(Value::as_array) else { return };
        for entry in entries {
            let Some(id) = entry.pointer("/request/requestId").or_else(|| entry.get("id")).and_then(Value::as_str) else { continue };
            self.pending.entry(id.to_owned()).or_insert_with(|| entry.clone());
        }
    }

    fn remember_requests(&mut self) {
        self.active.clear();
        let Some(requests) = self.state.get_mut("requests").and_then(Value::as_array_mut) else {
            return;
        };
        for (slot, request) in requests.iter_mut().enumerate() {
            if !request.is_object() {
                continue;
            }
            let identity = request.get("requestId").and_then(Value::as_str)
                .or_else(|| request.pointer("/result/metadata/responseId").and_then(Value::as_str))
                .or_else(|| request.get("responseId").and_then(Value::as_str))
                .or_else(|| request.pointer("/result/responseId").and_then(Value::as_str))
                .map(|id| format!("id:{id}"));
            let previous = request.get("_ledgerKey").and_then(Value::as_str).map(ToOwned::to_owned);
            let key = identity.or_else(|| previous.clone()).unwrap_or_else(|| format!("slot:{slot}"));
            // A partial journal can reveal the stable ID after its first delta.
            if !self.identities.contains_key(&key)
                && let Some(previous) = previous.filter(|p| p.starts_with("slot:"))
                && let Some(index) = self.identities.remove(&previous)
            {
                self.identities.insert(key.clone(), index);
            }
            let index = *self.identities.entry(key.clone()).or_insert_with(|| {
                // Keep gaps in partial journals; they are not free exchanges.
                if slot > self.history.len() {
                    self.history.resize(slot, Value::Null);
                }
                self.history.push(Value::Null);
                self.history.len() - 1
            });
            request["_ledgerKey"] = json!(key);
            if let Some(credits) = valid_credits(request.get("copilotCredits")) {
                let previous = self.peaks.entry(index).or_default();
                if credits >= previous.0 {
                    *previous = (credits, request.get("_creditsLine").and_then(Value::as_u64).unwrap_or(1));
                }
            }
            self.history[index] = request.clone();
            self.active.insert(index);
        }
    }

    fn effort_events(&self, path: &Path, project: Option<&str>) -> Vec<EffortEvent> {
        let session = self.state.get("sessionId").and_then(Value::as_str)
            .or_else(|| path.file_stem().and_then(|s| s.to_str())).unwrap_or_default();
        let event = |r: &Value, pending: Option<&Value>| {
            let automatic = ["isSystemInitiated", "hiddenFromTranscript"].iter().any(|k| r.get(k).and_then(Value::as_bool) == Some(true))
                || ["systemInitiatedLabel", "requestSource"].iter().any(|k| r.get(k).and_then(Value::as_str).is_some_and(|s| !s.is_empty()))
                || r.get("message").and_then(Value::as_str) == Some("terminal") && r.get("terminalExecutionId").and_then(Value::as_str).is_some()
                || pending.is_some_and(|p| ["/sendOptions/isSystemInitiated", "/sendOptions/hideFromTranscript"].iter()
                    .any(|k| p.pointer(k).and_then(Value::as_bool) == Some(true)));
            EffortEvent {
                source: "VS Code", session: session.to_owned(),
                id: r.get("requestId").and_then(Value::as_str).or_else(|| pending.and_then(|p| p.get("id").and_then(Value::as_str))).map(ToOwned::to_owned),
                project: project.map(ToOwned::to_owned),
                timestamp_ms: r.get("timestamp").and_then(Value::as_i64).filter(|&t| t > 0),
                file: path.to_path_buf(),
                line: r.get("_requestLine").and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok()).unwrap_or(1),
                kind: if automatic { EffortKind::Automatic } else if pending.is_some() { EffortKind::Queued }
                    else if r.get("message").and_then(Value::as_str).is_some() { EffortKind::Human } else { EffortKind::Unverified },
            }
        };
        let mut used = HashSet::new();
        let mut events = Vec::new();
        for request in self.history.iter().filter(|r| r.is_object()) {
            let mut submission = event(request, None);
            if let Some(id) = request.get("requestId").and_then(Value::as_str)
                && let Some(pending) = self.pending.get(id)
                && let Some(r) = pending.get("request") {
                used.insert(id);
                let queued = event(r, Some(pending));
                if queued.kind == EffortKind::Automatic {
                    submission.kind = EffortKind::Automatic;
                } else if queued.timestamp_ms.is_some() {
                    submission.timestamp_ms = queued.timestamp_ms;
                    submission.line = queued.line;
                }
            }
            events.push(submission);
        }
        events.extend(self.pending.iter().filter(|(id, _)| !used.contains(id.as_str()))
            .filter_map(|(_, p)| Some(event(p.get("request")?, Some(p)))));
        events
    }

    fn records(&self, path: &Path, project: Option<String>) -> Vec<UsageRecord> {
        let requests = &self.history;
        let session_id = self.state.get("sessionId").and_then(Value::as_str);
        let title = self.state.get("customTitle").and_then(Value::as_str)
            .or_else(|| self.state.get("title").and_then(Value::as_str))
            .map(str::trim).filter(|s| !s.is_empty());
        let mut records: Vec<_> = requests.iter().enumerate().filter_map(|(index, request)| {
            let object = request.as_object()?;
            let result = request.get("result");
            let details = result.and_then(|v| v.get("details")).and_then(Value::as_str);
            let parsed = details.and_then(parse_credit_details);
            let premium = details.and_then(parse_premium_details);
            let latest = valid_credits(request.get("copilotCredits"));
            // A continued turn can replace the progress total with a smaller
            // partial report. Without a final footer preserve observed credits,
            // but do not sum progress updates or override a valid final result.
            let retained = parsed.is_none().then(|| self.peaks.get(&index).copied()).flatten()
                .filter(|(peak, _)| *peak > latest.unwrap_or_default());
            let cumulative = retained.map(|(c, _)| c).or(latest);
            // Some versions/continued requests persist a partial context-widget
            // total even though the final result contains a larger turn total.
            // These are overlapping totals: choose the more complete one, never
            // sum them. Prefer full precision within details' 0.1-credit rounding.
            let use_cumulative = cumulative.is_some_and(|c| {
                parsed.as_ref().is_none_or(|(_, d)| c + 0.050000001 >= *d)
            });
            let credits = if use_cumulative {
                cumulative?
            } else {
                parsed.as_ref().map(|(_, c)| *c).or_else(|| premium.as_ref().map(|_| 0.0))?
            };
            let request_count = (!use_cumulative && parsed.is_none() && premium.is_some()).then_some(1);
            let mut context = ContextFields::default();
            // Prefer the extension's final response ID for legacy compatibility.
            if let Some(metadata) = result.and_then(|v| v.get("metadata")).and_then(Value::as_object) {
                merge_context_from_object(&mut context, metadata);
            }
            merge_context_from_object(&mut context, object);
            if let Some(result) = result.and_then(Value::as_object) {
                merge_context_from_object(&mut context, result);
            }
            context.timestamp_ms = request.pointer("/modelState/completedAt").and_then(Value::as_i64)
                .or_else(|| request.get("responseTimestamp").and_then(Value::as_i64))
                .or(context.timestamp_ms)
                .or_else(|| self.state.get("creationDate").and_then(Value::as_i64));
            let model_id = context.model_id.as_deref().map(display_model_id);
            let model = parsed.as_ref().or(premium.as_ref()).map(|(m, _)| m.clone())
                .or_else(|| model_id.clone()).unwrap_or_else(|| "Unknown".into());
            let (mut details, line) = if use_cumulative {
                if let Some((_, line)) = retained {
                    (format!("{model} • {credits} credits (largest recorded cumulative total; no final details)"), Some(line))
                } else {
                    (format!("{model} • {credits} credits (cumulative, includes subagents)"),
                     request.get("_creditsLine").and_then(Value::as_u64))
                }
            } else {
                (details.unwrap_or_default().to_owned(),
                 result.and_then(|v| v.get("_detailsLine")).and_then(Value::as_u64))
            };
            if !self.active.contains(&index) {
                details.push_str(" (historical turn)");
            }
            Some(UsageRecord {
                source: "vscode.chatSessions".into(),
                hostname: String::new(),
                timestamp_ms: context.timestamp_ms,
                local_time_hint: context.timestamp_ms.and_then(format_local_time),
                chat_title: title.map(ToOwned::to_owned),
                model,
                model_id,
                credits,
                request_count,
                premium_requests: None,
                unclassified_usage: None,
                details,
                session_id: session_id.map(ToOwned::to_owned).or(context.session_id)
                    .or_else(|| path.file_stem().and_then(|s| s.to_str()).map(ToOwned::to_owned)),
                request_id: context.request_id,
                response_id: context.response_id,
                agent_id: context.agent_id,
                project: project.clone(),
                duration_ms: context.total_elapsed_ms.or_else(|| request.get("elapsedMs").and_then(Value::as_i64)),
                // Count user exchanges, not model calls or tool invocations.
                // Missing/rewound turns must not renumber subsequent exchanges.
                turn_index: u32::try_from(index + 1).ok(),
                session_total: u32::try_from(requests.len()).ok(),
                file: path.to_string_lossy().into_owned(),
                line: line.and_then(|l| usize::try_from(l).ok()).unwrap_or(1),
            })
        }).collect();
        let aggregate = self.state.get("sessionCostInfo").into_iter()
            .filter_map(|v| Some((valid_credits(v.get("credits"))?, v.get("_sessionCreditsLine").and_then(Value::as_u64), None)))
            .chain(self.active.iter().filter_map(|&index| {
                let request = self.history.get(index)?;
                Some((valid_credits(request.get("sessionCopilotCredits"))?, request.get("_sessionCreditsLine").and_then(Value::as_u64), request.get("timestamp").and_then(Value::as_i64)))
            })).max_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        if let Some((total, line, timestamp)) = aggregate {
            let observed = records.iter().map(|r| r.credits).sum::<f64>();
            let remainder = total - observed;
            if remainder > 0.00000001 {
                let timestamp_ms = timestamp.or_else(|| records.iter().filter_map(|r| r.timestamp_ms).max())
                    .or_else(|| self.state.get("creationDate").and_then(Value::as_i64));
                records.push(UsageRecord {
                    source: "vscode.sessionCost".into(), hostname: String::new(), timestamp_ms,
                    local_time_hint: timestamp_ms.and_then(format_local_time), chat_title: title.map(ToOwned::to_owned),
                    model: "Unattributed".into(), model_id: None, credits: remainder,
                    request_count: None,
                    premium_requests: None,
                    unclassified_usage: None,
                    details: format!("VS Code session aggregate: {total} credits; {observed} already attributed to turns"),
                    session_id: session_id.map(ToOwned::to_owned).or_else(|| path.file_stem().and_then(|s| s.to_str()).map(ToOwned::to_owned)),
                    request_id: None, response_id: None, agent_id: None, project, duration_ms: None,
                    turn_index: None, session_total: None,
                    file: path.to_string_lossy().into_owned(), line: line.and_then(|l| usize::try_from(l).ok()).unwrap_or(1),
                });
            }
        }
        records
    }
}

fn valid_credits(value: Option<&Value>) -> Option<f64> {
    value.and_then(Value::as_f64).filter(|v| v.is_finite() && *v >= 0.0)
}

pub(super) fn scan(path: &Path, project: Option<String>, collect_effort: bool) -> FileScanResult {
    let mut result = FileScanResult::default();
    let Ok(file) = File::open(path) else {
        result.parse_errors += 1;
        return result;
    };
    let mut session = Session::default();
    let mut reader = BufReader::with_capacity(1024 * 1024, file);
    let mut apply = |line: std::io::Result<Vec<u8>>| {
        result.scanned_lines += 1;
        let Ok(line) = line else {
            result.parse_errors += 1;
            return false;
        };
        let line = line.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&line);
        if line.iter().all(u8::is_ascii_whitespace) {
            return true;
        }
        match parse_entry(line) {
            Ok(None) => {}
            Ok(Some(value)) => {
                result.json_candidate_lines += 1;
                if !session.apply(&value, result.scanned_lines) {
                    result.parse_errors += 1;
                    eprintln!("warning: unsupported VS Code journal update at {}:{}", path.display(), result.scanned_lines);
                }
            }
            Err(error) => {
                result.parse_errors += 1;
                eprintln!("warning: invalid VS Code journal at {}:{}: {error}", path.display(), result.scanned_lines);
            }
        }
        true
    };
    if path.extension().is_some_and(|ext| ext == "json") {
        let mut bytes = Vec::new();
        apply(reader.read_to_end(&mut bytes).map(|_| bytes));
    } else {
        for line in reader.split(b'\n') {
            if !apply(line) { break }
        }
    }
    if collect_effort { result.effort = session.effort_events(path, project.as_deref()); }
    result.records = session.records(path, project);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::assign_turn_indices;
    use std::fs;

    fn scan_entries(entries: &[Value]) -> FileScanResult {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("gh-usage-vscode-{}-{unique}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.jsonl");
        let text = entries.iter().map(Value::to_string).collect::<Vec<_>>().join("\n");
        fs::write(&path, text).unwrap();
        let result = scan(&path, Some("project".into()), true);
        fs::remove_dir_all(dir).unwrap();
        result
    }

    #[test]
    fn counts_long_running_request_without_final_details_once() {
        let result = scan_entries(&[
            json!({"kind":0,"v":{"sessionId":"session","creationDate":1000,"requests":[]}}),
            json!({"kind":2,"k":["requests"],"v":[{
                "requestId":"request","responseId":"response","timestamp":2000,
                "modelId":"copilot/gpt-6-astra","copilotCredits":10.0
            }]}),
            json!({"kind":1,"k":["requests",0,"copilotCredits"],"v":100.0}),
            json!({"kind":2,"k":["requests",0,"response"],"v":[{
                "toolCallId":"child","toolSpecificData":{"kind":"subagent","credits":80.0}
            }]}),
            json!({"kind":1,"k":["requests",0,"copilotCredits"],"v":39950.53665}),
            json!({"kind":1,"k":["requests",0,"copilotCredits"],"v":39950.53665}),
            json!({"kind":1,"k":["requests",0,"modelState"],"v":{"completedAt":9000}}),
            json!({"kind":1,"k":["customTitle"],"v":"Long task"}),
        ]);
        assert_eq!(result.parse_errors, 0);
        assert_eq!(result.records.len(), 1);
        let r = &result.records[0];
        assert_eq!(r.credits, 39950.53665);
        assert_eq!(r.line, 6);
        assert_eq!(r.session_id.as_deref(), Some("session"));
        assert_eq!(r.request_id.as_deref(), Some("request"));
        assert_eq!(r.chat_title.as_deref(), Some("Long task"));
        assert_eq!(r.model_id.as_deref(), Some("gpt-6-astra"));
        assert_eq!(r.timestamp_ms, Some(9000));
        assert_eq!(r.project.as_deref(), Some("project"));
    }

    #[test]
    fn preserves_reported_credits_when_unfinished_turn_total_drops() {
        let result = scan_entries(&[
            json!({"kind":0,"v":{"requests":[{"requestId":"unfinished","modelId":"copilot/gpt-6.1-sol","copilotCredits":3.56761},
                {"requestId":"finished","modelId":"copilot/gpt-6.2-sol","copilotCredits":50.0}]}}),
            json!({"kind":1,"k":["requests",0,"copilotCredits"],"v":0.102067}),
            json!({"kind":1,"k":["requests",1,"copilotCredits"],"v":2.0}),
            json!({"kind":1,"k":["requests",1,"result"],"v":{"details":"GPT-6.2 • 11.0 credits"}}),
        ]);
        assert_eq!(result.records.iter().map(|r| (r.credits, r.line)).collect::<Vec<_>>(), vec![(3.56761, 1), (11.0, 4)]);
    }

    #[test]
    fn counts_only_the_unattributed_remainder_of_session_usage() {
        let mut result = scan_entries(&[
            json!({"kind":0,"v":{"sessionId":"accounting","requests":[{"requestId":"one","copilotCredits":12.0,"sessionCopilotCredits":20.0}]}}),
            json!({"kind":1,"k":["sessionCostInfo"],"v":{"credits":25.0,"status":"complete"}}),
            json!({"kind":2,"k":["requests"],"v":[{"requestId":"two","copilotCredits":10.0,"sessionCopilotCredits":29.0}]}),
            json!({"kind":1,"k":["sessionCostInfo"],"v":{"credits":30.0}}),
            json!({"kind":1,"k":["sessionCostInfo"]}),
        ]);
        assign_turn_indices(&mut result.records);
        assert_eq!(result.parse_errors, 0);
        assert_eq!(result.records.iter().map(|r| r.credits).sum::<f64>(), 29.0);
        let remainder = result.records.last().unwrap();
        assert_eq!((remainder.credits, remainder.line, remainder.turn_index, remainder.session_total), (7.0, 3, None, None));
        assert_eq!(remainder.source, "vscode.sessionCost");
    }

    #[test]
    fn effort_uses_request_time_and_excludes_visible_automatic_requests() {
        let result = scan_entries(&[
            json!({"kind":0,"v":{"sessionId":"effort","requests":[
                {"requestId":"manual","timestamp":1000,"responseTimestamp":99999,"message":{"text":"private"}},
                {"requestId":"terminal","timestamp":2000,"message":{"text":"[Terminal completed]"},"terminalExecutionId":"job","isSystemInitiated":false,"copilotCredits":1},
                {"requestId":"missing","message":"legacy","responseTimestamp":5000}
            ]}}),
            json!({"kind":2,"k":["pendingRequests"],"v":[{"id":"queued","request":{"requestId":"queued","timestamp":3000,"message":{"text":"private"}}}]}),
            json!({"kind":2,"k":["pendingRequests"],"i":0}),
        ]);
        assert_eq!(result.parse_errors, 0);
        assert_eq!(result.records.len(), 1);
        let shape = result.effort.iter().map(|event| (event.id.as_deref(), event.timestamp_ms, event.kind, event.line)).collect::<Vec<_>>();
        assert_eq!(shape, vec![(Some("manual"), Some(1000), EffortKind::Human, 1), (Some("terminal"), Some(2000), EffortKind::Automatic, 1),
            (Some("missing"), None, EffortKind::Human, 1), (Some("queued"), Some(3000), EffortKind::Queued, 2)]);
    }

    #[test]
    fn reconciles_overlapping_totals_without_adding_subagents() {
        let result = scan_entries(&[
            json!({"kind":0,"v":{"requests":[{
                "copilotCredits":120.12345,"modelId":"copilot/gpt-test",
                "response":[{"toolSpecificData":{"kind":"subagent","credits":100.0}}],
                "result":{"details":"GPT-Test • 120.1 credits"}
            }]}}),
            json!({"kind":1,"k":["requests",0,"result"],"v":{"details":"GPT-Test • 120.1 credits"}}),
            json!({"kind":1,"k":["requests",0,"copilotCredits"],"v":140.25}),
        ]);
        assert_eq!(result.records.len(), 1);
        assert_eq!(result.records[0].credits, 140.25);
        assert_eq!(result.records[0].line, 3);
        assert_eq!(result.records[0].model, "GPT-Test");
    }

    #[test]
    fn preserves_final_total_when_widget_total_is_partial() {
        let result = scan_entries(&[json!({"kind":0,"v":{"requests":[
            {"copilotCredits":1610.629925,"result":{"details":"GPT-Test • 3192.1 credits"}},
            {"copilotCredits":12.56,"result":{"details":"GPT-Test • 12.6 credits"}},
            {"copilotCredits":0,"result":{"details":"GPT-Test • 0 credits"}}
        ]}})]);
        assert_eq!(result.records.iter().map(|r| r.credits).collect::<Vec<_>>(), vec![3192.1,12.56,0.0]);
    }

    #[test]
    fn supports_legacy_singular_credit_and_missing_response_ids() {
        let result = scan_entries(&[
            json!({"kind":0,"v":{"sessionId":"legacy","requests":[
                {"result":{"details":"GPT-Test • 1 credit"}},
                {"result":{"details":"GPT-Test • 2 credits"}}
            ]}}),
            json!({"kind":1,"k":["requests",0,"result","details"],"v":"GPT-Test • 3 credits"}),
            json!({"kind":1,"k":["requests",0,"result","details"],"v":"GPT-Test • 3 credits"}),
        ]);
        assert_eq!(result.records.len(), 2);
        assert_eq!(result.records[0].credits, 3.0);
        assert_eq!(result.records[0].line, 3);
        assert_eq!(result.records[1].credits, 2.0);
        assert!(parse_credit_details("GPT-Test • 1 credit").is_some());
    }

    #[test]
    fn legacy_multiplier_is_not_a_credit_or_part_of_session_credit_remainder() {
        let result = scan_entries(&[json!({"kind":0,"v":{"sessionId":"mixed-plan",
            "sessionCostInfo":{"credits":9}, "requests":[
                {"requestId":"legacy","result":{"details":"GPT-Test • 1x"}},
                {"requestId":"fraction","result":{"details":"GPT-Test • 0.33×"}},
                {"requestId":"included","result":{"details":"GPT-Test • 0x"}},
                {"requestId":"metered","copilotCredits":4,"result":{"details":"GPT-Test • 1x"}}
            ]}})]);
        assert_eq!(result.records.iter().map(|r| r.request_count).collect::<Vec<_>>(),
            vec![Some(1), Some(1), Some(1), None, None]);
        assert_eq!(result.records.iter().map(|r| r.credits).collect::<Vec<_>>(), vec![0.0, 0.0, 0.0, 4.0, 5.0]);
    }

    #[test]
    fn replays_array_tail_replacement_append_and_truncation() {
        let result = scan_entries(&[
            json!({"kind":0,"v":{"requests":[{"copilotCredits":10},{"copilotCredits":20}]}}),
            json!({"kind":2,"k":["requests"],"i":1,"v":[{"copilotCredits":30}]}),
            json!({"kind":2,"k":["requests"],"v":[{"copilotCredits":40}]}),
            json!({"kind":2,"k":["requests"],"i":2}),
            json!({"kind":1,"k":["requests",1,"copilotCredits"],"v":35}),
        ]);
        assert_eq!(result.parse_errors, 0);
        assert_eq!(result.records.iter().map(|r| r.credits).collect::<Vec<_>>(), vec![10.0,35.0,40.0]);
        assert_eq!(result.records[1].line, 5);
        assert!(result.records[2].details.contains("historical turn"));
    }

    #[test]
    fn full_snapshots_replace_state_and_do_not_recharge() {
        let snapshot = json!({"kind":0,"v":{"sessionId":"snapshot","requests":[
            {"copilotCredits":42.25,"result":{"details":"GPT-Test • 42.3 credits"}}
        ]}});
        let result = scan_entries(&[snapshot.clone(),snapshot]);
        assert_eq!(result.records.len(), 1);
        assert_eq!(result.records[0].credits, 42.25);
        assert_eq!(result.records[0].line, 2);
    }

    #[test]
    fn ignores_unrelated_cost_fields_and_discards_conversation_text() {
        let mut session = Session::default();
        assert!(session.apply(&json!({"kind":0,"v":{"requests":[{
            "message":{"text":"private"},"response":[{"details":"Fake • 999 credits"}],
            "result":{"metadata":{"renderedUserMessage":"private"},"details":"GPT-Test • 1 credit"}
        }]}}),1));
        let retained = session.state.to_string();
        assert!(!retained.contains("private"));
        assert!(!retained.contains("Fake"));
        assert_eq!(session.records(Path::new("session.jsonl"),None)[0].credits,1.0);
    }

    #[test]
    fn invalid_usage_and_malformed_tail_do_not_invent_charges() {
        let result = scan_entries(&[
            json!({"kind":0,"v":{"requests":[
                {"copilotCredits":-1,"result":{"details":"GPT-Test • 2 credits"}},
                {"copilotCredits":"invalid"},
                {"result":{"details":"GPT-Test • NaN credits"}},
                {"result":{"details":"GPT-Test • -10 credits"}}
            ]}}),
            json!({"kind":2,"k":["requests"],"i":100,"v":[]}),
        ]);
        assert_eq!(result.parse_errors,1);
        assert_eq!(result.records.len(),1);
        assert_eq!(result.records[0].credits,2.0);
    }

    #[test]
    fn mixed_old_and_new_turns_keep_user_exchange_numbers() {
        let mut result = scan_entries(&[
            json!({"kind":0,"v":{"sessionId":"mixed","requests":[
                {"requestId":"one","result":{"details":"GPT-Test • 10 credits"}},
                {"requestId":"two"},
                {"requestId":"three","copilotCredits":25.125,
                 "response":[
                     {"toolSpecificData":{"kind":"subagent","credits":9}},
                     {"toolSpecificData":{"kind":"subagent","credits":6}}
                 ]},
                {"requestId":"four"}
            ]}}),
            json!({"kind":1,"k":["requests",2,"copilotCredits"],"v":30.125}),
        ]);
        assign_turn_indices(&mut result.records);
        assert_eq!(result.records.len(),2);
        assert_eq!(result.records[0].credits,10.0);
        assert_eq!(result.records[0].turn_index,Some(1));
        assert_eq!(result.records[1].credits,30.125);
        assert_eq!(result.records[1].turn_index,Some(3));
        assert!(result.records.iter().all(|r| r.session_total == Some(4)));
    }

    #[test]
    fn unwrapped_legacy_snapshot_keeps_direct_result_metadata() {
        let result = scan_entries(&[json!({"title":"Legacy title","requests":[{
            "result":{"details":"GPT-Test • 1 credit","sessionId":"internal-session",
                "responseId":"response","requestId":"request","modelId":"copilot/gpt-test",
                "agentId":"agent","timings":{"totalElapsed":250},"completedAt":1000}
        }]})]);
        assert_eq!(result.parse_errors,0);
        let r=&result.records[0];
        assert_eq!(r.session_id.as_deref(),Some("internal-session"));
        assert_eq!(r.response_id.as_deref(),Some("response"));
        assert_eq!(r.request_id.as_deref(),Some("request"));
        assert_eq!(r.model_id.as_deref(),Some("gpt-test"));
        assert_eq!(r.agent_id.as_deref(),Some("agent"));
        assert_eq!(r.chat_title.as_deref(),Some("Legacy title"));
        assert_eq!(r.duration_ms,Some(250));
        assert_eq!(r.timestamp_ms,Some(1000));
    }

    #[test]
    fn rewound_billed_requests_are_retained_without_duplicate_snapshots() {
        let first = json!({"requestId":"first","copilotCredits":100});
        let retry = json!({"requestId":"retry","result":{"details":"GPT-Test • 20 credits"}});
        let result = scan_entries(&[
            json!({"kind":0,"v":{"requests":[first.clone()]}}),
            json!({"kind":2,"k":["requests"],"i":0}),
            json!({"kind":2,"k":["requests"],"v":[retry.clone()]}),
            json!({"kind":0,"v":{"requests":[retry.clone()]}}),
            json!({"kind":1,"k":["requests",0,"result","details"],"v":"GPT-Test • 25 credits"}),
        ]);
        assert_eq!(result.records.len(),2);
        assert_eq!(result.records[0].credits,100.0);
        assert_eq!(result.records[0].turn_index,Some(1));
        assert!(result.records[0].details.contains("historical turn"));
        assert_eq!(result.records[1].credits,25.0);
        assert_eq!(result.records[1].turn_index,Some(2));
        assert_eq!(result.records[1].session_total,Some(2));
    }

    #[test]
    fn unrelated_javascript_surrogates_do_not_block_billing() {
        let text = br#"{"kind":0,"v":{"requests":[{"requestId":"one","copilotCredits":12.5,"response":[{"text":"\ud800"}]}]}}"#;
        let entry = parse_entry(text).unwrap().unwrap();
        let mut session = Session::default();
        assert!(session.apply(&entry,1));
        assert_eq!(session.records(Path::new("session.jsonl"),None)[0].credits,12.5);
        let delta = br#"{"kind":2,"k":["requests",0,"response"],"v":[{"text":"\ud800"}]}"#;
        assert!(parse_entry(delta).unwrap().is_none());
    }
}