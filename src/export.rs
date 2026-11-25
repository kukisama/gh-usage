//! Integrity metadata for file-based CSV exports. CSV columns remain unchanged.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use chrono::DateTime;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::billing::parse_premium_details;
use crate::record::{BillingUnit, EffortRow, UsageRecord};

pub(crate) const USAGE_PREFIX: &str = "copilot-usage-";
pub(crate) const EFFORT_PREFIX: &str = "copilot-effort-";
pub(crate) const EFFORT_HEADER: [&str; 8] = [
    "hostname",
    "timestamp_ms",
    "source",
    "project",
    "session_id",
    "event_id",
    "file",
    "line",
];

pub(crate) fn usage_csv_name(host: &str) -> String {
    format!("{USAGE_PREFIX}{host}.csv")
}

pub(crate) fn canonical_csv_host(path: &Path) -> Option<String> {
    let host = host_from_csv(path, USAGE_PREFIX)?;
    (path.file_name()?.to_str()? == usage_csv_name(&host)).then_some(host)
}

pub(crate) fn host_from_csv(path: &Path, prefix: &str) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    (stem.get(..prefix.len())?.eq_ignore_ascii_case(prefix) && stem.len() > prefix.len())
        .then(|| stem[prefix.len()..].to_owned())
}

pub(crate) fn effort_path(usage: &Path) -> PathBuf {
    if let Some(suffix) = host_from_csv(usage, USAGE_PREFIX) {
        usage.with_file_name(format!("{EFFORT_PREFIX}{suffix}.csv"))
    } else {
        usage.with_extension("effort.csv")
    }
}

#[derive(Deserialize, Serialize)]
pub(crate) struct Receipt {
    version: u8,
    pub hostname: String,
    usage_sha256: String,
    effort_sha256: Option<String>,
}

pub(crate) fn receipt_path(usage: &Path) -> PathBuf {
    usage.with_extension("export.json")
}

pub(crate) fn read_csv(path: &Path) -> Result<Vec<u8>> {
    fs::read(path).with_context(|| format!("read CSV {}", path.display()))
}

