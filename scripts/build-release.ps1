$ErrorActionPreference = 'Stop'

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$projectRoot = Split-Path -Parent $scriptDir

Set-Location $projectRoot

Write-Host "Building gh-usage release binary..." -ForegroundColor Cyan
cargo build --release -p gh-usage

$exePath = Join-Path $projectRoot 'target\release\gh-usage.exe'

if (-not (Test-Path $exePath)) {
    throw "Release binary was not found: $exePath"
}

Write-Host "Release build completed:" -ForegroundColor Green
Write-Host $exePath