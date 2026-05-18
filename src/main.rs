use std::collections::{BTreeMap, HashMap};
use std::env;
use std::fmt::Write as FmtWrite;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use chrono::{DateTime, Local, Utc};
use clap::{Parser, ValueEnum};
use memchr::memmem;
use rayon::prelude::*;
use serde::Serialize;
use serde_json::Value;
use walkdir::WalkDir;

#[derive(Debug, Parser)]
#[command(name = "gh-usage")]
#[command(
    about = "Scan local GitHub Copilot / VS Code Chat usage records and export credit usage as CSV",
    version
)]
struct Cli {
    /// VS Code workspaceStorage root. Defaults to %APPDATA%\Code\User\workspaceStorage on Windows.
    #[arg(long)]
    vscode_workspace_storage: Option<PathBuf>,

    /// Copilot CLI root. Reserved for optional probing; defaults to %USERPROFILE%\.copilot on Windows.
    #[arg(long)]
    copilot_cli_root: Option<PathBuf>,

    /// Include generic Copilot CLI log probing. Slower and currently best-effort.
    #[arg(long)]
    include_cli_logs: bool,

    /// Only scan files modified within N days.
    #[arg(long)]
    since_days: Option<u64>,

    /// Maximum files to scan, useful while experimenting.
    #[arg(long)]
    max_files: Option<usize>,

    /// Output CSV path. Use '-' for stdout.
    #[arg(long, short, default_value = "copilot-usage.csv")]
    output: PathBuf,

    /// Output format. CSV is the default for spreadsheet inspection.
    #[arg(long, value_enum, default_value_t = OutputFormat::Csv)]
    format: OutputFormat,

    /// Print a short summary to stderr after writing output.
    #[arg(long)]
    summary: bool,

    /// Do not write a UTF-8 BOM for file output. By default files include BOM for Windows Excel compatibility.
    #[arg(long)]
    no_bom: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Csv,
    Json,
}

#[derive(Debug, Clone, Default)]
struct ContextFields {
    session_id: Option<String>,
    request_id: Option<String>,
    response_id: Option<String>,
    model_id: Option<String>,
    agent_id: Option<String>,
    timestamp_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
struct UsageRecord {
    source: String,
    timestamp_ms: Option<i64>,
    local_time_hint: Option<String>,
    chat_title: Option<String>,
    model: String,
    model_id: Option<String>,
    credits: f64,
    details: String,
    session_id: Option<String>,
    request_id: Option<String>,
    response_id: Option<String>,
    agent_id: Option<String>,
    file: String,
    line: usize,
}

#[derive(Debug, Default)]
struct ScanStats {
    scanned_files: usize,
    scanned_lines: usize,
    json_candidate_lines: usize,
    parse_errors: usize,
}

#[derive(Debug, Default)]
struct FileScanResult {
    records: Vec<UsageRecord>,
    scanned_lines: usize,
    json_candidate_lines: usize,
    parse_errors: usize,
}

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
    details: String,
}

