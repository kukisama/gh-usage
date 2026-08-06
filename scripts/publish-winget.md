# `publish-winget.ps1`

`publish-winget.ps1` prepares and optionally submits the Windows Package Manager (`winget`) manifest for a released version of `gh-usage`.

Read this before running the script. The normal workflow is:

1. Let the script check and install missing prerequisite tools.
2. Restart the console if the script installed anything.
3. Make sure the GitHub Release already exists.
4. Generate the winget manifest locally.
5. Validate the manifest locally.
6. Submit exactly one pull request to `microsoft/winget-pkgs`.
7. Wait for winget automation and maintainers to review the PR.

## What the script does

For a version such as `1.0.0`, the script:

1. Checks required command-line tools.
2. Installs missing tools with `winget` when possible.
3. Before submission, checks that `wingetcreate` meets the known-good minimum version and upgrades it with `winget` when needed.
4. Verifies GitHub CLI authentication and opens its secure browser login flow when needed.
5. Verifies read access to the release repository and `microsoft/winget-pkgs`.
6. Before submission, creates the authenticated account's `winget-pkgs` fork when missing and safely fast-forwards its `master` branch when it is behind.
7. Verifies WingetCreate's separate GitHub OAuth cache and opens its login flow when needed.
8. Stops after installing or upgrading a tool and asks you to restart the console.
9. Requires the active shell to be PowerShell 7 before continuing.
10. Normalizes the version to `1.0.0` and the release tag to `v1.0.0`.
11. Reads the GitHub Release from `kukisama/gh-usage`.
12. Finds these release assets:
   - `gh-usage-v1.0.0-windows-x64.zip`
   - `gh-usage-v1.0.0-checksums.txt`
13. Downloads those assets into `target/winget/<version>/`.
14. Reads the SHA256 checksum for the Windows zip.
15. Verifies that the zip contains `gh-usage.exe` at the zip root.
16. Generates the winget manifest files under:
   - `target/winget/<version>/manifests/k/Kukisama/gh-usage/<version>/`
17. Optionally runs `winget validate`.
18. Optionally runs `wingetcreate submit` to create a PR in `microsoft/winget-pkgs`.

## What the script does not do

The script does **not**:

- Build the release binaries.
- Create the GitHub Release.
- Upload release assets.
- Store secrets in this repository.
- Print or write GitHub tokens.
- Modify repository source files outside its generated winget output directory. It may install or upgrade the prerequisite tools listed below through `winget`.
- Submit directly to the `gh-usage` repository.
- Force-reset an `ahead` or `diverged` personal `winget-pkgs` fork.
- Continue the release flow immediately after installing tools. It stops and asks you to restart the console first.

## Safety notes

The script is intended to be safe for normal release usage.

Important details:

- There are two independent GitHub authentication caches: GitHub CLI (`gh`) reads the release, checks permissions, and maintains the fork; `wingetcreate` uses its own OAuth token to create the submission branch and PR.
- If GitHub CLI is not authenticated, the script runs `gh auth login --web` and opens GitHub's secure browser flow.
- During `-Submit`, the script runs `wingetcreate token --store`. It uses the cached token when valid, or opens WingetCreate's GitHub OAuth flow when authentication is missing.
- If GitHub requires organization SAML SSO, the script recognizes the `X-GitHub-SSO` response header or an SSO URL printed by WingetCreate, validates that the URL uses `https://github.com`, and opens the authorization page. Complete authorization and rerun the script.
- Users who are not subject to organization SSO see no additional prompt or browser page; SSO handling is failure-driven rather than an employee-specific mode.
- The script itself does not contain a token, API key, password, or other secret.
- Do not pass a GitHub token on the command line unless you fully understand the logging risk.
- Complete authentication only in the browser/device flow opened by the tools; never paste a token into this script.
- Direct write access to `microsoft/winget-pkgs` is not required. Submission uses a writable personal fork plus permission to open a PR against the public upstream repository.
- A missing personal fork is created automatically. A fork that is merely behind is fast-forwarded automatically without `--force`.
- If the fork's `master` is `ahead` or `diverged`, the script stops instead of discarding personal commits. Restore that branch manually before retrying.
- Missing tools are installed with `winget install --id <PackageId> --exact --source winget`.
- During `-Submit`, an older-than-supported `wingetcreate` is upgraded with `winget upgrade --id Microsoft.WingetCreate --exact --source winget`.
- If the script installs or upgrades any tools, it exits on purpose. Restart the console before running it again so PATH updates and app execution aliases are available.
- The only destructive local operation is cleaning the generated work directory for the selected version:
  - `target/winget/<version>/`
