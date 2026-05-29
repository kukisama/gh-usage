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
