<#
.SYNOPSIS
    Build the thin-edge.io Windows MSIX and APPX packages.

.DESCRIPTION
    Stages package contents, substitutes version/publisher/arch into the
    manifest, and calls makeappx.exe to produce both a .msix and a .appx
    file from the same staging directory.

    .msix — standard Windows 10/11 Desktop and IoT Enterprise installer.
    .appx — identical content; used for sideloading on Windows IoT Core and
            older tooling that predates the .msix extension.

    Requires the Windows SDK (makeappx.exe), available on GitHub Actions
    windows-latest runners at the standard SDK path.

.PARAMETER Version
    Four-part package version (e.g. "1.5.0.0"). Defaults to GIT_SEMVER env
    var; a three-part semver has ".0" appended automatically.

.PARAMETER Publisher
    AppxManifest publisher identity. Defaults to MSIX_PUBLISHER env var or
    "CN=thin-edge.io".

.PARAMETER Arch
    Target processor architecture written into the manifest Identity element.
    Must be one of: x64, arm64, x86.  Defaults to CARGO_ARCH env var or x64.

.PARAMETER TedgeExe
    Path to the compiled tedge.exe binary. The default resolves relative to
    the repo root using the standard Cargo output layout for the given arch:
      x64   → target\release\tedge.exe
      arm64 → target\aarch64-pc-windows-msvc\release\tedge.exe

.PARAMETER OutputDir
    Directory where packages are written. Defaults to target\packages.
#>
param(
    [string]$Version   = $env:GIT_SEMVER,
    [string]$Publisher = $(if ($env:MSIX_PUBLISHER) { $env:MSIX_PUBLISHER } else { "CN=thin-edge.io" }),
    [string]$Arch      = $(if ($env:CARGO_ARCH) { $env:CARGO_ARCH } else { "x64" }),
    [string]$TedgeExe  = "",
    [string]$OutputDir = "target\packages"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# Normalise semver to four-part version required by MSIX (e.g. 1.5.0 -> 1.5.0.0)
if ($Version -match '^\d+\.\d+\.\d+$') { $Version = "$Version.0" }
if (-not ($Version -match '^\d+\.\d+\.\d+\.\d+$')) {
    Write-Error "VERSION must be a four-part dotted version (got: '$Version'). Set GIT_SEMVER or pass -Version."
}

$ValidArches = @("x64", "arm64", "x86")
if ($Arch -notin $ValidArches) {
    Write-Error "Arch must be one of: $($ValidArches -join ', ') (got: '$Arch')"
}

# Default binary path depends on architecture
if (-not $TedgeExe) {
    $TedgeExe = switch ($Arch) {
        "x64"   { "target\release\tedge.exe" }
        "arm64" { "target\aarch64-pc-windows-msvc\release\tedge.exe" }
        "x86"   { "target\i686-pc-windows-msvc\release\tedge.exe" }
    }
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
Write-Host "Arch:           $Arch"

$RepoRoot   = (Get-Item $PSScriptRoot).Parent.Parent.Parent.FullName
$StagingDir = Join-Path $RepoRoot "target\msix-staging-$Arch"
$OutputDir  = Join-Path $RepoRoot $OutputDir

# Clean and recreate staging area
if (Test-Path $StagingDir) { Remove-Item $StagingDir -Recurse -Force }
New-Item -ItemType Directory -Path "$StagingDir\bin"        | Out-Null
New-Item -ItemType Directory -Path "$StagingDir\sm-plugins" | Out-Null
New-Item -ItemType Directory -Path "$StagingDir\assets"     | Out-Null
New-Item -ItemType Directory -Path $OutputDir               -Force | Out-Null

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

# --- Logo ---
$LogoSrc = Join-Path $RepoRoot "configuration\package_manifests\windows\assets\logo.png"
if (Test-Path $LogoSrc) {
    Copy-Item $LogoSrc "$StagingDir\assets\logo.png"
} else {
    # Minimal valid 1×1 white PNG
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
$ManifestContent  = $ManifestContent -replace '\$\{ARCH\}',      $Arch
Set-Content -Path "$StagingDir\AppxManifest.xml" -Value $ManifestContent -Encoding UTF8

# --- Pack: produce both .msix and .appx from the same staging dir ---
# .msix and .appx are identical formats; the extension signals intent:
#   .msix — Windows 10+ Desktop / IoT Enterprise
#   .appx — IoT Core sideloading and legacy tooling
foreach ($ext in @("msix", "appx")) {
    $OutputFile = Join-Path $OutputDir "tedge_${Version}_${Arch}.${ext}"
    Write-Host "Packing: $OutputFile"
    & $MakeAppx pack /d $StagingDir /p $OutputFile /nv /o
    if ($LASTEXITCODE -ne 0) {
        Write-Error "makeappx.exe failed (exit $LASTEXITCODE) producing .$ext"
    }
}

Write-Host ""
Write-Host "Packages produced in: $OutputDir"
Write-Host "  tedge_${Version}_${Arch}.msix  — Desktop / IoT Enterprise"
Write-Host "  tedge_${Version}_${Arch}.appx  — IoT Core / legacy sideload"
Write-Host ""
Write-Host "To install (sideload, unsigned):"
Write-Host "  Add-AppxPackage -AllowUnsigned '$(Join-Path $OutputDir "tedge_${Version}_${Arch}.msix")'"
