//! Offline report inputs and rendering. This module never opens Copilot logs.

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use chrono::Local;

use crate::export::{CsvExport, effort_path, receipt_path};
use crate::ledger::{EffortData, prepare_effort, prepare_usage, validate_totals};
use crate::record::{BillingUnit, UsageRecord};

pub(super) fn ensure_distinct_output(output: &Path, inputs: &[&Path]) -> Result<()> {
    let key = |path: &Path| {
        let absolute = std::path::absolute(path)?;
        Ok::<_, std::io::Error>(fs::canonicalize(&absolute).unwrap_or_else(|_| {
            absolute.parent().and_then(|dir| fs::canonicalize(dir).ok())
                .zip(absolute.file_name()).map_or(absolute.clone(), |(dir, name)| dir.join(name))
        }))
    };
    let target = key(output)?;
    for input in inputs {
        let other = key(input)?;
        let same = if cfg!(windows) {
            target.to_string_lossy().eq_ignore_ascii_case(&other.to_string_lossy())
        } else {
            target == other
        };
        anyhow::ensure!(!same, "output path must not overwrite {}", input.display());
    }
    Ok(())
}

pub(crate) fn write_json(path: &Path, records: &[UsageRecord], emit_bom: bool) -> Result<()> {
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

const TEMPLATE: &str = include_str!("report-template.html");
#[cfg(feature = "rhythm-theme")]
const RHYTHM_THEME: [&str; 3] = [
    include_str!("report-themes/rhythm.html"),
    include_str!("report-themes/rhythm.css"),
    include_str!("report-themes/rhythm.js"),
];
#[cfg(not(feature = "rhythm-theme"))]
const RHYTHM_THEME: [&str; 3] = ["", "", ""];

pub(crate) enum ReportOrigin<'a> {
    Local(&'a str),
    Imported(&'a BTreeSet<String>),
    Merged(&'a BTreeSet<String>),
}

pub(super) fn render_csv(html: &Path, usage: &Path, include_effort: bool, local_host: Option<&str>) -> Result<()> {
    let companion = effort_path(usage);
    ensure_distinct_output(html, &[usage, &companion, &receipt_path(usage)])?;
    let input = CsvExport::read(usage, include_effort)?;
    let hosts: BTreeSet<_> = input.empty_host.into_iter().collect();
    let mut records = input.usage;
    prepare_usage(usage, &mut records)?;
    validate_totals(&records)?;
    let primary_host = local_host.or_else(|| records.first().map(|r| r.hostname.as_str()))
        .or_else(|| hosts.first().map(String::as_str)).unwrap_or("unknown-host");
    let effort = if let Some((host, rows)) = input.effort {
        let mut data = EffortData { rows: prepare_effort(&companion, rows)?, hosts: [host].into() };
        data.sort();
        Some(data)
    } else { None };
    let origin = local_host.map_or(ReportOrigin::Imported(&hosts), ReportOrigin::Local);
    write_html(html, &records, primary_host, effort.as_ref(), origin)
}

pub(super) fn write_html(path: &Path, records: &[UsageRecord], primary_host: &str,
    effort: Option<&EffortData>, origin: ReportOrigin<'_>) -> Result<()> {
    let (local_source_host, imported_hosts) = match origin {
        ReportOrigin::Local(host) => (Some(host), Some([host.to_owned()].into())),
        ReportOrigin::Imported(hosts) | ReportOrigin::Merged(hosts) => (None, Some(hosts.clone())),
    };
    let (credits, history): (Vec<_>, Vec<_>) = records.iter().partition(|r| r.amount().0 == BillingUnit::Credits);
    let mut payload = serde_json::json!({
        "generatedAt": Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        "primaryHost": primary_host,
        "localSourceHost": local_source_host,
        "importedHosts": imported_hosts,
        "records": credits,
        "history": history,
    });
    if let Some(effort) = effort {
        payload["effort"] = serde_json::to_value(&effort.rows)?;
        payload["effortHosts"] = serde_json::to_value(&effort.hosts)?;
    }

    // Keep CSV strings from terminating the inline JSON block.
    let data = serde_json::to_string(&payload)?.replace('<', "\\u003c");
    let html = TEMPLATE
        .replace("__GH_USAGE_SHEETS_SCRIPT__", include_str!("report-sheets.js"))
        .replace("__GH_USAGE_EFFORT_SCRIPT__", include_str!("report-effort.js"))
        .replace("__GH_USAGE_EFFORT_STYLE__", include_str!("report-effort.css"))
        .replace("__GH_USAGE_SNAPDOM__", include_str!("snapdom.js"))
        .replace("__GH_USAGE_THEME_TEMPLATE__", RHYTHM_THEME[0])
        .replace("__GH_USAGE_THEME_STYLE__", RHYTHM_THEME[1])
        .replace("__GH_USAGE_THEME_SCRIPT__", RHYTHM_THEME[2])
        .replace("__GH_USAGE_DATA__", &data);
    fs::write(path, html).with_context(|| format!("create HTML output {}", path.display()))?;
    Ok(())
}