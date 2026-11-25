//! Local human submissions, independent of credits and response timings.

use std::collections::{BTreeMap, HashSet};
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, FixedOffset, NaiveDate, Timelike, Utc};

use crate::record::{EffortEvent as Event, EffortKind as Kind, EffortRow};

fn text(v: &str) -> String {
    v.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
        .replace('`', "&#96;").replace('|', "&#124;").replace(['\r', '\n'], " ")
}

fn china_time(ms: i64) -> Option<DateTime<FixedOffset>> {
    DateTime::from_timestamp_millis(ms)
        .map(|t| t.with_timezone(&FixedOffset::east_opt(8 * 3600).unwrap()))
}

#[derive(Default)]
struct Day {
    submissions: usize,
    slots: u64,
}

impl Day {
    fn mark(&mut self, time: DateTime<FixedOffset>) {
        self.submissions += 1;
        self.slots |= 1 << (2 * time.hour() + time.minute() / 30);
    }

    fn segments(&self) -> Vec<(u32, u32)> {
        let mut segments = Vec::new();
        let mut slot = 0;
        while slot < 48 {
            if self.slots & (1 << slot) == 0 {
                slot += 1;
                continue;
            }
            let start = slot;
            while slot < 48 && self.slots & (1 << slot) != 0 {
                slot += 1;
            }
            segments.push((start, slot));
        }
        segments
    }
}

fn clock(slot: u32) -> String {
    if slot == 48 { "次日00:00".into() } else { format!("{:02}:{:02}", slot / 2, slot % 2 * 30) }
}

pub(super) struct Scope<'a> {
    pub since: Option<i64>,
    pub until: i64,
    pub vscode_root: &'a Path,
    pub cli_root: Option<&'a Path>,
    pub parse_errors: usize,
    pub limited_files: Option<usize>,
    pub projects: &'a [String],
}

pub(super) fn prepare_events(mut events: Vec<Event>, since: Option<i64>, until: i64) -> Vec<Event> {
    events.retain(|e| e.timestamp_ms.is_none_or(|ms| ms <= until && since.is_none_or(|from| ms >= from)));
    let mut occurrences = BTreeMap::new();
    let rank = |e: &Event| match (e.kind, e.timestamp_ms) {
        (Kind::Automatic, _) => 3,
        (Kind::Human, Some(_)) => 2,
        (Kind::Human, None) => 1,
        _ => 0,
    };
    for (index, e) in events.iter().enumerate() {
        if let Some(id) = e.id.as_deref().filter(|s| !s.is_empty()) {
            let found = occurrences.entry((e.source, &e.session, id)).or_insert(index);
            if rank(e) > rank(&events[*found]) { *found = index; }
        }
    }
    let keep: HashSet<_> = occurrences.into_values().collect();
    let mut index = 0;
    events.retain(|event| {
        let retained = event.id.as_deref().is_none_or(str::is_empty) || keep.contains(&index);
        index += 1;
        retained
    });
    events.sort_by(|a, b| (a.timestamp_ms, a.source, &a.session, &a.file, a.line)
        .cmp(&(b.timestamp_ms, b.source, &b.session, &b.file, b.line)));
    events
}

pub(super) fn export_rows(events: &[Event], hostname: &str) -> Vec<EffortRow> {
    events.iter().filter_map(|e| {
        let time = e.timestamp_ms.and_then(china_time)?;
        (e.kind == Kind::Human).then(|| EffortRow {
            hostname: hostname.to_owned(), timestamp_ms: time.timestamp_millis(), source: e.source.to_owned(),
            session_id: e.session.clone(), event_id: e.id.clone(), project: e.project.clone(),
            file: e.file.to_string_lossy().into_owned(), line: e.line,
        })
    }).collect()
}

