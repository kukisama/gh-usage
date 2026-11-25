//! Session discovery and dispatch, including the generic legacy log probe.

use std::collections::{BTreeSet, HashMap};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::Result;
use memchr::memmem;
use serde_json::Value;

use crate::billing::{parse_credit_details, parse_premium_details};
use crate::cli_scan;
use crate::record::{BillingUnit, ContextFields, FileScanResult, UsageRecord};
use crate::scan_fields::{contains_bytes, display_model_id, has_component, merge_context_from_object, modified_after};
use crate::time::format_local_time;
use crate::vscode;

#[derive(Debug, Default)]
struct LineExtraction {
    response_metadata: HashMap<String, ContextFields>,
    records: Vec<RawUsageRecord>,
}

#[derive(Debug)]
struct RawUsageRecord {
    context: ContextFields,
    model: String,
    credits: f64,
    unit: BillingUnit,
    details: String,
}

pub(crate) fn collect_vscode_chat_session_files(root: &Path, cutoff: Option<&SystemTime>) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if root.exists() {
        for entry in fs::read_dir(root)?.flatten().filter(|entry| entry.file_type().is_ok_and(|t| t.is_dir())) {
            collect_vscode_session_directory(&entry.path().join("chatSessions"), cutoff, &mut files);
        }
    }
    // VS Code saves no-folder chats under globalStorage, not workspaceStorage.
    if let Some(user) = root.parent().filter(|_| root.file_name().is_some_and(|n| n.eq_ignore_ascii_case("workspaceStorage"))) {
        collect_vscode_session_directory(&user.join("globalStorage/emptyWindowChatSessions"), cutoff, &mut files);
    }

    Ok(files)
}

fn collect_vscode_session_directory(dir: &Path, cutoff: Option<&SystemTime>, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten().filter(|entry| entry.file_type().is_ok_and(|t| t.is_file())) {
        let path = entry.path();
        let wanted = match path.extension().and_then(|ext| ext.to_str()) {
            Some("jsonl") => true,
            // A JSONL journal can replace the old snapshot in a migration.
            Some("json") => !path.with_extension("jsonl").is_file(),
            _ => false,
        };
        if wanted && modified_after(&path, cutoff) { files.push(path); }
    }
}

