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
6. Before submission, checks the active account's `winget-pkgs` fork and safely fast-forwards its `master` branch when it is behind. The script attempts to create a missing fork; see [fork lookup failures](#the-personal-winget-fork-cannot-be-found) for the manual fallback.
7. Runs `wingetcreate token --store` for WingetCreate's separate GitHub OAuth cache.
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
- If GitHub CLI is not authenticated, the script runs `gh auth login --web`. A valid login can still belong to the wrong account: [verify your release account](#check-the-release-account-and-authorize-both-tools) before proceeding.
- During `-Submit`, `wingetcreate token --store` may open WingetCreate's GitHub OAuth flow. Use the same GitHub account as `gh`; the script does not compare the identities behind the two caches.
- If GitHub requires organization SAML SSO, the script can open a verified `https://github.com` authorization URL after a repository-access or WingetCreate submission error. Follow any official authorization prompt and rerun the script with the same GitHub account.
- Users who are not subject to organization SSO see no additional prompt or browser page; SSO handling is failure-driven rather than an employee-specific mode.
- The script itself does not contain a token, API key, password, or other secret.
- Do not pass a GitHub token on the command line unless you fully understand the logging risk.
- Complete authentication only in the browser/device flow opened by the tools; never paste a token into this script.
- Direct write access to `microsoft/winget-pkgs` is not required. Submission uses a writable personal fork plus permission to open a PR against the public upstream repository.
- Reuse the existing release account's fork. A fork that is merely behind is fast-forwarded automatically without `--force`.
- If the fork's `master` is `ahead` or `diverged`, the script stops instead of discarding personal commits. Restore that branch manually before retrying.
- Missing tools are installed with `winget install --id <PackageId> --exact --source winget`.
- During `-Submit`, an older-than-supported `wingetcreate` is upgraded with `winget upgrade --id Microsoft.WingetCreate --exact --source winget`.
- If the script installs or upgrades any tools, it exits on purpose. Restart the console before running it again so PATH updates and app execution aliases are available.
- The only destructive local operation is cleaning the generated work directory for the selected version:
  - `target/winget/<version>/`
- The script stops before calling `wingetcreate submit` if it finds an open PR with the expected title. Third-party PRs may use a different title; see [duplicate PR protection](#duplicate-pr-protection).

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

The target GitHub Release must already exist and include the expected Windows zip and checksum file.

### Check the release account and authorize both tools

Use the same GitHub account in GitHub CLI and WingetCreate. For an update, reuse the account that owns the existing WinGet fork. If more than one account is configured in GitHub CLI, select the intended account without signing out the others:

```powershell
gh auth switch --hostname github.com
```

If the account is not configured, use the browser login. The script also starts this login if `gh` authentication fails, including during `-Validate`:

```powershell
gh auth login --hostname github.com --git-protocol https --web
```

If switching accounts has no effect, check whether `GH_TOKEN` or `GITHUB_TOKEN` is overriding the stored login, without printing its value.

WingetCreate has a separate cache. If you switched GitHub accounts or are unsure which account WingetCreate last used, clear only its local cache and reauthorize it before submission:

```powershell
wingetcreate token --clear
wingetcreate token --store
```

Select the same GitHub account in this browser/device flow. GitHub may then ask for Microsoft organization SAML SSO. Complete that extra authorization if prompted; it does not require switching the GitHub identity used for the PR. Do not paste tokens or device codes into chat or scripts.

## Quick start

For a normal update after the GitHub Release has completed, the core flow is:

1. Confirm the GitHub Release exists and includes the Windows zip plus checksum file.
2. Verify the active GitHub release account and, if it changed, reauthorize WingetCreate with the same account.
3. Generate and validate the winget manifest.
4. Submit the validated manifest to `microsoft/winget-pkgs`.

For example, after release `v1.2.0` is available:

```powershell
.\scripts\publish-winget.ps1 -Version 1.2.0 -Validate
```

`-Validate` does not check or sync the personal fork, authenticate WingetCreate, or create a PR. It can still install missing tools or prompt for GitHub CLI authentication. Stop after validation if you are not ready to submit; `-Submit` is not a dry run.

When ready, submit:

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

1. Confirm the GitHub Release exists and [check the release account](#check-the-release-account-and-authorize-both-tools).
2. If the validation command installs missing prerequisites, restart the console with PowerShell 7 and rerun it.
3. Run:

   ```powershell
   .\scripts\publish-winget.ps1 -Version 1.0.0 -Validate
   ```

4. Confirm validation succeeds.
5. Run:

   ```powershell
   .\scripts\publish-winget.ps1 -Version 1.0.0 -Submit
   ```

6. Complete any browser login or organization SSO prompt using the same GitHub release identity for both tools.
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

If such a PR already exists, the script stops before submitting. The guard matches the full title, so also search for the package and version without relying on exact wording if someone else might have submitted it with a prefix such as `New version:`.

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

Follow [the account and authorization checks](#check-the-release-account-and-authorize-both-tools). A successful login to either tool does not authorize the other. If GitHub reports `X-GitHub-SSO: required` or asks for organization authorization, complete the relevant authorization in the browser and rerun the same command. A missing fork alone is not evidence that SSO is required.

### The personal winget fork cannot be found

If the fork lookup fails, first check that both tools are using the intended GitHub account. For a repeat release, switch back to the established account and reuse its fork; do not create a new fork under the account that happened to be active.

If this is genuinely a first-time submission with no fork under the intended account, create it from `microsoft/winget-pkgs` in the GitHub web UI, or run:

```powershell
gh repo fork microsoft/winget-pkgs --clone=false --default-branch-only
```

Do not add `--remote=false` to that command: current versions of `gh` reject `--remote` when an explicit repository is provided. The script's current automatic path can stop at the missing-repository lookup or hit that flag error. Verify the fork exists under the intended account before retrying; if creation is denied, check the account's permissions rather than bypassing the checks.

### The personal winget fork is stale

Before submission, the script compares `<authenticated-user>/winget-pkgs:master` with `microsoft/winget-pkgs:master`. A fork that is only behind is normal and is fast-forwarded automatically. If the fork is ahead, diverged, unrelated, or not writable, submission stops with a targeted error; the script never uses `--force` because that could discard personal commits.

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
4. If `wingetbot` or a maintainer requests changes, address them in the existing PR.

### Fix PR text without resubmitting

For a typo, formatting problem, or garbled text in an open PR, use **Edit** on the PR description in GitHub. The author can edit it before approval. A description-only edit does not change the manifest files or commit, so do not rerun `-Submit`. If the manifest files need changes, push a commit to the existing PR's head branch instead.

When using `gh` from PowerShell, UTF-8 output can be misread under a different Windows code page. A read–edit–write round trip can then save garbled text back to GitHub, even when only an English sentence was added. If the text looks wrong only in the terminal, check the rendered PR before changing it.

- Before capturing `gh` output, temporarily set `[Console]::OutputEncoding` to `[System.Text.UTF8Encoding]::new($false)` and restore the original value afterward.
- Preserve the full PR description, including the official template and any bot-added text. Save the corrected body as UTF-8 and use `gh pr edit <number> --repo microsoft/winget-pkgs --body-file <path>` rather than piping the body through the terminal. If sending JSON to the API instead, use `ConvertTo-Json -EscapeHandling EscapeNonAscii` to keep non-ASCII characters intact in transit.
- Refresh the original PR after saving. Check that the rendered headings and text are correct and that the commit and files changed are unchanged for a description-only edit.

