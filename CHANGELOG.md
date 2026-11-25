# Changelog

English | [简体中文](design/CHANGELOG.zh-CN.md)

All notable changes to **gh-usage** are documented here, written from your point of view — what's new, what got better, and why it matters.

This project follows [Semantic Versioning](https://semver.org/).

---

## [1.2.6] — 2026-11-25

Rebuild reports from CSV without rescanning logs, and combine effort across machines without double-counting shared time slots.

### Added

- **Optional Rhythm layout.** Keep Classic or switch to a device atlas with comparable bars and a period inspector, plus a three-column trend workspace. Both layouts keep the same filters and totals.

- **Separate effort CSV.** File-based usage CSV exports include a companion file with verified human submissions, including those with no recorded credits. The effort CSV does not include chat text. Use `--no-html` to export the CSVs without generating a report.
- **Offline reconstruction and merging.** `--from-csv` rebuilds a report from one pair of CSVs without rescanning logs. `--merge` combines multiple machines and counts each occupied Beijing-time half-hour slot once across the selected machines. Older usage-only CSVs still work; AI credits and historical request counts remain separate.

### Changed

- **Source navigation follows the report mode.** Reports generated directly from local logs retain VS Code links to the source file and line. Reports generated with `--from-csv` or `--merge` display source labels without open or copy-path actions. Source coordinates remain in the exported data.
- **Separate collection from rendering.** Split the Rust command entry point, collectors, shared records, CSV merge, and HTML rendering into modules. Default file-based CSV runs now generate HTML from the exported files without changing the usage CSV schema or the counting rules.

### Fixed

- **Show every month on the usage chart.** Each calendar month in view has a label, including months with no recorded usage. When the chart does not fit, drag or swipe it sideways. Clicking a bar still opens that month's daily breakdown.
- **One standard export per machine.** Directory merges never add multiple snapshots of the same machine. Use `--from-csv` for custom-named archives. New file-based exports include a checksum receipt to bind usage and effort without changing the CSV columns; conflicting billing units or mismatched companions stop the import.
- **VS Code workspace copies do not add charges.** Matching records for one request in different source files count once; conflicting amounts are reported instead.
- **Accurate effort coverage.** Machines with empty usage exports remain listed. Missing effort CSVs are shown as not imported, not as zero effort.
- **Keep sessions distinct across machines.** The session ranking and drill-down distinguish matching session IDs on different machines. Titles found only in historical records can also appear in effort details.
- **Keep filters and dates consistent across sheets.** Usage-only filters stay with the Usage sheet. Switching between Usage and Effort keeps the selected date within the visible range, even when their log dates differ.
- **Windows portable builds no longer require a separate Visual C++ Redistributable.** The standalone executable links the C runtime statically. Release CI checks its DLL imports and runs `--version` before packaging.
- **WinGet updates preserve release metadata.** Generated manifests include the publisher page, support link, release notes, and UTC release date.

## [1.2.5] — 2026-11-23

See time spent interacting with Copilot separately from AI credits, and explore older request counts without mixing billing units.

### Added

- **Effort time has its own sheet.** Browse year, month, week, and day views and drill down to the original human submissions. Each occupied Beijing-time half hour counts once across the selected projects, regardless of how many times you submit; waiting and background execution do not count. Month bars show the share of a 24-hour day, and weekly totals turn green below 30 hours, yellow from 30 to under 50 hours, and red at 50 hours or more.
- **Optional Markdown effort report.** Use `--effort-report <PATH>` to export source references, exclusions, and daily occupied intervals. Repeat `--effort-project <NAME>` to select exact project names without changing the usage export, or use `--no-effort` to omit effort from HTML.

### Changed

- **Usage and Historical requests are separate sheets.** Usage shows AI credits only. Historical requests follows the dates actually recorded for each source and counts VS Code request entries and CLI model calls separately, without applying model multipliers or converting them to credits. Older values without a recoverable count are shown for audit only.

### Fixed

- **More accurate local usage totals.** VS Code session totals now contribute only the portion not already assigned to turns. CLI shutdown snapshots with the same session start and unit are not added together, and older request-era values are no longer treated as AI credits.
- **Multi-machine merges preserve host attribution.** Missing hostnames are recovered from the CSV that supplied each row. Duplicates from the same machine are removed without collapsing matching records from different machines. Merged usage CSVs do not contain the submission timestamps needed to calculate effort.

## [1.2.4] — 2026-09-26

More complete usage totals for large multi-agent tasks, with one-command report viewing.

### Added

- **Generate and view with one command.** `gh-usage --view` (short form `-v`, or leading `-view`) generates fresh HTML and opens it with the default handler on Windows, macOS, and Linux, without a keypress pause. Works with filters, custom HTML paths, stdout exports, and `--merge`; launch failures report the saved file path. Existing export-only behavior is unchanged.

### Fixed

- **Long-running and interrupted VS Code tasks no longer lose their recorded usage.** Replay session snapshots and deltas, including standalone `copilotCredits` updates, instead of requiring a final credit footer. Cumulative totals already include subagents and are never added to child-tool costs or repeated snapshots.
- **Preserve complete totals and precision across VS Code versions.** Reconcile the cumulative value with the final result total (allowing for footer rounding), retain larger final totals when a context-widget value is partial, and support legacy `1 credit` footers. Source links point to the selected billing value's journal line.
- **Keep billed turns removed by rewind/retry.** A stable user-request ledger preserves their cost and labels them as historical turns, rather than silently dropping already incurred usage. Unrelated tool text no longer blocks parsing of billing fields.

### Changed

- **Reports stop at user-turn granularity.** One row represents one user exchange with its recorded subtask usage included. CLI model aggregates sharing a turn are combined for display; legacy session-only totals are explicitly labelled as summaries. Tools, MCP calls, and subagents are not shown as separate billing rows.

### Fixed

- **Winget submission preflight is more reliable.** The release script now verifies GitHub CLI and WingetCreate tokens, detects organization authorization requirements only when GitHub reports them, safely opens validated GitHub authorization links, checks repository access, updates outdated tooling, and synchronizes a stale personal `winget-pkgs` fork before submission without force-resetting personal commits.

## [1.2.3] — 2026-08-06

### Added

- **Optional USD/CNY cost estimates.** Reports can convert AI credits using GitHub's official `1 credit = $0.01 USD` rate and an editable USD→CNY rate, defaulting to `6.75` with a `2026-08-06` reference date. Cost display is off by default, with direct links to GitHub billing, official documentation, and UsagePricing.
- **Quick date ranges.** Choose the last 7 days, 30 days, 3 months, or all data (the default) while retaining custom start/end dates.

### Fixed

- **Daily machine bars now follow the selected date range.** Date shortcuts and custom dates update the stacked daily chart together with the rest of the report.
- **Repeated VS Code session snapshots are no longer double-counted.** The scanner keeps the latest persisted state for each stable response ID, preventing append-style JSONL rewrites from inflating credits and turn counts; merged CSV reports continue to deduplicate matching records across machines.
- **Copilot CLI sessions now appear in reports.** The scanner uses each session's `events.jsonl` as the traceable source record, `workspace.yaml.name` as the authoritative title (falling back to the first real user message), and `workspace.yaml.cwd` for the project. Only after a SQLite usage row matches a JSONL response does `session-store.db` enrich it with precise nano-AIU credits, duration, and turn metadata.
- **Active and legacy CLI sessions are both covered.** Current usage is enriched from SQLite (including its live WAL); older sessions absent from the usage table are recovered from non-zero `session.shutdown` metrics without double counting.
- **CLI sources open at the matching event.** Every correlated record points to its real `events.jsonl` response line, and the report's Open action launches that exact line in VS Code instead of opening the SQLite database.

### Changed

- **Copilot CLI scanning is enabled by default.** A normal `gh-usage` run now combines VS Code and CLI usage automatically. Use `--no-cli-logs` when you explicitly want a VS Code-only report. The old `--include-cli-logs` flag remains accepted for compatibility.

---

## [1.2.2] — 2026-06-17

A follow-up fix that finally stops automated installers from getting stuck.

### Fixed

- **Package-validation sandboxes no longer hang.** The 1.2.1 fix relied on detecting whether the console was interactive, but inside winget's installation-verification sandbox the console still looks like a real terminal, so the app kept waiting for a keypress and timed out. The keypress pause now triggers only when you actually double-click `gh-usage.exe` from Explorer. Terminal, CI, and package-validation runs write the report files and exit cleanly. On double-click the HTML report now opens automatically — no keypress needed to view it.

---

## [1.2.1] — 2026-06-16

A small fix that keeps the app well-behaved when no one's watching.

### Fixed

- **No more hanging in non-interactive environments.** When run without a real console — in CI, through a pipe, or inside automated package-validation sandboxes — the app no longer waits for a keypress or tries to open a browser. It just writes the report files and exits cleanly. Double-click and terminal runs still get the familiar "press any key to open the report" prompt.

---

## [1.2.0] — 2026-06-14

Polishing the report into something you can confidently share with anyone.

### Added

- **One-click full-page screenshot.** A camera button in the report saves the whole page as a single, lightweight image (around 430 KB) — perfect for pasting into a chat, doc, or status update without wrestling with screen-capture tools.
- **One-click privacy mask.** A new privacy toggle blurs hostnames, project names, and session titles, so you can share results without giving away sensitive context. It switches on automatically while a screenshot is being taken, so nothing slips through by accident.

### Improved

- **Smaller download, faster updates.** The app shed a good chunk of weight (the Windows build is roughly 45% smaller), so installing and upgrading is quicker and lighter.
- **Sharper, more reliable screenshots.** Capture now relies on the browser's own rendering, so masked fields come out looking exactly as they do on screen.

---

## [1.1.0] — 2026-06-05

The release that turned raw numbers into a report you'll actually want to open.

### Added

- **Self-contained HTML report.** Every run produces a single, polished HTML file that opens in any browser — no server, no database, no internet required. It packs in headline metrics, a daily usage chart, a model breakdown, and a searchable, sortable, paginated records table.
- **Multi-language interface.** The report comes with English and Simplified Chinese built in, switchable right from the page.
- **Merge across machines.** Run `gh-usage` on several computers, drop the CSVs into one folder, and `--merge` rolls them up into a single report with a per-machine breakdown — ideal for team reviews and device migrations.

### Improved

- More accurate record extraction, plus a handful of stability fixes.

---

## [1.0.0] — 2026-05-18

The first stable release — fast, local, and to the point.

### Added

- **Local usage scanning.** Point `gh-usage` at your machine and it pulls your GitHub Copilot usage records straight from the files already sitting on disk — nothing is uploaded.
- **Spreadsheet-ready exports.** Results go to CSV (with a UTF-8 BOM so Excel opens them cleanly) or JSON for automation.
- **At-a-glance terminal summary.** Each run prints a compact summary — total credits, active days, record count, and daily averages — right in your terminal.
- **Easy install on Windows.** Available through the Windows Package Manager: `winget install gh-usage`.

---

[1.2.6]: https://github.com/kukisama/gh-usage/compare/v1.2.5...v1.2.6
[1.2.5]: https://github.com/kukisama/gh-usage/compare/v1.2.4...v1.2.5
[1.2.4]: https://github.com/kukisama/gh-usage/compare/v1.2.3...v1.2.4
[1.2.3]: https://github.com/kukisama/gh-usage/compare/v1.2.2...v1.2.3
[1.2.2]: https://github.com/kukisama/gh-usage/releases/tag/v1.2.2
[1.2.1]: https://github.com/kukisama/gh-usage/releases/tag/v1.2.1
[1.2.0]: https://github.com/kukisama/gh-usage/releases/tag/v1.2.0
[1.1.0]: https://github.com/kukisama/gh-usage/releases/tag/v1.1.0
[1.0.0]: https://github.com/kukisama/gh-usage/releases/tag/v1.0.0
