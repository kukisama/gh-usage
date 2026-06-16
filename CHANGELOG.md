# Changelog

English | [简体中文](design/CHANGELOG.zh-CN.md)

All notable changes to **gh-usage** are documented here, written from your point of view — what's new, what got better, and why it matters.

This project follows [Semantic Versioning](https://semver.org/).

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

[Unreleased]: https://github.com/kukisama/gh-usage/compare/v1.2.1...HEAD
[1.2.1]: https://github.com/kukisama/gh-usage/releases/tag/v1.2.1
[1.2.0]: https://github.com/kukisama/gh-usage/releases/tag/v1.2.0
[1.1.0]: https://github.com/kukisama/gh-usage/releases/tag/v1.1.0
[1.0.0]: https://github.com/kukisama/gh-usage/releases/tag/v1.0.0
