#!/usr/bin/env pwsh
# UAPS CLI Installer for Windows
# Usage: irm https://raw.githubusercontent.com/iyulab/U-APS/main/install.ps1 | iex

$ErrorActionPreference = "Stop"

$repo = "iyulab/U-APS"
$installDir = "$env:LOCALAPPDATA\UAPS\cli"

# Detect architecture
$arch = if ([Environment]::Is64BitOperatingSystem) { "x64" } else { "x86" }
$rid = "win-$arch"
$fileName = "uaps-cli-$rid.zip"

# Get latest release
Write-Host "Fetching latest release..."
$release = Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/releases/latest"
$version = $release.tag_name
$asset = $release.assets | Where-Object { $_.name -eq $fileName }

if (-not $asset) {
    Write-Error "Could not find $fileName in release $version"
    exit 1
}

$downloadUrl = $asset.browser_download_url

# Create install directory
if (Test-Path $installDir) {
    Remove-Item -Recurse -Force $installDir
}
New-Item -ItemType Directory -Force -Path $installDir | Out-Null

# Download and extract
$tempFile = Join-Path $env:TEMP $fileName
Write-Host "Downloading UAPS CLI $version for $rid..."
Invoke-WebRequest -Uri $downloadUrl -OutFile $tempFile

Write-Host "Extracting to $installDir..."
Expand-Archive -Path $tempFile -DestinationPath $installDir -Force
Remove-Item $tempFile

# Add to PATH
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$installDir*") {
    Write-Host "Adding to PATH..."
    [Environment]::SetEnvironmentVariable("Path", "$userPath;$installDir", "User")
}

Write-Host ""
Write-Host "UAPS CLI $version installed successfully!" -ForegroundColor Green
Write-Host "Location: $installDir"
Write-Host ""
Write-Host "Restart your terminal, then run: uaps --help"