fn checksum(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

impl Receipt {
    pub fn write(
        usage: &Path,
        usage_sha256: String,
        effort_sha256: Option<String>,
        hostname: &str,
    ) -> Result<()> {
        let receipt = Self {
            version: 1,
            hostname: hostname.to_owned(),
            usage_sha256,
            effort_sha256,
        };
        let path = receipt_path(usage);
        fs::write(&path, serde_json::to_vec_pretty(&receipt)?)
            .with_context(|| format!("write export receipt {}", path.display()))
    }

    fn read(usage: &Path) -> Result<Option<Self>> {
        let path = receipt_path(usage);
        let contents = match fs::read(&path) {
            Ok(contents) => contents,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => {
                return Err(e).with_context(|| format!("read export receipt {}", path.display()));
            }
        };
        let receipt: Self = serde_json::from_slice(&contents)
            .with_context(|| format!("invalid export receipt {}", path.display()))?;
        ensure!(
            receipt.version == 1 && !receipt.hostname.trim().is_empty(),
            "unsupported or incomplete export receipt {}",
            path.display()
        );
        Ok(Some(receipt))
    }

    fn verify_usage(&self, usage: &Path, bytes: &[u8]) -> Result<()> {
        ensure!(
            checksum(bytes) == self.usage_sha256,
            "usage CSV does not match export receipt: {} (copy files from the same export)",
            usage.display()
        );
        Ok(())
    }

    pub fn effort(&self, path: &Path) -> Result<Option<Vec<u8>>> {
        let Some(expected) = &self.effort_sha256 else {
            return Ok(None);
        };
        if !path.try_exists()? {
            return Ok(None);
        }
        let bytes = read_csv(path)?;
        ensure!(
            checksum(&bytes) == *expected,
            "effort CSV does not match export receipt: {} (copy files from the same export)",
            path.display()
        );
        Ok(Some(bytes))
    }
}

pub(crate) fn write_usage(
    path: &Path,
    rows: &[UsageRecord],
    emit_bom: bool,
) -> Result<Option<String>> {
    if path.as_os_str() == "-" {
        let mut writer = csv::Writer::from_writer(std::io::stdout().lock());
        for row in rows {
            writer.serialize(row)?;
        }
        writer.flush()?;
        return Ok(None);
    }
    write_csv(path, rows, emit_bom, None).map(Some)
}

pub(crate) fn write_effort(path: &Path, rows: &[EffortRow], emit_bom: bool) -> Result<String> {
    write_csv(path, rows, emit_bom, Some(&EFFORT_HEADER))
}

fn write_csv<T: Serialize>(
    path: &Path,
    rows: &[T],
    emit_bom: bool,
    header: Option<&[&str]>,
) -> Result<String> {
    let bytes = if emit_bom {
        b"\xEF\xBB\xBF".to_vec()
    } else {
        Vec::new()
    };
    let mut writer = csv::WriterBuilder::new()
        .has_headers(header.is_none())
        .from_writer(bytes);
    if let Some(header) = header {
        writer.write_record(header)?;
    }
    for row in rows {
        writer.serialize(row)?;
    }
    let bytes = writer.into_inner()?;
    fs::write(path, &bytes).with_context(|| format!("write CSV {}", path.display()))?;
    Ok(checksum(&bytes))
}

pub(crate) struct CsvExport {
    pub usage: Vec<UsageRecord>,
    pub empty_host: Option<String>,
    pub effort: Option<(String, Vec<EffortRow>)>,
}

impl CsvExport {
    pub fn read(path: &Path, include_effort: bool) -> Result<Self> {
        Self::read_inner(path, include_effort, None)?
            .context("explicit CSV import unexpectedly skipped")
    }

    pub fn read_canonical(path: &Path, host: &str, include_effort: bool) -> Result<Option<Self>> {
        Self::read_inner(path, include_effort, Some(host))
    }

    fn read_inner(
        path: &Path,
        include_effort: bool,
        canonical_host: Option<&str>,
    ) -> Result<Option<Self>> {
        let receipt = Receipt::read(path)?;
        if let (Some(host), Some(receipt)) = (canonical_host, &receipt)
            && host != receipt.hostname
        {
            warn_noncanonical(path, &receipt.hostname);
            return Ok(None);
        }
        let bytes = read_csv(path)?;
        if let Some(receipt) = &receipt {
            receipt.verify_usage(path, &bytes)?;
        } else if let Some(host) = canonical_host
            && let Some(actual) = legacy_foreign_host(path, &bytes, host)?
        {
            warn_noncanonical(path, &actual);
            return Ok(None);
        }
        let fallback = host_from_csv(path, USAGE_PREFIX);
        let mut reader = csv::Reader::from_reader(bytes.as_slice());
        let headers = reader
            .headers()
            .with_context(|| format!("read usage header {}", path.display()))?;
        let has_credits = headers.iter().any(|column| column == "credits");
        ensure!(
            headers.is_empty()
                || headers.iter().any(|column| matches!(
                    column,
                    "credits" | "request_count" | "premium_requests" | "unclassified_usage"
                )),
            "expected a usage CSV with billing columns: {}",
            path.display()
        );
        let mut usage = Vec::new();
        for (index, row) in reader.deserialize::<UsageRecord>().enumerate() {
            let row_context = || {
                format!(
                    "invalid usage CSV {} (data row {})",
                    path.display(),
                    index + 1
                )
            };
            let mut record = row.with_context(row_context)?;
            let host = record.hostname.trim();
            if let Some(receipt) = &receipt {
                ensure!(
                    host == receipt.hostname,
                    "{}: machine does not match export receipt",
                    row_context()
                );
            }
            if host.is_empty() {
                record.hostname = fallback.as_deref().with_context(|| format!(
                    "{}: missing machine; use the original copilot-usage-<machine>.csv filename", row_context()
                ))?.to_owned();
            } else if host != record.hostname {
                record.hostname = host.to_owned();
            }
            ensure!(
                has_credits
                    || record.request_count.is_some()
                    || record.premium_requests.is_some()
                    || record.unclassified_usage.is_some(),
                "{}: missing billing value",
                row_context()
            );
            normalize_units(&mut record).with_context(row_context)?;
            ensure!(
                record.timestamp_ms.is_none_or(valid_timestamp),
                "{}: invalid timestamp_ms",
                row_context()
            );
            usage.push(record);
        }
        let empty_host = if usage.is_empty() {
            receipt.as_ref().map(|r| r.hostname.clone()).or(fallback)
        } else {
            None
        };
        let companion = effort_path(path);
        let effort = if !include_effort {
            None
        } else if let Some(receipt) = receipt {
            match receipt.effort(&companion)? {
                Some(bytes) => Some((
                    receipt.hostname.clone(),
                    read_effort(&companion, &bytes, &receipt.hostname)?,
                )),
                None => {
                    if receipt.effort_sha256.is_some() {
                        eprintln!(
                            "warning: effort not imported; missing {}",
                            companion.display()
                        );
                    } else if companion.try_exists()? {
                        eprintln!(
                            "warning: ignoring {} (this export has no effort data)",
                            companion.display()
                        );
                    }
                    None
                }
            }
        } else {
            ensure!(
                !companion.try_exists()?,
                "cannot verify effort CSV {} without {} (re-export or use --no-effort)",
                companion.display(),
                receipt_path(path).display()
            );
            None
        };
        Ok(Some(Self {
            usage,
            empty_host,
            effort,
        }))
    }
}

fn warn_noncanonical(path: &Path, host: &str) {
    eprintln!(
        "warning: ignoring {}: export belongs to {host}; --merge only reads {}",
        path.display(),
        usage_csv_name(host)
    );
}

fn legacy_foreign_host(path: &Path, bytes: &[u8], expected: &str) -> Result<Option<String>> {
    let mut reader = csv::Reader::from_reader(bytes);
    let context = || format!("read machine identity from {}", path.display());
    let Some(index) = reader
        .headers()
        .with_context(context)?
        .iter()
        .position(|key| key == "hostname")
    else {
        return Ok(None);
    };
    let mut row = csv::StringRecord::new();
    while reader.read_record(&mut row).with_context(context)? {
        if let Some(host) = row
            .get(index)
            .map(str::trim)
            .filter(|h| !h.is_empty() && *h != expected)
        {
            return Ok(Some(host.to_owned()));
        }
    }
    Ok(None)
}

fn valid_timestamp(ms: i64) -> bool {
    // The report uses civil dates in Beijing time with four-digit years.
    const OFFSET_MS: i64 = 8 * 3_600_000;
    let Some(date) = ms
        .checked_add(OFFSET_MS)
        .and_then(DateTime::from_timestamp_millis)
    else {
        return false;
    };
    use chrono::Datelike;
    (100..=9999).contains(&date.year())
}

fn normalize_units(r: &mut UsageRecord) -> Result<()> {
    let finite = |n: f64| n.is_finite() && n >= 0.0;
    ensure!(
        finite(r.credits)
            && r.premium_requests.is_none_or(finite)
            && r.unclassified_usage.is_none_or(finite),
        "invalid billing amount"
    );
    let unit_fields = [
        r.request_count.is_some(),
        r.premium_requests.is_some(),
        r.unclassified_usage.is_some(),
    ];
    let count = unit_fields.into_iter().filter(|x| *x).count();
    // Transitional CSVs used the credits column for precisely the same weighted value.
    let legacy_mirror = r.premium_requests == Some(r.credits);
    ensure!(
        count <= 1 && (count == 0 || r.credits == 0.0 || legacy_mirror),
        "conflicting billing units"
    );
    ensure!(
        r.request_count.is_none_or(|n| n <= 9_007_199_254_740_991),
        "request count exceeds exact report precision"
    );
    if let Some(weighted) = r.premium_requests {
        if r.source == "vscode.chatSessions"
            && parse_premium_details(
                r.details
                    .strip_suffix(" (historical turn)")
                    .unwrap_or(&r.details),
            )
            .is_some()
        {
            r.set_amount(BillingUnit::RequestCount, 1.0);
        } else {
            r.set_amount(BillingUnit::Unclassified, weighted);
        }
    } else if count == 0
        && (r.source == "copilot.cli.events" && r.details.contains("(legacy session shutdown)")
            || r.source == "copilot.cli.probe"
                && r.details.starts_with("Copilot CLI ")
                && !r.details.ends_with(" AI credits (totalNanoAiu)"))
    {
        r.set_amount(BillingUnit::Unclassified, r.credits);
    }
    // Canonicalize -0.0 without rounding a nonzero value.
    if r.credits == 0.0 {
        r.credits = 0.0;
    }
    if r.unclassified_usage == Some(0.0) {
        r.unclassified_usage = Some(0.0);
    }
    Ok(())
}

fn read_effort(path: &Path, bytes: &[u8], host: &str) -> Result<Vec<EffortRow>> {
    let mut reader = csv::Reader::from_reader(bytes);
    let header = reader
        .headers()
        .with_context(|| format!("read effort header {}", path.display()))?;
    ensure!(
        EFFORT_HEADER[..6]
            .iter()
            .all(|name| header.iter().any(|h| h == *name)),
        "invalid effort CSV header: {}",
        path.display()
    );
    let mut rows = Vec::new();
    for (index, row) in reader.deserialize::<EffortRow>().enumerate() {
        let context = || {
            format!(
                "invalid effort CSV {} (data row {})",
                path.display(),
                index + 1
            )
        };
        let row = row.with_context(context)?;
        ensure!(
            row.hostname == host
                && !row.source.trim().is_empty()
                && !row.session_id.trim().is_empty()
                && valid_timestamp(row.timestamp_ms),
            "{}: incorrect machine or missing source, session or submission time",
            context()
        );
        rows.push(row);
    }
    Ok(rows)
}