pub(crate) fn scan_file(path: &Path, project_map: &HashMap<PathBuf, String>, collect_effort: bool) -> FileScanResult {
    if path.parent().and_then(Path::file_name).is_some_and(|name|
        name.eq_ignore_ascii_case("chatSessions") || name.eq_ignore_ascii_case("emptyWindowChatSessions")) {
        let project = workspace_dir_for_session(path)
            .and_then(|dir| project_map.get(&dir).cloned());
        return vscode::scan(path, project, collect_effort);
    }
    if let Some(result) = cli_scan::scan_legacy_session(path) {
        return result;
    }
    let mut result = FileScanResult::default();

    let Ok(file) = File::open(path) else {
        result.parse_errors += 1;
        return result;
    };

    let source = if has_component(path, "chatSessions") {
        "vscode.chatSessions"
    } else {
        "copilot.cli.probe"
    };

    let cli_workspace = (source == "copilot.cli.probe")
        .then(|| path.parent())
        .flatten()
        .map(cli_scan::read_cli_workspace_metadata)
        .unwrap_or_default();
    let project = workspace_dir_for_session(path)
        .and_then(|dir| project_map.get(&dir).cloned())
        .or(cli_workspace.project);
    let cli_session_id = (source == "copilot.cli.probe")
        .then(|| cli_scan::cli_session_id_from_event_path(path))
        .flatten();

    let mut reader = BufReader::with_capacity(1024 * 1024, file);
    let mut custom_title = cli_workspace.title;
    let mut weak_title: Option<String> = None;
    // VS Code chatSessions JSONL is a state-change log, not an append-only
    // event ledger. The same request result can be persisted repeatedly as
    // later mutations overwrite `requests[index].result`. Keep one logical
    // usage record per stable response identity and update it to the latest
    // persisted state instead of charging every physical JSONL line.
    let mut vscode_response_records: HashMap<String, usize> = HashMap::new();
    let mut line_index = 0usize;
    let mut line = Vec::with_capacity(16 * 1024);

    loop {
        line.clear();
        let bytes_read = match reader.read_until(b'\n', &mut line) {
            Ok(bytes_read) => bytes_read,
            Err(_) => {
                result.parse_errors += 1;
                break;
            }
        };
        if bytes_read == 0 {
            break;
        }

        line_index += 1;
        result.scanned_lines += 1;

        // Title resolution: the authoritative session name is `customTitle`.
        // Scan the header window for it and only fall back to a plain `title`
        // when no customTitle exists anywhere, so an early JSON-schema title
        // (e.g. the model's "Thinking Effort" label) can never win.
        if custom_title.is_none()
            && source == "copilot.cli.probe"
            && let Some(title) = cli_scan::extract_cli_user_title_from_line(&line)
        {
            custom_title = Some(title);
        } else if custom_title.is_none() && line_index <= 64 {
            if let Some(title) = extract_custom_title_from_line(&line) {
                custom_title = Some(title);
            } else if weak_title.is_none()
                && let Some(title) = extract_weak_title_from_line(&line)
            {
                weak_title = Some(title);
            }
        }

        if !is_usage_candidate_line(source, &line) {
            continue;
        }

        result.json_candidate_lines += 1;

        match serde_json::from_slice::<Value>(&line) {
            Ok(value) => {
                let mut extraction = LineExtraction::default();
                collect_line_extraction(&value, ContextFields::default(), &mut extraction);
                for raw in extraction.records {
                    let local =
                        enrich_from_response_metadata(raw.context, &extraction.response_metadata);
                    let mut record = UsageRecord {
                        source: source.to_owned(),
                        hostname: String::new(),
                        timestamp_ms: local.timestamp_ms,
                        local_time_hint: local.timestamp_ms.and_then(format_local_time),
                        chat_title: None,
                        model: raw.model,
                        model_id: local.model_id.as_deref().map(display_model_id),
                        credits: raw.credits,
                        request_count: None,
                        premium_requests: None,
                        unclassified_usage: None,
                        details: raw.details,
                        session_id: local.session_id.clone().or_else(|| cli_session_id.clone()),
                        request_id: local.request_id.clone(),
                        response_id: local.response_id.clone(),
                        agent_id: local.agent_id.clone(),
                        project: project.clone(),
                        duration_ms: local.total_elapsed_ms,
                        turn_index: None,
                        session_total: None,
                        file: path.to_string_lossy().into_owned(),
                        line: line_index,
                    };
                    record.set_amount(raw.unit, raw.credits);
                    store_scanned_record(&mut result.records, &mut vscode_response_records, record);
                }
            }
            Err(_) => {
                result.parse_errors += 1;
                if let Ok(text) = std::str::from_utf8(&line) {
                    collect_records_from_text(source, path, line_index, text, &mut result.records);
                }
            }
        }
    }

    if let Some(title) = custom_title.or(weak_title) {
        for record in &mut result.records {
            record.chat_title = Some(title.clone());
        }
    }

    result
}

fn store_scanned_record(
    records: &mut Vec<UsageRecord>,
    vscode_response_records: &mut HashMap<String, usize>,
    record: UsageRecord,
) {
    if record.source == "vscode.chatSessions"
        && let Some(response_id) = record.response_id.clone()
    {
        if let Some(index) = vscode_response_records.get(&response_id).copied() {
            records[index] = record;
        } else {
            vscode_response_records.insert(response_id, records.len());
            records.push(record);
        }
    } else {
        records.push(record);
    }
}

fn is_usage_candidate_line(source: &str, line: &[u8]) -> bool {
    if contains_bytes(line, b"details") && (contains_bytes(line, b"credits") || contains_bytes(line, b"premium request")) {
        return true;
    }

    source == "copilot.cli.probe"
        && contains_bytes(line, b"session.shutdown")
        && (contains_bytes(line, b"modelMetrics") || contains_bytes(line, b"totalNanoAiu") || contains_bytes(line, b"totalPremiumRequests"))
}

fn collect_line_extraction(value: &Value, context: ContextFields, extraction: &mut LineExtraction) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_line_extraction(item, context.clone(), extraction);
            }
        }
        Value::Object(map) => {
            let mut local = context.clone();
            merge_context_from_object(&mut local, map);
            if let Some(Value::Object(metadata)) = map.get("metadata") {
                merge_context_from_object(&mut local, metadata);
            }

            if let Some(response_id) = local.response_id.clone() {
                let entry = extraction.response_metadata.entry(response_id).or_default();
                merge_context_missing(entry, &local);
            }

            if let Some(details) = map.get("details").and_then(Value::as_str)
                && let Some((model, unit, credits)) = parse_credit_details(details).map(|(m, n)| (m, BillingUnit::Credits, n))
                    .or_else(|| parse_premium_details(details).map(|(m, _)| (m, BillingUnit::RequestCount, 1.0)))
            {
                extraction.records.push(RawUsageRecord {
                    context: local.clone(),
                    model,
                    credits,
                    unit,
                    details: details.to_owned(),
                });
            }

            collect_cli_shutdown_metrics(map, &local, extraction);

            for child in map.values() {
                collect_line_extraction(child, local.clone(), extraction);
            }
        }
        _ => {}
    }
}

