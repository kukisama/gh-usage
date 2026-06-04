# gh-usage

`gh-usage` is a small command-line tool that scans local GitHub Copilot / Copilot Chat records and exports credit usage details.

It is intended for local inspection only. The result is based on records available on the current machine and is not a replacement for GitHub billing or official usage reports.

## Why this exists

GitHub Copilot usage data can be useful for local review, troubleshooting, and rough usage analysis. This tool helps answer basic questions such as:

- How many Copilot credits were found locally?
- Which days had usage?
- How many records were found?
- Where are the detailed records stored?

## Advantages

- Fast scanning, implemented in Rust.
- Accurate extraction from local records that contain credit details.
- CSV output by default for Excel and spreadsheet workflows.
- Optional JSON output for scripts and automation.
- Simple single-binary usage after building.
- Supports Windows, Linux, and macOS default VS Code data paths.

## Install

### Windows (winget)

The package is published to the official [winget-pkgs](https://github.com/microsoft/winget-pkgs) repository as `gh-usage`.

```powershell
winget install gh-usage

```

Upgrade to the latest version:

```powershell
winget upgrade gh-usage
```

Uninstall:

```powershell
winget uninstall gh-usage
```

After installation, the `gh-usage` command is available on `PATH`:

```powershell
gh-usage --help
```

> Note: It may take some time for a newly merged manifest to propagate to the winget source. If `winget install` reports the package was not found, run `winget source update` and try again later.

### Other platforms

Download the prebuilt archive for your OS from the [Releases page](https://github.com/kukisama/gh-usage/releases), or build from source (see below).

## Build

Build the optimized release binary:

```powershell
.\scripts\build-release.ps1
```

Or use Cargo directly on any supported platform:

```powershell
cargo build --release -p gh-usage
```

The binary is generated under:

```text
target/release/
```

## Release packaging

The repository includes a GitHub Actions release workflow at `.github/workflows/release.yml`.

To publish a new release, update the `version` field in `Cargo.toml` and push that commit to `main` or `master`. The workflow will:

1. Resolve the release tag as `v<version>`.
2. Run the test suite.
3. Build optimized executables on Windows, Linux, and macOS.
4. Create the git tag if it does not already exist.
5. Publish or update the GitHub Release assets.

Release assets include:

- `gh-usage-v<version>-windows-<arch>.zip`: Windows executable with README and license
- `gh-usage-v<version>-linux-<arch>.zip`: Linux executable with README and license
- `gh-usage-v<version>-macos-<arch>.zip`: macOS executable with README and license
- `gh-usage-v<version>-source.zip`: source archive for the released commit
- `gh-usage-v<version>-checksums.txt`: SHA256 checksums

You can also publish by pushing a tag such as `v0.1.0`, or by running the workflow manually from GitHub Actions.

## Basic usage

Run without arguments:

```powershell
.\target\release\gh-usage.exe
```

On Linux or macOS, run:

```sh
./target/release/gh-usage
```

By default, the tool:

1. Prints a usage summary.
2. Writes detailed records to `copilot-usage.csv` in the current directory.

You can also double-click the release executable on Windows. In that case, the tool keeps the console window open at the end so you can read the message before closing it.

## Output example

```text
GitHub Copilot usage summary
output=copilot-usage.csv
records=101
total_credits=16679.800
active_days=2
avg_credits_per_active_day=8339.900

daily_credits:
  2026-05-17 records=25 credits=5558.600
  2026-05-18 records=76 credits=11121.200

scan_stats:
  scanned_files=774
  scanned_lines=73660
  candidate_lines=179
  parse_errors=0

timing_ms:
  discover_ms=11
  scan_ms=2074
  reduce_ms=0
  write_ms=0
  total_ms=2087
```

## Common commands

Show help:

```powershell
.\target\release\gh-usage.exe --help
```

Write CSV to a custom path:

```powershell
.\target\release\gh-usage.exe --output .\target\gh-usage.csv --summary
```

Scan only files modified in the last 7 days:

```powershell
.\target\release\gh-usage.exe --since-days 7 --output .\target\gh-usage-last-7-days.csv --summary
```

Include GitHub Copilot CLI logs:

```powershell
.\target\release\gh-usage.exe --include-cli-logs --output .\target\gh-usage-with-cli.csv --summary
```

Export JSON:

```powershell
.\target\release\gh-usage.exe --format json --output .\target\gh-usage.json --summary
```

Measure runtime in PowerShell:

```powershell
Measure-Command { .\target\release\gh-usage.exe --output .\target\gh-usage.csv --summary }
```

## Merge CSVs from multiple machines

When you run `gh-usage` on several machines, each one writes its own
`copilot-usage-<machine>.csv`. Drop those CSVs into a single folder and
combine them into one self-contained HTML report:

```powershell
# Merge every copilot-usage-*.csv in the current directory
.\target\release\gh-usage.exe --merge

# ... or in a specific folder. The merged HTML is written next to the CSVs.
.\target\release\gh-usage.exe --merge .\shared\copilot-usage
```

Behaviour:

- Reads every `copilot-usage-*.csv` in the target directory (the merged
  output `copilot-usage-merged.csv` itself, if present, is skipped).
- Skips the local VS Code scan entirely - merge mode is purely an
  aggregator.
- Deduplicates records by `hostname + file + line + response_id + details`,
  so re-running with the same CSVs is safe.
- Writes `copilot-usage-merged.html` to the target directory. The report
  still has the host sidebar, model / source filters, and pagination, now
  driven by every machine you supplied.
- When double-clicked, the same "press any key to open the report" flow
  applies.

## CSV fields

The CSV contains one row per extracted usage record. Important fields include:

- `local_time_hint`: local timestamp when available
- `chat_title`: chat title when available
- `model`: model name parsed from the record
- `model_id`: model identifier when available
- `credits`: credits consumed by the record
- `details`: raw credit detail text
- `file`: source file scanned
- `line`: source line number

CSV files include a UTF-8 BOM by default for better compatibility with Windows Excel. Use `--no-bom` to disable it.

## Notes

- The tool scans local files only.
- By default, it looks under the standard VS Code `workspaceStorage` location for the current OS.
- Missing or deleted local history cannot be reconstructed.
- Records without credit details are ignored.
- Results are useful for local analysis and rough comparison, not official accounting.

## Changelog (business view)

Recent updates, described from a "what does this mean for me" perspective rather than the underlying implementation.

### 2026-06 · Interactive HTML report, bilingual UI, machine-aware aggregation

- **One-click interactive report.** Each run now produces a self-contained `.html` next to the CSV. Open it in any browser - no server, no internet, no plugins - to see total credits, active days, a daily stacked-bar chart, a model breakdown donut, a per-machine summary table, and a searchable record list. Safe to email or archive as a single file.
- **Cross-machine merge in one command.** Drop several machines' `copilot-usage-<host>.csv` files into a folder and run `gh-usage --merge` (or `--merge <dir>`). It combines all of them, deduplicates, and writes a single `copilot-usage-merged.html` next to the CSVs - no scanning of the local machine and no manual concatenation required.
- **English / 中文 toggle.** A language switch lives in the top-right corner of the report. Default is English; the choice is remembered in the browser for next time.
- **Filter and drill down.** The report includes a sidebar with one checkbox per machine, plus a toolbar with a free-text search box, a model dropdown, and a source dropdown. Anything you uncheck or filter is immediately removed from the charts and totals.
- **Pagination for long record lists.** The bottom record table now shows 20 rows per page (toggle to 10 / 50 / 100) with First / Prev / Next / Last controls, so even thousands of records stay readable.
- **Machine-aware output.** Every record now carries a `hostname` column, and the default file name is `copilot-usage-<machine>.csv`. Drop CSVs from several machines into one folder and merge them later without losing track of which laptop or workstation each row came from.
- **Better chat titles.** Previously some sessions showed an empty `chat_title` because VS Code stores certain chat history as a single large JSON document with the title nested inside. The scanner now finds those titles too, so the records table is much more informative.
- **Compact end-of-run summary.** Output is reorganized into a fixed-width table that fits an 80-column terminal: a 2x4 KPI grid, a daily credits section, and a files section. Less scrolling, easier to screenshot, same information.
- **Friendlier double-click flow.** When you launch the executable from Explorer, the console now explains what the CSV and HTML are for, then waits for any key to open the HTML in your default browser. Closing the window also works - both files are already on disk. Pass `--no-html` to skip the HTML and only keep the CSV.
- **Get in touch.** The HTML footer now includes a contact link and a GitHub project link, so anyone you share the report with can easily reach back or check for updates.

