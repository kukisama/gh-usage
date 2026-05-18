param(
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^v?\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$')]
    [string]$Version,

    [string]$PackageIdentifier = 'Kukisama.gh-usage',
    [string]$PackageName = 'gh-usage',
    [string]$Publisher = 'kukisama',
    [string]$Repository = 'kukisama/gh-usage',
    [string]$DefaultLocale = 'en-US',
    [string]$ManifestVersion = '1.10.0',
    [string]$OutputRoot = '.\target\winget',

    [switch]$InstallWingetCreate,
    [switch]$Validate,
    [switch]$Submit
)

$ErrorActionPreference = 'Stop'

function Assert-Command {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,
        [string]$InstallHint
    )

    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        if ($InstallHint) {
            throw "$Name was not found. $InstallHint"
        }

        throw "$Name was not found."
    }
}

function ConvertTo-ReleaseVersion {
    param([Parameter(Mandatory = $true)][string]$Value)
    return $Value.Trim().TrimStart('v')
}

function ConvertTo-ReleaseTag {
    param([Parameter(Mandatory = $true)][string]$Value)
    $clean = ConvertTo-ReleaseVersion $Value
    return "v$clean"
}

function Get-JsonFromGh {
    param([Parameter(Mandatory = $true)][string[]]$Arguments)

    $output = & gh @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "gh command failed: gh $($Arguments -join ' ')"
    }

    return $output | ConvertFrom-Json
}

function Write-Utf8NoBomFile {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Content
    )

    $utf8NoBom = [System.Text.UTF8Encoding]::new($false)
    [System.IO.File]::WriteAllText((Resolve-Path -LiteralPath (Split-Path -Parent $Path)).Path + [System.IO.Path]::DirectorySeparatorChar + (Split-Path -Leaf $Path), $Content, $utf8NoBom)
}

function New-DirectoryClean {
    param([Parameter(Mandatory = $true)][string]$Path)

    if (Test-Path $Path) {
        Remove-Item $Path -Recurse -Force
    }

    New-Item -ItemType Directory -Force -Path $Path | Out-Null
}

function Get-ReleaseAsset {
    param(
        [Parameter(Mandatory = $true)]$Release,
        [Parameter(Mandatory = $true)][string]$Pattern
    )

    $candidateAssets = @($Release.assets | Where-Object { $_.name -like $Pattern })
    if ($candidateAssets.Count -eq 0) {
        throw "No release asset matched pattern '$Pattern'."
    }
    if ($candidateAssets.Count -gt 1) {
        $names = ($candidateAssets | ForEach-Object name) -join ', '
        throw "More than one release asset matched pattern '$Pattern': $names"
    }

    return $candidateAssets[0]
}

function Read-ChecksumForAsset {
    param(
        [Parameter(Mandatory = $true)][string]$ChecksumsPath,
        [Parameter(Mandatory = $true)][string]$AssetName
    )

    $escapedName = [regex]::Escape($AssetName)
    $line = Get-Content $ChecksumsPath | Where-Object { $_ -match "^([a-fA-F0-9]{64})\s+$escapedName$" } | Select-Object -First 1
    if (-not $line) {
        throw "Could not find SHA256 entry for $AssetName in $ChecksumsPath."
    }

    return ([regex]::Match($line, '^([a-fA-F0-9]{64})')).Groups[1].Value.ToUpperInvariant()
}