pub(super) fn write_report(path: &Path, events: &[Event], scope: Scope<'_>) -> Result<()> {
    let Scope { since, until, vscode_root, cli_root, parse_errors, limited_files, projects } = scope;
    let in_project = |e: &&Event| projects.is_empty()
        || e.project.as_deref().is_some_and(|p| projects.iter().any(|name| name.eq_ignore_ascii_case(p)));
    let excluded_projects = events.len() - events.iter().filter(in_project).count();
    let file = OpenOptions::new().write(true).create_new(true).open(path)
        .with_context(|| format!("create effort report {} (use a new path)", path.display()))?;
    let mut out = BufWriter::new(file);
    let cutoff = |ms| china_time(ms).map(|t| t.format("%F %T%.3f").to_string()).unwrap_or_default();
    let from = since.map(cutoff).unwrap_or_else(|| "本地可验证历史的最早记录".into());
    let cli = cli_root.map(|p| text(&p.to_string_lossy())).unwrap_or_else(|| "未纳入".into());
    writeln!(out, "# 本地 AI 人工触发与半小时精力统计\n\n时区：北京时间（UTC+08:00）。范围：{from} 至 {}。\n\nVS Code：`{}`；Copilot CLI：`{cli}`。\n\n按本机已落盘的提交事件统计；不自动判断业务归属。正文和工具结果均不导出。", cutoff(until), text(&vscode_root.to_string_lossy()))?;
    if !projects.is_empty() {
        writeln!(out, "\n项目范围：{}。不匹配或项目未知的来源记录未入表（{excluded_projects} 条）。", projects.iter().map(|s| text(s)).collect::<Vec<_>>().join("、"))?;
    }
    writeln!(out, "\n## 一、原始记录\n\n### 本地人工触发\n\n| 时间（UTC+08:00） | 原始时间（Unix ms UTC） | 来源 | 项目 | 会话 ID | 请求／事件 ID | 源文件:行 |\n| --- | ---: | --- | --- | --- | --- | --- |")?;
    let mut days: BTreeMap<NaiveDate, Day> = BTreeMap::new();
    let mut sessions = HashSet::new();
    let mut excluded = Vec::new();
    for e in events.iter().filter(in_project) {
        let time = e.timestamp_ms.and_then(china_time);
        if let (Kind::Human, Some(time)) = (e.kind, time) {
            days.entry(time.date_naive()).or_default().mark(time);
            sessions.insert((e.source, e.session.as_str()));
            writeln!(out, "| {} | {} | {} | {} | {} | {} | `{}:{}` |", time.format("%F %T%.3f"),
                time.timestamp_millis(), e.source, text(e.project.as_deref().unwrap_or("未识别")), text(&e.session),
                text(e.id.as_deref().unwrap_or("—")), text(&e.file.to_string_lossy()), e.line)?;
        } else {
            excluded.push(e);
        }
    }
    writeln!(out, "\n### 云端交接\n\n未检索云端任务清单；云端创建、等待、执行和完成均不单独计时。\n\n### 排除项\n\n| 原始时间（Unix ms UTC） | 来源 | 会话 ID | 请求／事件 ID | 原因 | 源文件:行 |\n| ---: | --- | --- | --- | --- | --- |")?;
    for e in excluded {
        let reason = match e.kind {
            Kind::Human => "缺少可验证的提交时间",
            Kind::Automatic => "系统、隐藏或子代理事件",
            Kind::Unverified => "无法验证为人工提交",
            Kind::Queued => "排队记录无法与派发一一对应，不另计",
        };
        writeln!(out, "| {} | {} | {} | {} | {reason} | `{}:{}` |",
            e.timestamp_ms.map(|v| v.to_string()).as_deref().unwrap_or("—"), e.source,
            text(&e.session), text(e.id.as_deref().unwrap_or("—")), text(&e.file.to_string_lossy()), e.line)?;
    }
    writeln!(out, "\n## 二、分析规则\n\n整点／半点划格，左闭右开，每格有至少一次人工提交即计 1 周期（0.5 小时）。跨项目、会话和来源只计一次，空格不补、午夜不外延。08:29 与 08:31 占两格，同格多次仍只占一格。相邻格仅在当天并段。精力小时不是人工连续劳动时长，也不是 AI 用量或时长。")?;
    writeln!(out, "\n## 三、每日精力占用分析\n\n| 日期 | 人工触发数 | 半小时周期数 | 折算小时 | 连续管理段数 | 覆盖时段 |\n| --- | ---: | ---: | ---: | ---: | --- |")?;
    let (mut submissions, mut slots, mut segments) = (0, 0, 0);
    for (date, day) in &days {
        let n = day.slots.count_ones();
        let ranges = day.segments();
        let coverage = ranges.iter().map(|&(a, b)| format!("{}–{}", clock(a), clock(b))).collect::<Vec<_>>().join("；");
        submissions += day.submissions;
        slots += n;
        segments += ranges.len();
        writeln!(out, "| {date} | {} | {n} | {:.1} | {} | {coverage} |", day.submissions, f64::from(n) / 2.0, ranges.len())?;
    }
    writeln!(out, "| **合计** | **{submissions}** | **{slots}** | **{:.1}** | **{segments}** | **{} 个活动日，{} 场会话** |", f64::from(slots) / 2.0, days.len(), sessions.len())?;
    if submissions == 0 {
        writeln!(out, "\n当前来源和时间范围未发现可计入的记录；不代表没有人工投入。")?;
    }
    writeln!(out, "\n## 四、证据边界\n\n未保存、已删除、其他设备、网页和无可靠时间戳的操作无法补计。不能唯一关联的排队记录不另占格，已出现在会话里的请求按它保存的时间落格。其他 VS Code 数据根以及没有可读本地事件的 Agent Host 会话不在覆盖范围内。费用采集和 VS Code 重放报告的解析错误：{parse_errors}。")?;
    if !vscode_root.is_dir() {
        writeln!(out, "指定的 VS Code 工作区存储不存在或不可访问；不能将缺失解读为零投入。")?;
    }
    if cli_root.is_some_and(|root| !root.join("session-state").is_dir()) {
        writeln!(out, "指定的 Copilot CLI 会话目录不存在或不可访问；不能将缺失解读为零投入。")?;
    }
    if let Some(limit) = limited_files {
        writeln!(out, "文件扫描使用了 `--max-files {limit}`；VS Code 原始事件不代表所有本地会话。")?;
    }
    out.flush()?;
    eprintln!("Effort report: {slots} half-hour slots, {:.1} h, {}", f64::from(slots) / 2.0, path.display());
    Ok(())
}

