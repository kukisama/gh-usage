//! Offline merge command; reads CSVs, never local session stores.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::export::{CsvExport, canonical_csv_host, effort_path, receipt_path};
use crate::ledger::{EffortData, prepare_effort, prepare_usage, validate_totals};
use crate::record::BillingUnit;
use crate::summary::{INNER, clip, header, historical_request_summary, normalize_zero, plain_border, row};
use crate::{platform, report};

/// Merge usage and effort CSVs in `dir` and render one offline report.
pub(crate) fn run_merge(dir: &Path, double_clicked: bool, view: bool, html: Option<&Path>, no_effort: bool) -> Result<()> {
    if !dir.is_dir() {
        anyhow::bail!("merge target is not a directory: {}", dir.display());
    }

    let mut csv_files = Vec::new();
    let mut protected = Vec::new();
    for entry in fs::read_dir(dir)
        .with_context(|| format!("read_dir failed for {}", dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        // Do not overwrite an archive or a companion, even if it is not selected.
        if path.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
            let lower = n.to_ascii_lowercase();
            lower.ends_with(".csv") || lower.ends_with(".export.json")
        }) {
            protected.push(path.clone());
        }
        if let Some(host) = canonical_csv_host(&path) {
            // Older versions wrote a merged CSV with no export receipt.
            if host == "merged" && !receipt_path(&path).try_exists()? { continue; }
            csv_files.push((path, host));
        }
    }
    csv_files.sort();

    let html_path = html.map(Path::to_path_buf)
        .unwrap_or_else(|| dir.join("copilot-usage-merged.html"));
    report::ensure_distinct_output(&html_path, &protected.iter().map(PathBuf::as_path).collect::<Vec<_>>())?;

    let mut records = Vec::new();
    let mut effort = EffortData::default();
    let mut per_file = Vec::new();
    let mut hosts = BTreeSet::new();
    for (path, hostname) in &csv_files {
        let Some(mut input) = CsvExport::read_canonical(path, hostname, !no_effort)? else { continue };
        anyhow::ensure!(hosts.insert(hostname.clone()), "more than one standard export for machine {hostname}");
        prepare_usage(path, &mut input.usage)?;
        per_file.push((path, input.usage.len()));
        records.extend(input.usage);
        if let Some((host, rows)) = input.effort {
            effort.hosts.insert(host);
            effort.rows.extend(prepare_effort(&effort_path(path), rows)?);
        }
    }

    let primary_host = hosts.first().with_context(|| format!(
        "no standard usage CSV files found in {} (expected copilot-usage-<machine>.csv)", dir.display()))?;
    validate_totals(&records)?;
    effort.sort();
    let effort = (!effort.hosts.is_empty()).then_some(effort);

    report::write_html(&html_path, &records, primary_host, effort.as_ref(), report::ReportOrigin::Merged(&hosts))?;

    let total_credits = normalize_zero(records.iter().filter(|r| r.amount().0 == BillingUnit::Credits).map(|r| r.credits).sum());
    let unclassified = records.iter().filter(|r| r.unclassified_usage.is_some()).count();
    let host_list: Vec<&str> = hosts.iter().map(String::as_str).collect();

    let mut out = String::new();
    out.push_str(&header("GitHub Copilot Usage - Merge"));
    out.push_str(&row(format!(
        "{:<14}{:>12}  {:<14}{:>12}",
        "csv files",
        per_file.len(),
        "effort files",
        effort.as_ref().map_or(0, |data| data.hosts.len())
    )));
    out.push_str(&row(format!(
        "{:<14}{:>12}  {:<14}{:>12}",
        "records",
        records.len(),
        "hosts",
        hosts.len()
    )));
    out.push_str(&row(format!(
        "{:<14}{:>12}  {:<14}{:>12}",
        "total credits",
        format!("{:.1}", total_credits),
        "target dir",
        clip(&dir.display().to_string(), 12)
    )));
    for summary in historical_request_summary(&records) { out.push_str(&row(summary)); }
    if unclassified > 0 {
        out.push_str(&row(format!("unit unverified: {unclassified} rows (re-export source logs)")));
    }
    if let Some(data) = effort {
        out.push_str(&row(format!("effort: {} submissions · {} of {} machines", data.rows.len(), data.hosts.len(), hosts.len())));
    }

    out.push_str(&header("Sources"));
    for (path, count) in &per_file {
        let file_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("<unknown>");
        out.push_str(&row(format!(
            "{:<40}{:>14}",
            clip(file_name, 40),
            format!("{count} rows")
        )));
    }

    out.push_str(&header("Hosts"));
    out.push_str(&row(clip(&host_list.join(", "), INNER)));

    out.push_str(&header("Output"));
    out.push_str(&row(format!(
        "html  {}",
        clip(&html_path.display().to_string(), INNER - 6)
    )));
    out.push_str(&plain_border());
    print!("{out}");

    if view {
        platform::open_path(&html_path)?;
    } else if double_clicked && platform::console_is_interactive() {
        platform::show_merge_exit_notice(&html_path, &host_list);
    }
    Ok(())
}