fn main() -> Result<()> {
    let total_started = Instant::now();
    let no_args = env::args_os().len() == 1;
    let cli = Cli::parse();

    let vscode_root = cli
        .vscode_workspace_storage
        .or_else(default_vscode_workspace_storage)
        .context(
            "Could not determine VS Code workspaceStorage path; pass --vscode-workspace-storage",
        )?;

    let copilot_cli_root = cli.copilot_cli_root.or_else(default_copilot_cli_root);
    let cutoff = cli.since_days.map(cutoff_system_time);

    let discover_started = Instant::now();
    let mut files = collect_vscode_chat_session_files(&vscode_root, cutoff.as_ref())?;

    if cli.include_cli_logs {
        if let Some(root) = copilot_cli_root.as_ref() {
            files.extend(collect_cli_probe_files(root, cutoff.as_ref())?);
        }
    }

    files.sort();
    files.dedup();

    if let Some(max_files) = cli.max_files {
        files.truncate(max_files);
    }
    let discover_ms = discover_started.elapsed().as_millis();

    let scanned_files = files.len();
    let scan_started = Instant::now();
    let file_results: Vec<FileScanResult> = files.par_iter().map(|path| scan_file(path)).collect();
    let scan_ms = scan_started.elapsed().as_millis();

    let reduce_started = Instant::now();
    let mut stats = ScanStats {
        scanned_files,
        ..ScanStats::default()
    };
    let mut records = Vec::new();

    for result in file_results {
        stats.scanned_lines += result.scanned_lines;
        stats.json_candidate_lines += result.json_candidate_lines;
        stats.parse_errors += result.parse_errors;
        records.extend(result.records);
    }

    records.sort_by(|a, b| {
        a.timestamp_ms
            .cmp(&b.timestamp_ms)
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
            .then_with(|| a.response_id.cmp(&b.response_id))
    });
    records.dedup_by(|a, b| {
        a.file == b.file
            && a.line == b.line
            && a.response_id == b.response_id
            && a.details == b.details
    });
    let reduce_ms = reduce_started.elapsed().as_millis();

    let write_started = Instant::now();
    match cli.format {
        OutputFormat::Csv => write_csv(&cli.output, &records, !cli.no_bom)?,
        OutputFormat::Json => write_json(&cli.output, &records, !cli.no_bom)?,
    }
    let write_ms = write_started.elapsed().as_millis();
    let total_ms = total_started.elapsed().as_millis();

    if no_args || cli.summary {
        let report = build_summary_report(
            &cli.output,
            &records,
            &stats,
            discover_ms,
            scan_ms,
            reduce_ms,
            write_ms,
            total_ms,
        );
        if cli.output.as_os_str() == "-" {
            eprint!("{report}");
        } else {
            print!("{report}");
        }
    }

    Ok(())
}

fn build_summary_report(
    output: &Path,
    records: &[UsageRecord],
    stats: &ScanStats,
    discover_ms: u128,
    scan_ms: u128,
    reduce_ms: u128,
    write_ms: u128,
    total_ms: u128,
) -> String {
    let total_credits = normalize_zero(records.iter().map(|record| record.credits).sum());
    let mut daily: BTreeMap<String, (usize, f64)> = BTreeMap::new();

    for record in records {
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

    let mut report = String::new();
    let _ = writeln!(report, "GitHub Copilot usage summary");
    let _ = writeln!(report, "output={}", output.display());
    let _ = writeln!(report, "records={}", records.len());
    let _ = writeln!(report, "total_credits={:.3}", total_credits);
    let _ = writeln!(report, "active_days={}", active_days);
    let _ = writeln!(
        report,
        "avg_credits_per_active_day={:.3}",
        average_per_active_day
    );
    let _ = writeln!(report);
    let _ = writeln!(report, "daily_credits:");
    if daily.is_empty() {
        let _ = writeln!(report, "  none");
    } else {
        for (day, (count, credits)) in daily {
            let _ = writeln!(report, "  {} records={} credits={:.3}", day, count, credits);
        }
    }
    let _ = writeln!(report);
    let _ = writeln!(report, "scan_stats:");
    let _ = writeln!(report, "  scanned_files={}", stats.scanned_files);
    let _ = writeln!(report, "  scanned_lines={}", stats.scanned_lines);
    let _ = writeln!(report, "  candidate_lines={}", stats.json_candidate_lines);
    let _ = writeln!(report, "  parse_errors={}", stats.parse_errors);
    let _ = writeln!(report);
    let _ = writeln!(report, "timing_ms:");
    let _ = writeln!(report, "  discover_ms={}", discover_ms);
    let _ = writeln!(report, "  scan_ms={}", scan_ms);
    let _ = writeln!(report, "  reduce_ms={}", reduce_ms);
    let _ = writeln!(report, "  write_ms={}", write_ms);
    let _ = writeln!(report, "  total_ms={}", total_ms);
    report
}

fn default_vscode_workspace_storage() -> Option<PathBuf> {
    env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|path| path.join("Code").join("User").join("workspaceStorage"))
}

fn normalize_zero(value: f64) -> f64 {
    if value.abs() < f64::EPSILON {
        0.0
    } else {
        value
    }
}

fn default_copilot_cli_root() -> Option<PathBuf> {
    env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .map(|path| path.join(".copilot"))
}

fn cutoff_system_time(days: u64) -> std::time::SystemTime {
    std::time::SystemTime::now() - std::time::Duration::from_secs(days.saturating_mul(24 * 60 * 60))
}