pub(super) fn end_of_collection() -> i64 {
    Utc::now().timestamp_millis()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::temporary_root;
    use crate::time::parse_rfc3339_timestamp_ms;
    use std::fs;

    #[test]
    fn effort_report_keeps_raw_events_and_deduplicates_slots_across_sources() {
        let root = temporary_root("effort");
        let report = root.join("report.md");
        let make = |id: &str, time: &str, source, kind| Event {
            source, session: source.into(), id: Some(id.into()), project: Some(source.into()),
            timestamp_ms: parse_rfc3339_timestamp_ms(time), file: root.join("events.jsonl"), line: 1, kind,
        };
        let events = vec![
            make("a", "2026-11-23T08:29:59.999+08:00", "VS Code", Kind::Human),
            make("b", "2026-11-23T08:10:00+08:00", "Copilot CLI", Kind::Human),
            make("b", "2026-11-23T08:10:00+08:00", "Copilot CLI", Kind::Human),
            make("c", "2026-11-23T08:30:00+08:00", "VS Code", Kind::Human),
            make("sys", "2026-11-23T09:30:00+08:00", "VS Code", Kind::Automatic),
            make("d", "2026-11-23T23:59:59+08:00", "VS Code", Kind::Human),
            make("e", "2026-11-24T00:00:00+08:00", "Copilot CLI", Kind::Human),
        ];
        let events = prepare_events(events, None, 1_900_000_000_000);
        assert_eq!(export_rows(&events, "test-host").len(), 5);
        write_report(&report, &events, Scope { since: None, until: 1_900_000_000_000,
            vscode_root: &root, cli_root: Some(&root), parse_errors: 0, limited_files: None,
            projects: &["VS Code".into(), "Copilot CLI".into()] }).unwrap();
        let body = fs::read_to_string(&report).unwrap();
        assert!(body.contains("| 2026-11-23 | 4 | 3 | 1.5 | 2 | 08:00–09:00；23:30–次日00:00 |"));
        assert!(body.contains("| 2026-11-24 | 1 | 1 | 0.5 | 1 | 00:00–00:30 |"));
        assert!(body.contains("| **合计** | **5** | **4** | **2.0** | **3** |"));
        assert!(body.contains("| sys | 系统、隐藏或子代理事件 |"));
        assert!(body.contains("08:29:59.999"));
        fs::remove_dir_all(root).unwrap();
    }
}