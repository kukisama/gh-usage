# gh-usage · See your Copilot usage at a glance

English | [简体中文](design/README.zh-CN.md)

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Built with Rust](https://img.shields.io/badge/Built%20with-Rust-orange.svg?logo=rust)](https://www.rust-lang.org/)
[![Platform](https://img.shields.io/badge/Platform-Windows%20%7C%20Linux%20%7C%20macOS-lightgrey.svg)](#-getting-started)
[![Releases](https://img.shields.io/badge/Releases-download-brightgreen.svg)](https://github.com/kukisama/gh-usage/releases)

> One command turns your scattered local GitHub Copilot usage records into a clean, visual report.
> Nothing is uploaded, nothing phones home — your data stays on your machine.

[What's new in 1.2.6](CHANGELOG.md#126--2026-11-25)

![gh-usage report overview](design/01.jpg)

---

## 🙋 Who it's for

- **Heavy Copilot users** who want to see how many credits they spent this month, which days were busiest, and which model they rely on most.
- **Team leads and engineering managers** who need a quick read on usage across people and projects for reviews and planning.
- **Anyone who reports on their work** and wants a report they can screenshot and share, instead of raw logs.

---

## ✨ Highlights

### ⚡ Fast

Written in Rust, it scans local history directories quickly with a light footprint. In one local run, it processed **877 files** and extracted **1,020 records** in **1.52 seconds**.

```text
+- GitHub Copilot Usage ---------------------------------+
| records               1020  scanned files          877 |
| total credits     157204.3  candidate lines        102 |
| active days             21  parse errors             0 |
| avg / day           7485.9  total time          1.52 s |
+--------------------------------------------------------+
```

### 🔒 Local and private

- **Runs entirely on your machine.** It only reads files that already exist locally — nothing is uploaded.
- **One-click privacy mask.** A built-in toggle blurs hostnames, project names, and session titles, so screenshots are safe to share.
- **You own the data.** Every result is written to a local file you choose — keep it or delete it, your call.

### 🖥️ A report that's easy to use

The report is a **single self-contained HTML file** — double-click to open it in any browser. No database, no server, no internet required. Clean dark theme, tidy layout, and built-in English/Chinese switching.

---

## 📊 What's in the report

**Usage** shows GitHub AI Credits. **Effort time** shows when you submitted requests. **Historical requests** appears when legacy request-era records are available. Machine and project filters apply across sheets; Historical requests keeps its own date range.

### Key metrics at a glance

Total credits, active days, record count, daily average, total AI interaction time, and average time per exchange — the numbers you care about, right at the top.

![Key metric cards](design/dashboard-kpi.jpg)

### Trends and model breakdown

A daily bar chart shows which days were busiest, and a donut chart breaks down each model's share at a glance.

![Daily trend and model split](design/dashboard-charts.jpg)

### Drill down by project and session

A per-project bar chart lets you click to filter, and a "top sessions by credits" list shows exactly where your usage went.

![By project and top sessions](design/dashboard-project.jpg)

### A searchable, filterable records table

The records table supports keyword search, filtering by model and source, click-to-sort columns, and pagination. The screenshot below has privacy mode on — sensitive fields are masked automatically.

**Read it as session → user turn.** A turn is one user request and the ensuing agent work, not an individual tool, MCP, or subagent call. The table shows one row per known turn with its recorded total, including recorded subtask usage, without expanding child billing. CLI model records sharing a turn are combined for display; the CSV/JSON keeps the source aggregates for audit. Old CLI session totals without turn information are labelled as summaries, not invented turns.

**New and old VS Code logs are both supported.** The scanner replays snapshots and updates, using cumulative `copilotCredits` when available and `result.details` for older logs. If both exist, it selects the more complete total (allowing for the footer's rounding), never their sum. Already-included subagent costs are not added again. Active/interrupted turns can appear without a final footer; missing usage in another turn does not renumber later turns. Model labels describe turn-level attribution, not a per-subagent model bill.

Rewind/retry does not refund already recorded usage: identifiable earlier requests are retained as **historical turns** and included in the session's exchange count and credits, even when no longer visible in the current chat.

The scanner reads JSON snapshots and JSONL journals for workspace and no-folder chats. With no final credit footer, it retains the highest recorded cumulative turn value. If a reported session total is higher than the sum of its turns, only the difference is shown as unattributed session usage, without inventing a model or turn.

For Copilot CLI records, `events.jsonl` remains the traceable source while SQLite supplies precise credits and timing. Open a row's source popover and choose **Open** to jump to the matching JSONL event line in VS Code.

![Records table with privacy mask on](design/dashboard-records.jpg)

### Historical requests

The [legacy request-era](https://docs.github.com/en/copilot/concepts/billing/copilot-requests) sheet counts VS Code request entries and CLI model calls (`requests.count`) separately. Each source's date range ends at its last observed historical record. Counts are not model-weighted, converted to money, or added to AI credits. When the original count cannot be recovered, previously exported values are shown for audit only. Repeated CLI shutdown summaries for the same session start and unit are not added together.

### Sidebar filters

Filter by machine, project, or date range from the left, and the report updates live — no setup required. Date shortcuts cover the last 7 days, 30 days, 3 months, or all data (the default), with custom start/end dates still available.

![Sidebar filters](design/dashboard-sidebar.jpg)

### Optional cost estimates

Cost display is off by default. Turn it on to switch usage values and charts from AI credits to estimated USD or CNY. The report uses GitHub's official rate of **1 AI credit = $0.01 USD**. The default USD→CNY rate is **6.75 (reference date: 2026-08-06)** and remains editable because currency rates change. Links to the GitHub billing dashboard, [official AI Credits documentation](https://docs.github.com/en/copilot/concepts/billing/usage-based-billing-for-individuals), and [UsagePricing](https://www.usagepricing.com/blueprint/github-copilot) are included beside the controls.

### One-click screenshot

The camera button in the top-right saves the **entire page** as a single image (a crisp JPEG, around 430 KB). Privacy masking turns on automatically while capturing, so it's safe to share.

---

## 🚀 Getting started

### 1. Install

**Windows** (via the Windows Package Manager):

```powershell
winget install gh-usage
```

Upgrade:

```powershell
winget upgrade gh-usage
```

**Linux / macOS:** download the archive for your platform from the [Releases page](https://github.com/kukisama/gh-usage/releases), unpack it, and run `gh-usage`.

### 2. Run

Run it from your terminal:

```powershell
gh-usage
```

By default it writes three files to the current directory:

- `copilot-usage-<machine>.csv` — import into Excel for deeper analysis
- `copilot-effort-<machine>.csv` — verified human submission events, including submissions with no recorded credits
- `copilot-usage-<machine>.html` — the report you open with a double-click

### 3. Open the report

Run `gh-usage --view` to generate a fresh report and open it in your default browser immediately. `gh-usage -v` is the short form; `gh-usage -view` also works when `-view` is the first argument. CSV export is retained, and the terminal exits without waiting for a keypress.

Alternatively, double-click an existing HTML file. Search, filter, switch language, toggle privacy, and save a screenshot — all from the page.

---

## 🛠️ Common commands

Generate and open the report in one step: `gh-usage --view`. Combine it with filters, for example `gh-usage --view --since-days 7`, or choose a report path with `gh-usage --view --html report.html`.

`--view` cannot be combined with `--no-html`. With `--output -`, CSV/JSON stays on stdout while HTML is saved as `copilot-usage-<machine>.html` in the current directory (or at `--html <PATH>`). Browser-launch messages go to stderr. Without `--view`, terminal and automation runs still do not open a browser. A desktop/default HTML handler is required; launch errors are reported without deleting the generated files.

VS Code and GitHub Copilot CLI records are both included by default. To scan VS Code only:

```powershell
gh-usage --no-cli-logs
```

Scan only the last week:

```powershell
gh-usage --since-days 7
```

Write to a specific location:

```powershell
gh-usage --output .\reports\copilot-usage.csv --html .\reports\copilot-usage.html
```

Export JSON for automation, skip the HTML:

```powershell
gh-usage --format json --output .\reports\copilot-usage.json --no-html
```

### Effort timeline and audit export

The HTML report includes the **Effort time** sheet when effort data is available. Drag the overview to select or move a range, or drag its handles to resize it. Effort supports **Year** (monthly totals), **Month** (calendar), **Week** (Monday–Sunday), and **Day** (48 half-hour slots), all in Beijing time (UTC+08:00). Select a day or occupied slot to inspect the underlying submissions without changing the range totals. Pass `--no-effort` to skip effort export and display. Usage CSV alone cannot reconstruct effort.

- **Month:** bar length is the share of a 24-hour day. Daily effort is green below 4 hours, yellow from 4 to under 8, orange from 8 to under 12, and red at 12 hours or more.
- **Week:** the total is green below 30 hours, yellow from 30 to under 50, and red at 50 hours or more. The half-hour cells still show the number of submissions.

Use `gh-usage --since-days 7 --effort-report effort.md` to also write a Markdown report. It reads stored local VS Code submissions and Copilot CLI user events, even without recorded credits. Across the scanned projects and sessions, any number of human submissions in the same Beijing-time clock half-hour count as one slot (0.5 effort hours). The report includes source references, exclusions, daily slots, and occupied intervals. Automatic events, unlinked queue entries, cloud execution, and response time do not count. It does not infer business relevance or recover unavailable history. Use a new path to preserve earlier reports.

Repeat `--effort-project <NAME>` to combine exact project names in the effort report. The calculation uses the union of their slots, and the credits export is unchanged. A project filter does not classify messages within a mixed-topic session.

### Rebuild a report offline

The HTML report defaults to Classic. Choose Rhythm in the top-right theme control to switch layouts. **Device atlas** places filters and the range total above a grid of machine rows with bars on a shared scale. The detail panel opens on the latest period with records; select a date or a machine's bar to change the preview without changing global filters or totals. **Trend analysis** puts machine and project filters on the left, per-machine trends in the center, and the breakdown on the right. Drag sideways when the chart does not fit. The themes share the same data, filters, effort and historical request sheets. To build without the optional theme, run `cargo build --release --no-default-features`.

Run `gh-usage --no-html` to export without rendering. Keep `copilot-usage-<machine>.csv`, `copilot-effort-<machine>.csv` (if exported), and `copilot-usage-<machine>.export.json` together. The small JSON receipt identifies the machine and verifies that the CSVs match the export. Run `gh-usage --from-csv copilot-usage-<machine>.csv` to rebuild offline without rescanning logs. For a custom `report.csv`, the companion files are `report.effort.csv` and `report.export.json`. `--html <PATH>` sets the report output. JSON and stdout usage exports do not write companion files.

### Merge reports from multiple machines

Run `gh-usage --no-html` without `--output` on each machine, and give the machines distinct names (override with `--hostname` if needed). Collect each machine's `copilot-usage-<machine>.csv`, `copilot-effort-<machine>.csv` (if available), and `copilot-usage-<machine>.export.json` in one folder. To refresh a machine's data, replace that set with the latest export you want to report:

```powershell
gh-usage --merge .\shared\copilot-usage
```

The merge uses one standard-named snapshot per machine; it never combines older snapshots for the same machine or picks one using modification times. Use `--from-csv` to view custom-named archives separately. For older usage-only CSVs without a recorded hostname, the filename identifies the machine: keep the original filename and store renamed archives outside the merge folder. Effort CSVs without a matching receipt are not imported; re-export them or pass `--no-effort` for a usage-only report. AI credits and historical requests remain separate; different machines' records are preserved.

On the effort sheet, each Beijing-time half-hour slot counts once across the selected machines. Missing effort data is not counted as zero, and the sheet is omitted when no effort CSV is imported. A present but mismatched companion is an error, not partial coverage.

Reports generated directly from local logs can open source files and lines in VS Code. Reports rebuilt with `--from-csv` or `--merge` show source labels without open or copy-path actions. This is not path redaction: the exports can still contain source paths.

Add `--view` to open the merged report immediately: `gh-usage --merge .\shared\copilot-usage --view`. Use `--html <PATH>` to override the merged HTML destination; relative paths are resolved from the current working directory.

---

## 📄 CSV fields

Each row is one source usage record. Alongside machine, time, session, model, and source file/line, `credits` holds AI credits, `request_count` holds original historical counts when available, and `unclassified_usage` preserves values for audit only. Do not add these fields together; compare historical counts within the same source. The CSV includes a UTF-8 BOM by default so Windows Excel opens it cleanly (use `--no-bom` to turn it off).

The separate effort CSV has one row per verified human submission: `hostname`, `timestamp_ms` (submission time, UTC Unix milliseconds), `source`, optional `project`, `session_id`, and `event_id` when available. `file` and `line` retain the original source location for audit; the CSV does not contain chat text. Review machine names, project names and file paths before sharing. Offline reconstruction does not need the referenced files.

---

## ⚠️ Good to know

`gh-usage` is built for **local analysis and review** — great for spotting trends and rough comparisons, but **not a replacement for GitHub's official billing or usage reports**.

- It only reads files that exist locally; deleted history can't be recovered.
- Turns without any valid cumulative credits or legacy credit details are skipped; unknown usage is not assumed to be free. Active turns can grow after a report is generated.
- USD/CNY values are estimates for local analysis. Exchange rates are user-configurable, and GitHub's billing page remains authoritative.
- Copilot CLI event JSONL is the primary trace. The session title comes from `workspace.yaml.name` (or the first real JSONL user message when unnamed), and `workspace.yaml.cwd` identifies the project. `session-store.db` contributes exact credits, duration, and turn metadata only when its usage row matches a JSONL response or a `session.usage_record` event. Unmatched rows are reported in the terminal and not counted.
- It uses your system's standard VS Code and Copilot CLI data directories by default, and supports custom paths.

---

## 📚 Options

```text
--no-cli-logs            Skip GitHub Copilot CLI records
--since-days <N>         Only scan files modified within the last N days
--output <PATH>          Write CSV or JSON to a specific path
--html <PATH>            Write the HTML report to a specific path
--view, -v               Generate HTML and open it in the default browser
--no-html                Do not generate the HTML report
--effort-report <PATH>   Also write a Markdown report of local human effort slots
--effort-project <NAME>  Limit the Markdown effort report to exact project names (repeatable)
--no-effort              Skip effort CSV export and omit the Effort time sheet
--from-csv <CSV>         Rebuild HTML from one usage CSV and its companion
--merge [DIR]            Merge usage and effort CSVs into one offline report
--format csv|json        Choose the output format
--hostname <NAME>        Override the machine name stored in records
```

Run `gh-usage --help` for the full command reference.

---

## 📜 License

Released under the [MIT License](LICENSE) — free to use, modify, and distribute.

## 🤝 Contributing

Issues and pull requests are welcome. If this tool helps you out, a ⭐ Star is always appreciated.

---

<sub>Generated locally by gh-usage · your data stays on your machine.</sub>