- The script has a duplicate PR guard before submit. If an open PR already exists for the same package and version, it stops before calling `wingetcreate submit`.

## Prerequisites

The script checks these tools at startup:

- PowerShell 7 (`pwsh`)
- GitHub CLI (`gh`)
- Windows Package Manager (`winget`)
- Windows Package Manager Manifest Creator (`wingetcreate`)
- Git (`git`)

For submission, `wingetcreate` must be at least version `1.12.13.0` by default. This is a pinned, known-good minimum rather than a mandatory "latest version" check, so the release flow remains reproducible and does not fail merely because a newer release appears. Override `-MinimumWingetCreateVersion` only when the repository workflow has deliberately tested another minimum.

If `winget` is available, the script installs missing tools with these package IDs:

| Command | Package ID |
| --- | --- |
| `pwsh` | `Microsoft.PowerShell` |
| `git` | `Git.Git` |
| `gh` | `GitHub.cli` |
| `wingetcreate` | `Microsoft.WingetCreate` |

If `winget` itself is missing, install App Installer / Windows Package Manager first, restart the console, then rerun the script.

The script requires the active shell to be PowerShell 7. If it installs PowerShell 7, or if you are currently running Windows PowerShell instead of PowerShell 7, restart the console with `pwsh` and rerun the script.

The script checks GitHub CLI authentication automatically. You can inspect it manually with:


```powershell
gh auth status
```

If it is not authenticated, `-Submit` opens this web login automatically; you can also start it manually:

```powershell
gh auth login --hostname github.com --git-protocol https --web
```

The target GitHub Release must already exist and include the expected Windows zip and checksum file.

## Quick start

For a normal update after the GitHub Release has completed, the core flow is:

1. Confirm the GitHub Release exists and includes the Windows zip plus checksum file.
2. Generate and validate the winget manifest.
3. Submit the validated manifest to `microsoft/winget-pkgs`.

For example, after release `v1.2.0` is available:

```powershell
.\scripts\publish-winget.ps1 -Version 1.2.0 -Validate
```

Then submit:

```powershell
.\scripts\publish-winget.ps1 -Version 1.2.0 -Submit
```

The GitHub Release step happens before this script. This script only prepares, validates, and submits the winget package metadata for an already released version.

Generate the manifest only:

```powershell
.\scripts\publish-winget.ps1 -Version 1.0.0
```

Generate and validate the manifest:

```powershell
.\scripts\publish-winget.ps1 -Version 1.0.0 -Validate
```

Submit the manifest to `microsoft/winget-pkgs`:

```powershell
.\scripts\publish-winget.ps1 -Version 1.0.0 -Submit
```

After submit, `wingetcreate` should create a pull request and print its URL. At that point, do not submit the same version again unless the previous PR was closed or maintainers explicitly ask for a new PR.

## Recommended release workflow

Use this sequence for a normal winget release:

1. Run the script once. If it installs missing prerequisites, restart the console with PowerShell 7 and rerun the script.
2. Confirm the GitHub Release exists.
3. Run:

   ```powershell
   .\scripts\publish-winget.ps1 -Version 1.0.0 -Validate
   ```

4. Confirm validation succeeds.
5. Run:

   ```powershell
   .\scripts\publish-winget.ps1 -Version 1.0.0 -Submit
   ```

6. Complete either browser login flow if prompted. GitHub CLI and WingetCreate intentionally maintain separate tokens.
7. Open the PR URL printed by `wingetcreate`.
8. Wait for validation and maintainer review.
9. Respond only if `wingetbot` or maintainers request changes.

## Duplicate PR protection

The script checks for an existing open PR in `microsoft/winget-pkgs` with this title format:

```text
<PackageIdentifier> version <PackageVersion>
```

For example:

```text
Kukisama.gh-usage version 1.0.0
```

If such a PR already exists, the script stops before submitting. This prevents accidental duplicate PRs.

Only use `-ForceSubmit` after manually confirming that a duplicate submit is intentional:

```powershell
.\scripts\publish-winget.ps1 -Version 1.0.0 -Submit -ForceSubmit
```

Typical reasons to use `-ForceSubmit` are rare:

- The previous PR was closed and you intentionally need a replacement.
- The GitHub search result is stale or incorrect.
- A maintainer explicitly asked you to resubmit.

Do not use `-ForceSubmit` for normal submissions.