fn collect_vscode_chat_session_files(
    root: &Path,
    cutoff: Option<&std::time::SystemTime>,
) -> Result<Vec<PathBuf>> {
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    for workspace_entry in fs::read_dir(root)? {
        let Ok(workspace_entry) = workspace_entry else {
            continue;
        };
        let Ok(file_type) = workspace_entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }

        let chat_sessions = workspace_entry.path().join("chatSessions");
        let Ok(session_entries) = fs::read_dir(chat_sessions) else {
            continue;
        };

        for session_entry in session_entries {
            let Ok(session_entry) = session_entry else {
                continue;
            };
            let path = session_entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("jsonl") {
                continue;
            }
            if !modified_after(&path, cutoff) {
                continue;
            }
            files.push(path);
        }
    }

    Ok(files)
}

fn collect_cli_probe_files(
    root: &Path,
    cutoff: Option<&std::time::SystemTime>,
) -> Result<Vec<PathBuf>> {
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

fn has_component(path: &Path, wanted: &str) -> bool {
    path.components().any(|component| {
        component
            .as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case(wanted)
    })
}

fn modified_after(path: &Path, cutoff: Option<&std::time::SystemTime>) -> bool {
    match cutoff {
        Some(cutoff) => path
            .metadata()
            .and_then(|metadata| metadata.modified())
            .is_ok_and(|modified| modified >= *cutoff),
        None => true,
    }
}

fn scan_file(path: &Path) -> FileScanResult {
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

    let mut reader = BufReader::with_capacity(1024 * 1024, file);
    let mut chat_title = None;
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

        if chat_title.is_none() && line_index <= 64 && contains_bytes(&line, b"customTitle") {
            if let Ok(value) = serde_json::from_slice::<Value>(&line) {
                if let Some(title) = extract_custom_title(&value) {
                    chat_title = Some(title);
                }
            }
        }

        if !contains_bytes(&line, b"credits") || !contains_bytes(&line, b"details") {
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
                    result.records.push(UsageRecord {
                        source: source.to_owned(),
                        timestamp_ms: local.timestamp_ms,
                        local_time_hint: local.timestamp_ms.and_then(format_local_time),
                        chat_title: None,
                        model: raw.model,
                        model_id: local.model_id.as_deref().map(display_model_id),
                        credits: raw.credits,
                        details: raw.details,
                        session_id: local.session_id.clone(),
                        request_id: local.request_id.clone(),
                        response_id: local.response_id.clone(),
                        agent_id: local.agent_id.clone(),
                        file: path.to_string_lossy().into_owned(),
                        line: line_index,
                    });
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

    if let Some(title) = chat_title {
        for record in &mut result.records {
            record.chat_title = Some(title.clone());
        }
    }

    result
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    memmem::find(haystack, needle).is_some()
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

            if let Some(details) = map.get("details").and_then(Value::as_str) {
                if let Some((model, credits)) = parse_credit_details(details) {
                    extraction.records.push(RawUsageRecord {
                        context: local.clone(),
                        model,
                        credits,
                        details: details.to_owned(),
                    });
                }
            }

            for child in map.values() {
                collect_line_extraction(child, local.clone(), extraction);
            }
        }
        _ => {}
    }
}

fn merge_context_from_object(context: &mut ContextFields, map: &serde_json::Map<String, Value>) {
    assign_string(&mut context.session_id, map, "sessionId");
    assign_string(&mut context.request_id, map, "requestId");
    assign_string(&mut context.response_id, map, "responseId");
    assign_string(&mut context.model_id, map, "modelId");
    assign_string(&mut context.agent_id, map, "agentId");

    if context.timestamp_ms.is_none() {
        context.timestamp_ms = map.get("timestamp").and_then(Value::as_i64);
    }

    if let Some(Value::Object(model_state)) = map.get("modelState") {
        if let Some(completed_at) = model_state.get("completedAt").and_then(Value::as_i64) {
            context.timestamp_ms = Some(completed_at);
        }
    }

    if let Some(completed_at) = map.get("completedAt").and_then(Value::as_i64) {
        context.timestamp_ms = Some(completed_at);
    }
}