function Test-ZipContainsExe {
    param(
        [Parameter(Mandatory = $true)][string]$ZipPath,
        [Parameter(Mandatory = $true)][string]$ExeName
    )

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [System.IO.Compression.ZipFile]::OpenRead((Resolve-Path $ZipPath).Path)
    try {
        $entry = $zip.Entries | Where-Object { $_.FullName -replace '/', '\' -eq $ExeName } | Select-Object -First 1
        if (-not $entry) {
            $entries = ($zip.Entries | ForEach-Object FullName) -join ', '
            throw "$ZipPath does not contain $ExeName at the zip root. Entries: $entries"
        }
    }
    finally {
        $zip.Dispose()
    }
}

function Write-WingetManifestFiles {
    param(
        [Parameter(Mandatory = $true)][string]$ManifestDir,
        [Parameter(Mandatory = $true)][string]$PackageIdentifier,
        [Parameter(Mandatory = $true)][string]$PackageVersion,
        [Parameter(Mandatory = $true)][string]$PackageName,
        [Parameter(Mandatory = $true)][string]$Publisher,
        [Parameter(Mandatory = $true)][string]$DefaultLocale,
        [Parameter(Mandatory = $true)][string]$InstallerUrl,
        [Parameter(Mandatory = $true)][string]$InstallerSha256,
        [Parameter(Mandatory = $true)][string]$ManifestVersion
    )

    New-Item -ItemType Directory -Force -Path $ManifestDir | Out-Null

    $versionManifest = @"
# yaml-language-server: `$schema=https://aka.ms/winget-manifest.version.$ManifestVersion.schema.json
# Created with scripts/publish-winget.ps1
PackageIdentifier: $PackageIdentifier
PackageVersion: $PackageVersion
DefaultLocale: $DefaultLocale
ManifestType: version
ManifestVersion: $ManifestVersion
"@

    $installerManifest = @"
# yaml-language-server: `$schema=https://aka.ms/winget-manifest.installer.$ManifestVersion.schema.json
# Created with scripts/publish-winget.ps1
PackageIdentifier: $PackageIdentifier
PackageVersion: $PackageVersion
InstallerType: zip
NestedInstallerType: portable
NestedInstallerFiles:
- RelativeFilePath: gh-usage.exe
  PortableCommandAlias: gh-usage
Installers:
- Architecture: x64
  InstallerUrl: $InstallerUrl
  InstallerSha256: $InstallerSha256
ManifestType: installer
ManifestVersion: $ManifestVersion
"@

    $localeManifest = @"
# yaml-language-server: `$schema=https://aka.ms/winget-manifest.defaultLocale.$ManifestVersion.schema.json
# Created with scripts/publish-winget.ps1
PackageIdentifier: $PackageIdentifier
PackageVersion: $PackageVersion
PackageLocale: $DefaultLocale
Publisher: $Publisher
PackageName: $PackageName
PackageUrl: https://github.com/$Repository
License: MIT
LicenseUrl: https://github.com/$Repository/blob/master/LICENSE
ShortDescription: Fast local GitHub Copilot usage reports from VS Code and Copilot CLI records.
Description: A fast Rust CLI that scans local GitHub Copilot usage records from VS Code and Copilot CLI, summarizes credit usage, and exports detailed CSV or JSON reports.
Moniker: gh-usage
Tags:
- rust
- cli
- github-copilot
- vscode
- usage-tracking
- csv
- json
ReleaseNotesUrl: https://github.com/$Repository/releases/tag/v$PackageVersion
ManifestType: defaultLocale
ManifestVersion: $ManifestVersion
"@

    Write-Utf8NoBomFile -Path (Join-Path $ManifestDir "$PackageIdentifier.yaml") -Content $versionManifest
    Write-Utf8NoBomFile -Path (Join-Path $ManifestDir "$PackageIdentifier.installer.yaml") -Content $installerManifest
    Write-Utf8NoBomFile -Path (Join-Path $ManifestDir "$PackageIdentifier.locale.$DefaultLocale.yaml") -Content $localeManifest
}

$releaseVersion = ConvertTo-ReleaseVersion $Version
$releaseTag = ConvertTo-ReleaseTag $Version
$assetName = "gh-usage-$releaseTag-windows-x64.zip"

$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot

Assert-Command -Name gh -InstallHint 'Install GitHub CLI from https://cli.github.com/ and run gh auth login.'

Write-Host "Checking GitHub authentication..." -ForegroundColor Cyan
& gh auth status *> $null
if ($LASTEXITCODE -ne 0) {
    throw 'GitHub CLI is not authenticated. Run: gh auth login'
}

if ($InstallWingetCreate -and -not (Get-Command wingetcreate -ErrorAction SilentlyContinue)) {
    Assert-Command -Name winget -InstallHint 'Install winget first, then rerun with -InstallWingetCreate.'
    Write-Host "Installing Microsoft.WingetCreate..." -ForegroundColor Cyan
    winget install Microsoft.WingetCreate --disable-interactivity --accept-source-agreements --accept-package-agreements
}

Write-Host "Reading GitHub release $releaseTag..." -ForegroundColor Cyan
$release = Get-JsonFromGh -Arguments @('release', 'view', $releaseTag, '--repo', $Repository, '--json', 'tagName,assets,url')
$windowsInstallerAsset = Get-ReleaseAsset -Release $release -Pattern $assetName
$checksumsAsset = Get-ReleaseAsset -Release $release -Pattern "gh-usage-$releaseTag-checksums.txt"
$assetUrl = $windowsInstallerAsset.url

$outputRootPath = if ([System.IO.Path]::IsPathRooted($OutputRoot)) { $OutputRoot } else { Join-Path $repoRoot $OutputRoot }
$workRoot = Join-Path $outputRootPath $releaseVersion
New-DirectoryClean $workRoot

Write-Host "Downloading release assets..." -ForegroundColor Cyan
& gh release download $releaseTag --repo $Repository --pattern $assetName --pattern $checksumsAsset.name --dir $workRoot --clobber
if ($LASTEXITCODE -ne 0) {
    throw "Failed to download release assets for $releaseTag."
}

$zipPath = Join-Path $workRoot $assetName
$checksumsPath = Join-Path $workRoot $checksumsAsset.name
$installerSha256 = Read-ChecksumForAsset -ChecksumsPath $checksumsPath -AssetName $assetName

Test-ZipContainsExe -ZipPath $zipPath -ExeName 'gh-usage.exe'

$manifestDir = Join-Path $workRoot "manifests\k\Kukisama\gh-usage\$releaseVersion"
Write-WingetManifestFiles `
    -ManifestDir $manifestDir `
    -PackageIdentifier $PackageIdentifier `
    -PackageVersion $releaseVersion `
    -PackageName $PackageName `
    -Publisher $Publisher `
    -DefaultLocale $DefaultLocale `
    -InstallerUrl $assetUrl `
    -InstallerSha256 $installerSha256 `
    -ManifestVersion $ManifestVersion

Write-Host "Prepared winget manifest:" -ForegroundColor Green
Write-Host $manifestDir
Write-Host ""
Write-Host "InstallerUrl: $assetUrl"
Write-Host "InstallerSha256: $installerSha256"
Write-Host ""

if ($Validate -or $Submit) {
    Assert-Command -Name winget -InstallHint 'Install winget from https://learn.microsoft.com/windows/package-manager/winget/.'

    Write-Host "Validating manifest with winget..." -ForegroundColor Cyan
    winget validate $manifestDir
    if ($LASTEXITCODE -ne 0) {
        throw 'winget manifest validation failed.'
    }
}

if ($Submit) {
    Assert-Command -Name wingetcreate -InstallHint 'Install it with: winget install Microsoft.WingetCreate'

    Write-Host "Submitting manifest with wingetcreate..." -ForegroundColor Cyan
    wingetcreate submit $manifestDir
    if ($LASTEXITCODE -ne 0) {
        throw 'wingetcreate submit failed.'
    }
} else {
    Write-Host "Next steps:" -ForegroundColor Yellow
    Write-Host "  1. Install wingetcreate if needed: winget install Microsoft.WingetCreate"
    Write-Host "  2. Validate: .\scripts\publish-winget.ps1 -Version $releaseVersion -Validate"
    Write-Host "  3. Submit:   .\scripts\publish-winget.ps1 -Version $releaseVersion -Submit"
}
