# gh-usage · See your Copilot usage at a glance

English | [简体中文](design/README.zh-CN.md)

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Built with Rust](https://img.shields.io/badge/Built%20with-Rust-orange.svg?logo=rust)](https://www.rust-lang.org/)
[![Platform](https://img.shields.io/badge/Platform-Windows%20%7C%20Linux%20%7C%20macOS-lightgrey.svg)](#-getting-started)
[![Releases](https://img.shields.io/badge/Releases-download-brightgreen.svg)](https://github.com/kukisama/gh-usage/releases)

> One command turns your scattered local GitHub Copilot usage records into a clean, visual report.
> Nothing is uploaded, nothing phones home — your data stays on your machine.

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

![Records table with privacy mask on](design/dashboard-records.jpg)

### Sidebar filters

Filter by machine, project, or date range from the left, and the report updates live — no setup required.

![Sidebar filters](design/dashboard-sidebar.jpg)

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

It writes two files to the current directory:

- `copilot-usage-<machine>.csv` — import into Excel for deeper analysis
- `copilot-usage-<machine>.html` — the report you open with a double-click

### 3. Open the report

Double-click the HTML file. Search, filter, switch language, toggle privacy, and save a screenshot — all from the page.

---

## 🛠️ Common commands

Include GitHub Copilot CLI records:

```powershell
gh-usage --include-cli-logs
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

### Merge reports from multiple machines

Run `gh-usage` on each machine, collect the `copilot-usage-*.csv` files into one folder, then:

```powershell
gh-usage --merge .\shared\copilot-usage
```

It reads every CSV, deduplicates records, and produces one combined report with a per-machine breakdown — handy for swapping machines, team reviews, or comparing your desktop against your laptop.

---

## 📄 CSV fields

Each row is one usage record. Common fields include machine name, local time, session title, source, model, credits spent, the raw credit details, and the source file and line number. The CSV includes a UTF-8 BOM by default so Windows Excel opens it cleanly (use `--no-bom` to turn it off).

---

## ⚠️ Good to know

`gh-usage` is built for **local analysis and review** — great for spotting trends and rough comparisons, but **not a replacement for GitHub's official billing or usage reports**.

- It only reads files that exist locally; deleted history can't be recovered.
- Records without credit details are skipped.
- It uses your system's standard VS Code data directory by default, and supports custom paths.

---

## 📚 Options

```text
--include-cli-logs       Include GitHub Copilot CLI records
--since-days <N>         Only scan files modified within the last N days
--output <PATH>          Write CSV or JSON to a specific path
--html <PATH>            Write the HTML report to a specific path
--no-html                Do not generate the HTML report
--merge [DIR]            Merge existing copilot-usage-*.csv files into one report
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