fn collect_cli_shutdown_metrics(
    map: &serde_json::Map<String, Value>,
    context: &ContextFields,
    extraction: &mut LineExtraction,
) {
    if map.get("type").and_then(Value::as_str) != Some("session.shutdown") {
        return;
    }

    let Some(data) = map.get("data").and_then(Value::as_object) else { return };
    for (model, unit, credits) in cli_scan::cli_shutdown_amounts(data) {
        let details = cli_scan::shutdown_details(&model, unit, credits);
        extraction.records.push(RawUsageRecord {
            context: context.clone(),
            model,
            credits,
            unit,
            details,
        });
    }
}

/// Returns the workspaceStorage/<hash> directory that owns a chatSessions file:
/// `.../workspaceStorage/<hash>/chatSessions/<id>.jsonl` -> `.../workspaceStorage/<hash>`.
fn workspace_dir_for_session(path: &Path) -> Option<PathBuf> {
    let chat_sessions = path.parent()?;
    if !chat_sessions
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("chatSessions"))
    {
        return None;
    }
    Some(chat_sessions.parent()?.to_path_buf())
}

/// Reads each workspace's `workspace.json` once and maps the storage directory
/// to a friendly project name (the last segment of the opened folder/workspace).
pub(crate) fn build_project_map(files: &[PathBuf]) -> HashMap<PathBuf, String> {
    let mut map: HashMap<PathBuf, String> = HashMap::new();
    let mut dirs: BTreeSet<PathBuf> = BTreeSet::new();
    for file in files {
        if let Some(dir) = workspace_dir_for_session(file) {
            dirs.insert(dir);
        }
    }
    for dir in dirs {
        if let Some(name) = read_project_name(&dir) {
            map.insert(dir, name);
        }
    }
    map
}

fn read_project_name(workspace_dir: &Path) -> Option<String> {
    let text = fs::read_to_string(workspace_dir.join("workspace.json")).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    let object = value.as_object()?;
    if let Some(folder) = object.get("folder").and_then(Value::as_str) {
        return project_name_from_uri(folder, false);
    }
    if let Some(workspace) = object.get("workspace").and_then(Value::as_str) {
        return project_name_from_uri(workspace, true);
    }
    None
}

/// Extracts a human-readable project name from a VS Code file URI.
/// `file:///c%3A/Scripts/projects/opendecknew` -> `opendecknew`.
/// A `.code-workspace` file URI returns the file stem.
fn project_name_from_uri(uri: &str, is_workspace_file: bool) -> Option<String> {
    let without_scheme = uri.strip_prefix("file://").unwrap_or(uri);
    let decoded = percent_decode(without_scheme);
    let trimmed = decoded.trim_end_matches(['/', '\\']);
    let last = trimmed
        .rsplit(['/', '\\'])
        .find(|segment| !segment.is_empty())?;
    let name = if is_workspace_file {
        last.strip_suffix(".code-workspace").unwrap_or(last)
    } else {
        last
    }
    .trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_owned())
    }
}

/// Minimal percent-decoding sufficient for VS Code workspace URIs.
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hi = (bytes[index + 1] as char).to_digit(16);
            let lo = (bytes[index + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi * 16 + lo) as u8);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn collect_records_from_text(
    source: &str,
    path: &Path,
    line_number: usize,
    line: &str,
    records: &mut Vec<UsageRecord>,
) {
    let Some(details_key_index) = line.find("details") else {
        return;
    };
    let Some(credits_index) = line[details_key_index..].find("credits") else {
        return;
    };

    let end = details_key_index + credits_index + "credits".len();
    let start = line[..details_key_index]
        .rfind('"')
        .map(|index| index + 1)
        .unwrap_or(details_key_index);
    let candidate = &line[start..end];

    if let Some((model, credits)) = parse_credit_details(candidate) {
        records.push(UsageRecord {
            source: source.to_owned(),
            hostname: String::new(),
            timestamp_ms: None,
            local_time_hint: None,
            chat_title: None,
            model,
            model_id: None,
            credits,
            request_count: None,
            premium_requests: None,
            unclassified_usage: None,
            details: candidate.to_owned(),
            session_id: None,
            request_id: None,
            response_id: None,
            agent_id: None,
            project: None,
            duration_ms: None,
            turn_index: None,
            session_total: None,
            file: path.to_string_lossy().into_owned(),
            line: line_number,
        });
    }
}

