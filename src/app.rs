//! Orchestration for the local scan command.

use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

use anyhow::{Context, Result};
use rayon::prelude::*;

use crate::record::{EffortEvent, FileScanResult, ScanStats, UsageRecord};
use crate::export::{self, Receipt, effort_path, receipt_path};
use crate::ledger::{EffortData, prepare_usage, validate_totals};
use crate::summary::build_summary_report;
use crate::time::system_time_to_unix_ms;
use crate::{cli_scan, effort, platform, report, scan};

#[derive(Clone, Copy)]
pub(crate) enum UsageFormat {
    Csv,
    Json,
}

pub(crate) struct OutputPaths {
    usage: PathBuf,
    html: Option<PathBuf>,
    effort_csv: Option<PathBuf>,
    effort_report: Option<PathBuf>,
    format: UsageFormat,
    html_effort: bool,
}

impl OutputPaths {
    pub(crate) fn new(
        usage: PathBuf,
        html: Option<PathBuf>,
        effort_report: Option<PathBuf>,
        format: UsageFormat,
        no_effort: bool,
    ) -> Result<Self> {
        let csv_output = matches!(format, UsageFormat::Csv) && usage.as_os_str() != "-";
        let companion = csv_output.then(|| effort_path(&usage));
        let receipt = csv_output.then(|| receipt_path(&usage));
        let effort_csv = companion.clone().filter(|_| !no_effort);
        let html_effort = html.is_some() && !no_effort;
        if let Some(path) = companion.as_deref() {
            report::ensure_distinct_output(path, &[&usage])?;
        }
        if let Some(path) = receipt.as_deref() {
            let inputs = [Some(usage.as_path()), companion.as_deref()];
            report::ensure_distinct_output(path, &inputs.into_iter().flatten().collect::<Vec<_>>())?;
        }
        if let Some(path) = html.as_deref() {
            let inputs = [Some(usage.as_path()), companion.as_deref(), receipt.as_deref()];
            report::ensure_distinct_output(path, &inputs.into_iter().flatten().collect::<Vec<_>>())?;
        }
        if let Some(path) = effort_report.as_deref() {
            anyhow::ensure!(!path.exists(), "effort report already exists: {} (use a new path)", path.display());
            let outputs = [Some(usage.as_path()), html.as_deref(), companion.as_deref(), receipt.as_deref()];
            report::ensure_distinct_output(path, &outputs.into_iter().flatten().collect::<Vec<_>>())?;
        }
        Ok(Self { usage, html, effort_csv, effort_report, format, html_effort })
    }
}

pub(crate) struct ScanOptions<'a> {
    pub outputs: OutputPaths,
    pub hostname: &'a str,
    pub vscode_root: &'a Path,
    pub cli_root: Option<&'a Path>,
    pub cutoff: Option<SystemTime>,
    pub max_files: Option<usize>,
    pub effort_projects: &'a [String],
    pub total_started: Instant,
    pub effort_until: i64,
    pub emit_bom: bool,
    pub summary: bool,
    pub view: bool,
    pub double_clicked: bool,
}

