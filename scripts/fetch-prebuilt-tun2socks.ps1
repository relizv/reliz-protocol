#!/usr/bin/env pwsh
# Fetch prebuilt libhev-socks5-tunnel.so for Android from GitHub releases.
# Alternative to building from source.
#
# If hev-socks5-tunnel does not provide prebuilt Android libs,
# this script falls back to downloading from mirrors (shadowsocks-android, etc.)

param(
    [string]$OutputDir = "$PSScriptRoot\..\ghost_flutter\android\app\src\main\jniLibs"
)

$ErrorActionPreference = "Stop"

# Try hev-socks5-tunnel releases first
$Repo = "heiher/hev-socks5-tunnel"
$ApiUrl = "https://api.github.com/repos/$Repo/releases/latest"

Write-Host "Fetching latest release info from $Repo..."
try {
    $Release = Invoke-RestMethod -Uri $ApiUrl -Headers @{ "User-Agent" = "reliz-build" } -TimeoutSec 30
} catch {
    Write-Warning "Failed to fetch GitHub release info: $_"
    $Release = $null
}

# Search for Android .so assets in the release
$AndroidAssets = @()
if ($Release) {
    $AndroidAssets = $Release.assets | Where-Object {
        $_.name -match '(android|aarch64|arm64|armv7|x86_64).*\.(so|zip|tar\.gz)'
    }
}

if ($AndroidAssets.Count -eq 0) {
    Write-Host "No prebuilt Android libraries found in $Repo releases." -ForegroundColor Yellow
    Write-Host ""
    Write-Host "You have two options:" -ForegroundColor Cyan
    Write-Host "  1. Build from source:  bash scripts/build-tun2socks-android.sh" -ForegroundColor White
    Write-Host "  2. Download manually from another source:" -ForegroundColor White
    Write-Host "     - shadowsocks-android releases" -ForegroundColor White
    Write-Host "     - termux packages (pkg install tun2socks)" -ForegroundColor White
    Write-Host ""
    Write-Host "After obtaining .so files, place them in:" -ForegroundColor Cyan
    Write-Host "  ghost_flutter/android/app/src/main/jniLibs/<abi>/libhev-socks5-tunnel.so" -ForegroundColor White
    Write-Host ""
    Write-Host "Required ABIs: arm64-v8a, armeabi-v7a, x86_64" -ForegroundColor White
    exit 1
}

Write-Host "Found $($AndroidAssets.Count) potential asset(s). Downloading..."

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

foreach ($Asset in $AndroidAssets) {
    $FileName = $Asset.name
    $OutFile = Join-Path $OutputDir $FileName
    Write-Host "  Downloading $FileName ..."
    Invoke-WebRequest -Uri $Asset.browser_download_url -OutFile $OutFile -UseBasicParsing
    Write-Host "  Saved to $OutFile"

    # If it's a zip or tar.gz, extract it
    if ($FileName -match '\.zip$') {
        Expand-Archive -Path $OutFile -DestinationPath $OutputDir -Force
        Remove-Item $OutFile
    } elseif ($FileName -match '\.tar\.gz$') {
        tar -xzf $OutFile -C $OutputDir
        Remove-Item $OutFile
    }
}

Write-Host ""
Write-Host "Download complete. Check the following paths:" -ForegroundColor Green
Get-ChildItem -Path $OutputDir -Recurse -Filter "*.so" | ForEach-Object {
    Write-Host "  $($_.FullName)"
}