fn enrich_from_response_metadata(
    mut local: ContextFields,
    response_metadata: &HashMap<String, ContextFields>,
) -> ContextFields {
    if let Some(response_id) = local.response_id.as_ref()
        && let Some(metadata) = response_metadata.get(response_id)
    {
        merge_context_missing(&mut local, metadata);
    }
    local
}

fn merge_context_missing(target: &mut ContextFields, source: &ContextFields) {
    if target.session_id.is_none() {
        target.session_id = source.session_id.clone();
    }
    if target.request_id.is_none() {
        target.request_id = source.request_id.clone();
    }
    if target.response_id.is_none() {
        target.response_id = source.response_id.clone();
    }
    if target.model_id.is_none() {
        target.model_id = source.model_id.clone();
    }
    if target.agent_id.is_none() {
        target.agent_id = source.agent_id.clone();
    }
    if target.timestamp_ms.is_none() {
        target.timestamp_ms = source.timestamp_ms;
    }
    if target.total_elapsed_ms.is_none() {
        target.total_elapsed_ms = source.total_elapsed_ms;
    }
}

fn extract_custom_title(value: &Value) -> Option<String> {
    let object = value.as_object()?;
    let key = object.get("k")?.as_array()?;
    let is_custom_title = key
        .iter()
        .any(|part| part.as_str().is_some_and(|text| text == "customTitle"));
    if !is_custom_title {
        return None;
    }

    object
        .get("v")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(ToOwned::to_owned)
}

/// Best-effort byte-level scan for the chat title without parsing the entire
/// session document. Handles both shapes we have seen:
///   - delta jsonl header   : {"k":["customTitle"],"v":"…"}
///   - single-object session: {"kind":0,"v":{"customTitle":"…", "title":"…", …}}
#[cfg(test)]
fn quick_extract_title_from_line(line: &[u8]) -> Option<String> {
    const NEEDLES: [&[u8]; 2] = [b"\"customTitle\":\"", b"\"title\":\""];
    for needle in NEEDLES {
        if let Some(title) = extract_json_string_after(line, needle) {
            return Some(title);
        }
    }
    None
}

/// Walks the bytes after `needle` (which must end at the opening quote of a JSON
/// string) and decodes the first complete JSON string literal that follows.
fn extract_json_string_after(line: &[u8], needle: &[u8]) -> Option<String> {
    let mut search_from = 0;
    while let Some(rel) = memmem::find(&line[search_from..], needle) {
        let start = search_from + rel;
        // Include the opening quote so serde_json can decode the literal.
        let literal_start = start + needle.len() - 1;
        let mut cursor = literal_start + 1;
        while cursor < line.len() {
            let byte = line[cursor];
            if byte == b'\\' {
                cursor += 2;
                continue;
            }
            if byte == b'"' {
                break;
            }
            cursor += 1;
        }
        if cursor >= line.len() {
            break;
        }
        let literal = &line[literal_start..=cursor];
        if let Ok(text) = std::str::from_utf8(literal)
            && let Ok(decoded) = serde_json::from_str::<String>(text)
        {
            let trimmed = decoded.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_owned());
            }
        }
        search_from = cursor + 1;
    }
    None
}

/// Extracts the authoritative session `customTitle` from a single line, covering
/// both the inline (`"customTitle":"…"`) and delta (`{"k":["customTitle"],"v":…}`)
/// shapes.
fn extract_custom_title_from_line(line: &[u8]) -> Option<String> {
    if let Some(title) = extract_json_string_after(line, b"\"customTitle\":\"") {
        return Some(title);
    }
    if contains_bytes(line, b"customTitle")
        && let Ok(value) = serde_json::from_slice::<Value>(line)
        && let Some(title) = extract_custom_title(&value)
    {
        return Some(title);
    }
    None
}

/// Extracts a plain `"title"` as a weak fallback, skipping JSON-schema lines
/// (e.g. a model's `{"reasoningEffort":{"title":"Thinking Effort","enum":[…]}}`)
/// whose `title` is a UI label, not the session title.
fn extract_weak_title_from_line(line: &[u8]) -> Option<String> {
    if contains_bytes(line, b"\"enum\"")
        || contains_bytes(line, b"\"properties\"")
        || contains_bytes(line, b"reasoningEffort")
    {
        return None;
    }
    extract_json_string_after(line, b"\"title\":\"")
}

#[cfg(test)]
#[path = "scan_tests.rs"]
mod tests;