fn assign_string(target: &mut Option<String>, map: &serde_json::Map<String, Value>, key: &str) {
    if target.is_none() {
        if let Some(value) = map.get(key).and_then(Value::as_str) {
            *target = Some(value.to_owned());
        }
    }
}

fn parse_credit_details(details: &str) -> Option<(String, f64)> {
    let trimmed = details.trim();
    let credits_suffix = " credits";
    let before_suffix = trimmed.strip_suffix(credits_suffix)?;
    let mut parts = before_suffix.rsplitn(2, char::is_whitespace);
    let credits_text = parts.next()?.trim();
    let model_part = parts.next()?.trim();

    let credits = credits_text.parse::<f64>().ok()?;
    let model = model_part
        .trim_end_matches(|ch: char| {
            ch.is_whitespace() || !(ch.is_ascii_alphanumeric() || ch == ')' || ch == ']')
        })
        .trim()
        .to_owned();

    if model.is_empty() {
        None
    } else {
        Some((model, credits))
    }
}

fn display_model_id(model_id: &str) -> String {
    model_id
        .strip_prefix("copilot/")
        .unwrap_or(model_id)
        .to_owned()
}

fn format_local_time(timestamp_ms: i64) -> Option<String> {
    let utc = DateTime::<Utc>::from_timestamp_millis(timestamp_ms)?;
    Some(
        utc.with_timezone(&Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string(),
    )
}

fn format_local_day(timestamp_ms: i64) -> Option<String> {
    let utc = DateTime::<Utc>::from_timestamp_millis(timestamp_ms)?;
    Some(utc.with_timezone(&Local).format("%Y-%m-%d").to_string())
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
            timestamp_ms: None,
            local_time_hint: None,
            chat_title: None,
            model,
            model_id: None,
            credits,
            details: candidate.to_owned(),
            session_id: None,
            request_id: None,
            response_id: None,
            agent_id: None,
            file: path.to_string_lossy().into_owned(),
            line: line_number,
        });
    }
}

fn write_csv(path: &Path, records: &[UsageRecord], emit_bom: bool) -> Result<()> {
    if path.as_os_str() == "-" {
        let mut writer = csv::Writer::from_writer(std::io::stdout());
        for record in records {
            writer.serialize(record)?;
        }
        writer.flush()?;
        return Ok(());
    }

    let mut file = File::create(path)
        .with_context(|| format!("failed to create CSV output {}", path.display()))?;
    if emit_bom {
        file.write_all(b"\xEF\xBB\xBF")?;
    }

    let mut writer = csv::Writer::from_writer(file);
    for record in records {
        writer.serialize(record)?;
    }
    writer.flush()?;
    Ok(())
}

fn write_json(path: &Path, records: &[UsageRecord], emit_bom: bool) -> Result<()> {
    if path.as_os_str() == "-" {
        println!("{}", serde_json::to_string_pretty(records)?);
        return Ok(());
    }

    let mut file = File::create(path)
        .with_context(|| format!("failed to create JSON output {}", path.display()))?;
    if emit_bom {
        file.write_all(b"\xEF\xBB\xBF")?;
    }
    serde_json::to_writer_pretty(file, records)?;
    Ok(())
}

fn enrich_from_response_metadata(
    mut local: ContextFields,
    response_metadata: &HashMap<String, ContextFields>,
) -> ContextFields {
    if let Some(response_id) = local.response_id.as_ref() {
        if let Some(metadata) = response_metadata.get(response_id) {
            merge_context_missing(&mut local, metadata);
        }
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

#[cfg(test)]
mod tests {
    use super::{extract_custom_title, parse_credit_details};
    use serde_json::json;

    #[test]
    fn parses_credit_details_with_bullet_separator() {
        let (model, credits) = parse_credit_details("GPT-5.5 • 105.7 credits").unwrap();

        assert_eq!(model, "GPT-5.5");
        assert_eq!(credits, 105.7);
    }

    #[test]
    fn extracts_custom_title_from_chat_session_line() {
        let value = json!({"kind":1,"k":["customTitle"],"v":"切换速度优化建议"});

        assert_eq!(
            extract_custom_title(&value).as_deref(),
            Some("切换速度优化建议")
        );
    }
}