## Parameters

| Parameter | Default | Description |
| --- | --- | --- |
| `-Version` | Required | Release version. Accepts `1.0.0` or `v1.0.0`. |
| `-PackageIdentifier` | `Kukisama.gh-usage` | winget package identifier. |
| `-PackageName` | `gh-usage` | Package name shown in winget metadata. |
| `-Publisher` | `kukisama` | Publisher shown in winget metadata. |
| `-Repository` | `kukisama/gh-usage` | GitHub repository used to read the release and generate URLs. |
| `-DefaultLocale` | `en-US` | Default manifest locale. |
| `-ManifestVersion` | `1.10.0` | winget manifest schema version used in generated files. |
| `-MinimumWingetCreateVersion` | `1.12.13.0` | Minimum known-good `wingetcreate` version required by `-Submit`; older versions are upgraded and the script asks you to restart. |
| `-OutputRoot` | `./target/winget` | Root directory for generated winget files. |
| `-Validate` | Off | Runs `winget validate` after generating manifests. |
| `-Submit` | Off | Validates and submits the manifest with `wingetcreate submit`. |
| `-ForceSubmit` | Off | Skips the duplicate open PR guard. Use only when intentional. |

## Generated files

For `1.0.0`, the manifest directory is:

```text
target/winget/1.0.0/manifests/k/Kukisama/gh-usage/1.0.0/
```

It contains:

- `Kukisama.gh-usage.yaml`
- `Kukisama.gh-usage.installer.yaml`
- `Kukisama.gh-usage.locale.en-US.yaml`

The downloaded release assets are also stored under:

```text
target/winget/1.0.0/
```

This directory is regenerated each time the script runs for the same version.

## Common pitfalls

### The release does not exist

The script reads GitHub Release `v<version>`. Create the release first, then rerun the script.

### The asset names do not match

For `1.0.0`, the script expects:

```text
gh-usage-v1.0.0-windows-x64.zip
gh-usage-v1.0.0-checksums.txt
```

If the release workflow changes asset names, update the script or release assets accordingly.

### The checksum file is missing the Windows zip entry

The checksum file must contain a SHA256 line for the Windows zip. Otherwise the script cannot generate `InstallerSha256`.

### The zip does not contain `gh-usage.exe` at the root

The script expects `gh-usage.exe` at the zip root. If the executable is nested inside a folder, winget portable install behavior may not match the manifest.

### `winget validate` fails

Fix the generated manifest or script metadata, then rerun with `-Validate`.

### A PR already exists

If the script reports an existing open PR, do not submit again. Open the existing PR and wait for checks or maintainer feedback.

### Authentication prompts

The script first validates GitHub CLI authentication and opens `gh auth login --web` when needed. Before submission it separately runs `wingetcreate token --store`; WingetCreate may ask you to sign in to GitHub with a user code. Complete either browser flow, then return to the terminal.

GitHub CLI and WingetCreate use separate OAuth tokens, so an organization member may need to authorize each token separately for SAML SSO. When a failed GitHub request includes `X-GitHub-SSO: required; url=...`, or WingetCreate prints a GitHub organization/enterprise SSO URL, the script opens that URL and stops. Complete authorization and rerun the same command. External contributors whose accounts do not require organization SSO continue through the normal flow without seeing this step.

### The personal winget fork is stale

Before submission, the script compares `<authenticated-user>/winget-pkgs:master` with `microsoft/winget-pkgs:master`. It creates a missing fork and safely fast-forwards a behind fork. If the fork is ahead, diverged, unrelated, or not writable, submission stops with a targeted error; the script never uses `--force` because that could discard personal commits.

### A tool was installed but is still not detected

This is usually a PATH or app execution alias refresh issue. Close the current terminal, open a new PowerShell 7 terminal, then rerun the script.

### `wingetcreate` is older than the required version

When `-Submit` is used, the script compares the installed version with its pinned known-good minimum. If it is too old, the script upgrades `Microsoft.WingetCreate` and stops intentionally. Close the current terminal, open a new PowerShell 7 terminal, and rerun the same submit command. Generating or validating manifests does not require this version check because those steps do not invoke `wingetcreate`.

## After submission

When submission succeeds, `wingetcreate` prints a PR URL like:

```text
https://github.com/microsoft/winget-pkgs/pull/<number>
```

At that point:

1. Open the PR.
2. Wait for `wingetbot` validation.
3. Do not create another PR for the same version.
4. Reply or update only if the bot or maintainers request changes.

