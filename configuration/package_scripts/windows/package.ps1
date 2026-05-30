<#
.SYNOPSIS
    Build the thin-edge.io Windows MSIX package.

.DESCRIPTION
    Stages package contents, substitutes version/publisher into the manifest,
    and calls makeappx.exe to produce a .msix file.

    Requires the Windows SDK (makeappx.exe) to be installed, which is available
    on GitHub Actions windows-latest runners at the standard SDK path.

.PARAMETER Version
    Four-part package version (e.g. "1.5.0.0"). Defaults to GIT_SEMVER env var
    with a ".0" appended if it is a three-part semver.

.PARAMETER Publisher
    AppxManifest publisher identity. Defaults to "CN=thin-edge.io".

.PARAMETER TedgeExe
    Path to the compiled tedge.exe binary. Defaults to
    target\release\tedge.exe relative to the repo root.

.PARAMETER OutputDir
    Directory where the .msix is written. Defaults to target\packages.
#>
param(
    [string]$Version    = $env:GIT_SEMVER,
    [string]$Publisher  = $(if ($env:MSIX_PUBLISHER) { $env:MSIX_PUBLISHER } else { "CN=thin-edge.io" }),
    [string]$TedgeExe   = "target\release\tedge.exe",
    [string]$OutputDir  = "target\packages"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# Normalise semver to four-part version required by MSIX (e.g. 1.5.0 -> 1.5.0.0)
if ($Version -match '^\d+\.\d+\.\d+$') {
    $Version = "$Version.0"
}
if (-not ($Version -match '^\d+\.\d+\.\d+\.\d+$')) {
    Write-Error "VERSION must be a four-part dotted version (got: '$Version'). Set GIT_SEMVER or pass -Version."
}

# Locate makeappx.exe from the Windows SDK
$MakeAppx = Get-ChildItem `
    "C:\Program Files (x86)\Windows Kits\10\bin\*\x64\makeappx.exe" `
    -ErrorAction SilentlyContinue |
    Sort-Object FullName -Descending |
    Select-Object -First 1 -ExpandProperty FullName

if (-not $MakeAppx) {
    Write-Error "makeappx.exe not found. Install the Windows SDK (Windows 10 SDK, build tools)."
}

Write-Host "Using makeappx: $MakeAppx"
Write-Host "Version:        $Version"
Write-Host "Publisher:      $Publisher"

$RepoRoot   = (Get-Item $PSScriptRoot).Parent.Parent.Parent.FullName
$StagingDir = Join-Path $RepoRoot "target\msix-staging"
$OutputDir  = Join-Path $RepoRoot $OutputDir

# Clean and recreate staging area
if (Test-Path $StagingDir) { Remove-Item $StagingDir -Recurse -Force }
New-Item -ItemType Directory -Path "$StagingDir\bin"         | Out-Null
New-Item -ItemType Directory -Path "$StagingDir\sm-plugins"  | Out-Null
New-Item -ItemType Directory -Path "$StagingDir\assets"      | Out-Null
New-Item -ItemType Directory -Path $OutputDir                -Force | Out-Null

# --- Copy binary ---
$TedgeExeFull = Join-Path $RepoRoot $TedgeExe
if (-not (Test-Path $TedgeExeFull)) {
    Write-Error "tedge.exe not found at: $TedgeExeFull"
}
Copy-Item $TedgeExeFull "$StagingDir\bin\tedge.exe"

# --- Copy SM plugins ---
$WingetSrc = Join-Path $RepoRoot "configuration\contrib\sm-plugins\winget.ps1"
if (Test-Path $WingetSrc) {
    Copy-Item $WingetSrc "$StagingDir\sm-plugins\winget.ps1"
} else {
    Write-Warning "winget.ps1 not found at $WingetSrc — sm-plugins will be empty"
}

# --- Generate placeholder logo (1x1 white PNG) if no real logo exists ---
# A real logo should replace assets\logo.png in the repo.
$LogoSrc = Join-Path $RepoRoot "configuration\package_manifests\windows\assets\logo.png"
if (Test-Path $LogoSrc) {
    Copy-Item $LogoSrc "$StagingDir\assets\logo.png"
} else {
    # Minimal valid 1x1 white PNG (base64-encoded)
    $MinimalPng = [Convert]::FromBase64String(
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwADhQGAWjR9awAAAABJRU5ErkJggg=="
    )
    [IO.File]::WriteAllBytes("$StagingDir\assets\logo.png", $MinimalPng)
}

# --- Generate AppxManifest.xml from template ---
$ManifestTemplate = Join-Path $RepoRoot "configuration\package_manifests\windows\AppxManifest.xml"
$ManifestContent  = Get-Content $ManifestTemplate -Raw
$ManifestContent  = $ManifestContent -replace '\$\{VERSION\}',   $Version
$ManifestContent  = $ManifestContent -replace '\$\{PUBLISHER\}', $Publisher
Set-Content -Path "$StagingDir\AppxManifest.xml" -Value $ManifestContent -Encoding UTF8

# --- Pack MSIX ---
$OutputFile = Join-Path $OutputDir "tedge_${Version}_x64.msix"
Write-Host "Packing: $OutputFile"
& $MakeAppx pack /d $StagingDir /p $OutputFile /nv /o
if ($LASTEXITCODE -ne 0) {
    Write-Error "makeappx.exe failed with exit code $LASTEXITCODE"
}

Write-Host ""
Write-Host "MSIX produced: $OutputFile"
Write-Host ""
Write-Host "To install (sideload, unsigned):"
Write-Host "  Add-AppxPackage -AllowUnsigned '$OutputFile'"
