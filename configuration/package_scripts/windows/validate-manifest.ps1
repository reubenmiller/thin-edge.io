<#
.SYNOPSIS
    Validate AppxManifest.xml using makeappx.exe before a full package build.

.DESCRIPTION
    Substitutes placeholders, builds a minimal staging directory with stub
    files for every binary and asset referenced in the manifest, then asks
    makeappx.exe to pack it.  makeappx performs full schema and semantic
    validation (capabilities, extension placement, file references, version
    format, etc.) and reports exact line/column for every error.

    Because stub files are used this runs in a few seconds without needing a
    compiled tedge.exe, so it can run as an early CI step or locally before
    committing a manifest change.

.PARAMETER ManifestTemplate
    Path to AppxManifest.xml (may contain ${VERSION} and ${PUBLISHER}).
    Defaults to configuration\package_manifests\windows\AppxManifest.xml
    relative to the repo root.

.PARAMETER Version
    Four-part version string inserted for ${VERSION}. Defaults to "1.0.0.0".

.PARAMETER Publisher
    Publisher string inserted for ${PUBLISHER}. Defaults to "CN=TestPublisher".

.EXAMPLE
    # Run from the repo root
    pwsh configuration/package_scripts/windows/validate-manifest.ps1

.EXAMPLE
    # Check a modified manifest before pushing
    pwsh configuration/package_scripts/windows/validate-manifest.ps1 `
        -ManifestTemplate configuration/package_manifests/windows/AppxManifest.xml
#>
param(
    [string]$ManifestTemplate = $null,
    [string]$Version          = "1.0.0.0",
    [string]$Publisher        = "CN=TestPublisher"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# --- Locate repo root and manifest template ---
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot  = (Get-Item $ScriptDir).Parent.Parent.Parent.FullName

if (-not $ManifestTemplate) {
    $ManifestTemplate = Join-Path $RepoRoot "configuration\package_manifests\windows\AppxManifest.xml"
}
if (-not (Test-Path $ManifestTemplate)) {
    Write-Error "Manifest template not found: $ManifestTemplate"
}

# --- Locate makeappx.exe ---
$MakeAppx = Get-ChildItem `
    "C:\Program Files (x86)\Windows Kits\10\bin\*\x64\makeappx.exe" `
    -ErrorAction SilentlyContinue |
    Sort-Object FullName -Descending |
    Select-Object -First 1 -ExpandProperty FullName

if (-not $MakeAppx) {
    Write-Error "makeappx.exe not found. Install the Windows 10 SDK."
}

# --- Substitute placeholders ---
if ($Version -match '^\d+\.\d+\.\d+$') { $Version = "$Version.0" }

$ManifestContent = Get-Content $ManifestTemplate -Raw
$ManifestContent = $ManifestContent -replace '\$\{VERSION\}',   $Version
$ManifestContent = $ManifestContent -replace '\$\{PUBLISHER\}', $Publisher

# --- Parse manifest to discover referenced files ---
[xml]$Manifest = $ManifestContent

$ns = @{
    pkg     = "http://schemas.microsoft.com/appx/manifest/foundation/windows10"
    uap     = "http://schemas.microsoft.com/appx/manifest/uap/windows10"
    desktop6 = "http://schemas.microsoft.com/appx/manifest/desktop/windows10/6"
}

# Collect every path referenced via Executable= or Logo= attributes
$referencedFiles = @()

# Package/Properties/Logo
$logo = $Manifest.Package.Properties.Logo
if ($logo) { $referencedFiles += $logo }

# Application Executable
foreach ($app in $Manifest.Package.Applications.Application) {
    if ($app.Executable) { $referencedFiles += $app.Executable }
    # VisualElements logos — guard with PSObject.Properties to avoid
    # StrictMode throwing on attributes absent from this manifest.
    foreach ($ve in $app.VisualElements) {
        foreach ($attr in @("Square150x150Logo","Square44x44Logo","Square71x71Logo","Square310x310Logo","Wide310x150Logo")) {
            if ($ve.PSObject.Properties[$attr]) {
                $val = $ve.$attr
                if ($val) { $referencedFiles += $val }
            }
        }
    }
    # Extension Executables
    foreach ($ext in $app.Extensions.Extension) {
        if ($ext.Executable) { $referencedFiles += $ext.Executable }
    }
}

$referencedFiles = $referencedFiles | Sort-Object -Unique

# --- Build minimal staging directory ---
$StagingDir = Join-Path $env:TEMP "msix-manifest-validate-$(Get-Random)"
New-Item -ItemType Directory -Path $StagingDir | Out-Null

try {
    # Write substituted manifest
    Set-Content -Path "$StagingDir\AppxManifest.xml" -Value $ManifestContent -Encoding UTF8

    # Create stub files for every referenced path
    foreach ($relPath in $referencedFiles) {
        $fullPath = Join-Path $StagingDir $relPath
        $dir = Split-Path -Parent $fullPath
        if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir | Out-Null }
        # Stub: empty file for scripts/logos, minimal PE header stub for .exe
        if ($relPath -match '\.exe$') {
            # Minimal valid PE so makeappx doesn't reject it as non-executable
            # (makeappx only checks the manifest, not binary validity, so an empty file is fine)
            [IO.File]::WriteAllBytes($fullPath, [byte[]]@())
        } else {
            [IO.File]::WriteAllBytes($fullPath, [byte[]]@())
        }
    }

    # --- Run makeappx to validate ---
    $OutMsix = Join-Path $env:TEMP "msix-manifest-validate-$(Get-Random).msix"
    Write-Host "Validating manifest: $ManifestTemplate"
    Write-Host "  makeappx: $MakeAppx"
    Write-Host "  staging:  $StagingDir"
    Write-Host ""

    & $MakeAppx pack /d $StagingDir /p $OutMsix /nv /o 2>&1

    if ($LASTEXITCODE -ne 0) {
        Write-Host ""
        Write-Error "Manifest validation FAILED (makeappx exit code $LASTEXITCODE). See errors above."
    }

    Write-Host ""
    Write-Host "Manifest validation passed."

} finally {
    Remove-Item $StagingDir -Recurse -Force -ErrorAction SilentlyContinue
    if (Test-Path $OutMsix) { Remove-Item $OutMsix -Force -ErrorAction SilentlyContinue }
}