pub(crate) fn run_scan(options: ScanOptions<'_>) -> Result<()> {
    let ScanOptions { outputs, hostname, vscode_root, cli_root, cutoff, max_files,
        effort_projects, total_started, effort_until, emit_bom, summary, view, double_clicked } = options;
    let OutputPaths { usage, html, effort_csv, effort_report, format, html_effort } = outputs;
    let csv_output = matches!(format, UsageFormat::Csv) && usage.as_os_str() != "-";
    let collect_effort = html_effort || effort_csv.is_some() || effort_report.is_some();
    let (mut records, mut effort_events, stats) = collect(
        vscode_root, cli_root, cutoff.as_ref(), max_files, hostname, collect_effort,
    )?;
    prepare_usage(&usage, &mut records)?;
    validate_totals(&records)?;

    let effort_since = cutoff.as_ref().and_then(system_time_to_unix_ms);
    if collect_effort && let Some(root) = cli_root {
        effort_events.extend(cli_scan::scan_effort(root));
    }
    let effort_events = effort::prepare_events(effort_events, effort_since, effort_until);
    let usage_checksum = match format {
        UsageFormat::Csv => export::write_usage(&usage, &records, emit_bom)?,
        UsageFormat::Json => { report::write_json(&usage, &records, emit_bom)?; None }
    };
    let effort_checksum = if let Some(path) = effort_csv.as_deref() {
        let checksum = export::write_effort(path, &effort::export_rows(&effort_events, hostname), emit_bom)?;
        if summary { eprintln!("Effort CSV saved to {}", path.display()); }
        Some(checksum)
    } else { None };
    if let Some(checksum) = usage_checksum {
        Receipt::write(&usage, checksum, effort_checksum, hostname)?;
        if summary { eprintln!("Export receipt saved to {}", receipt_path(&usage).display()); }
    }
    if let Some(html_path) = html.as_deref() {
        if csv_output {
            report::render_csv(html_path, &usage, html_effort, Some(hostname))?;
        } else {
            // Stdout and JSON retain the existing direct report behavior.
            let effort = html_effort.then(|| EffortData {
                rows: effort::export_rows(&effort_events, hostname), hosts: [hostname.to_owned()].into(),
            });
            report::write_html(html_path, &records, hostname, effort.as_ref(), report::ReportOrigin::Local(hostname))?;
        }
    }
    if let Some(path) = effort_report.as_deref() {
        effort::write_report(path, &effort_events, effort::Scope {
            since: effort_since, until: effort_until,
            vscode_root, cli_root, parse_errors: stats.parse_errors, limited_files: max_files, projects: effort_projects,
        })?;
    }
    let total_ms = total_started.elapsed().as_millis();

    if summary {
        let report = build_summary_report(&usage, html.as_deref(), &records, &stats, total_ms);
        if usage.as_os_str() == "-" {
            eprint!("{report}");
        } else {
            print!("{report}");
        }
    }

    if view {
        platform::open_path(html.as_deref().context("--view requires HTML output")?)?;
    } else if double_clicked && platform::console_is_interactive() {
        platform::show_double_click_exit_notice(&usage, html.as_deref());
    }

    Ok(())
}

fn collect(
    vscode_root: &Path,
    cli_root: Option<&Path>,
    cutoff: Option<&SystemTime>,
    max_files: Option<usize>,
    hostname: &str,
    collect_effort: bool,
) -> Result<(Vec<UsageRecord>, Vec<EffortEvent>, ScanStats)> {
    let mut files = scan::collect_vscode_chat_session_files(vscode_root, cutoff)?;
    let mut cli_database_result = None;

    if let Some(root) = cli_root {
        match cli_scan::scan_cli_usage_database(root, cutoff) {
            Ok(Some(result)) => cli_database_result = Some(result),
            Ok(None) => files.extend(cli_scan::collect_cli_probe_files(root, cutoff)?),
            Err(error) => {
                eprintln!(
                    "warning: could not read Copilot CLI session store ({error:#}); falling back to logs"
                );
                files.extend(cli_scan::collect_cli_probe_files(root, cutoff)?);
            }
        }
    }

    files.sort();
    files.dedup();

    if let Some(max_files) = max_files {
        files.truncate(max_files);
    }
    let scanned_files = files.len() + usize::from(cli_database_result.is_some());
    let project_map = scan::build_project_map(&files);
    let mut file_results: Vec<FileScanResult> = files
        .par_iter()
        .map(|path| scan::scan_file(path, &project_map, collect_effort))
        .collect();
    if let Some(result) = cli_database_result {
        file_results.push(result);
    }
    let mut stats = ScanStats {
        scanned_files,
        ..ScanStats::default()
    };
    let mut records = Vec::new();
    let mut effort_events = Vec::new();

    for result in file_results {
        stats.scanned_lines += result.scanned_lines;
        stats.json_candidate_lines += result.json_candidate_lines;
        stats.parse_errors += result.parse_errors;
        records.extend(result.records);
        effort_events.extend(result.effort);
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
            && a.request_id == b.request_id
            && a.turn_index == b.turn_index
            && a.response_id == b.response_id
            && a.details == b.details
    });
    for record in &mut records {
        record.hostname = hostname.to_owned();
    }
    Ok((records, effort_events, stats))
}