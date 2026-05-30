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

.PARAMETER SigningCertThumbprint
    Thumbprint of a certificate in the current user's My store to sign the
    MSIX/APPX with signtool.exe. When omitted, packages are produced unsigned.
    Unsigned packages can only be installed with developer mode or
    Add-AppxPackage -AllowUnsigned; use the ZIP installer for policy-restricted
    machines instead.

.NOTES
    Two installation methods are produced:
      *.msix / *.appx  MSIX/APPX package (requires signing for non-dev machines)
      *-installer.zip  ZIP with install.ps1 — no certificate required, uses sc.exe
#>
param(
    [string]$Version                  = $env:GIT_SEMVER,
    [string]$Publisher                = $(if ($env:MSIX_PUBLISHER) { $env:MSIX_PUBLISHER } else { "CN=thin-edge.io" }),
    [string]$Arch                     = $(if ($env:CARGO_ARCH) { $env:CARGO_ARCH } else { "x64" }),
    [string]$TedgeExe                 = "",
    [string]$OutputDir                = "target\packages",
    [string]$SigningCertThumbprint    = $env:MSIX_SIGNING_CERT_THUMBPRINT
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

# Default binary path depends on architecture.
# When --target is passed to Cargo the output goes to target\<triple>\release\,
# not target\release\, so all three arches use their explicit triple path.
if (-not $TedgeExe) {
    $TedgeExe = switch ($Arch) {
        "x64"   { "target\x86_64-pc-windows-msvc\release\tedge.exe" }
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

# --- Developer certificate ---
# When no thumbprint is supplied, create a self-signed dev cert, sign with it,
# and export the public key so users can trust it via trust-dev-cert.ps1.
# Done here (after $OutputDir is created) so the .cer lands in a known location.
if (-not $SigningCertThumbprint) {
    Write-Host "No signing certificate provided — creating self-signed developer certificate"
    $cert = New-SelfSignedCertificate `
        -Type CodeSigningCert `
        -Subject $Publisher `
        -KeyUsage DigitalSignature `
        -FriendlyName "thin-edge.io Developer Certificate" `
        -CertStoreLocation "Cert:\CurrentUser\My" `
        -HashAlgorithm SHA256 `
        -NotAfter (Get-Date).AddYears(3)
    $SigningCertThumbprint = $cert.Thumbprint
    $CertFile = Join-Path $OutputDir "tedge-dev.cer"
    Export-Certificate -Cert $cert -FilePath $CertFile -Type CERT | Out-Null
    Write-Host "Developer certificate: $($cert.Thumbprint)"
    Write-Host "Public key exported:   $CertFile"
    # Expose to subsequent GitHub Actions steps if running in CI
    if ($env:GITHUB_ENV) {
        "CERT_THUMBPRINT=$($cert.Thumbprint)" | Out-File -Append -Encoding utf8 $env:GITHUB_ENV
    }
}

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

    # Sign if a certificate thumbprint was provided
    if ($SigningCertThumbprint) {
        $SignTool = Get-ChildItem `
            "C:\Program Files (x86)\Windows Kits\10\bin\*\x64\signtool.exe" `
            -ErrorAction SilentlyContinue |
            Sort-Object FullName -Descending |
            Select-Object -First 1 -ExpandProperty FullName

        if (-not $SignTool) { Write-Error "signtool.exe not found in Windows SDK." }

        Write-Host "Signing: $OutputFile"
        & $SignTool sign /fd SHA256 /sha1 $SigningCertThumbprint /td SHA256 /tr http://timestamp.digicert.com $OutputFile
        if ($LASTEXITCODE -ne 0) {
            Write-Error "signtool.exe failed signing $OutputFile (exit $LASTEXITCODE)"
        }
    }
}

# --- ZIP installer: no certificate or MSIX policy required ---
# Contains tedge.exe, winget.ps1, install.ps1, uninstall.ps1.
# Install with: PowerShell -ExecutionPolicy Bypass -File install.ps1
$ZipStagingDir = Join-Path $RepoRoot "target\zip-staging-$Arch"
if (Test-Path $ZipStagingDir) { Remove-Item $ZipStagingDir -Recurse -Force }
New-Item -ItemType Directory -Path "$ZipStagingDir\bin"        | Out-Null
New-Item -ItemType Directory -Path "$ZipStagingDir\sm-plugins" | Out-Null

Copy-Item "$StagingDir\bin\tedge.exe"             "$ZipStagingDir\bin\tedge.exe"
if (Test-Path "$StagingDir\sm-plugins\winget.ps1") {
    Copy-Item "$StagingDir\sm-plugins\winget.ps1" "$ZipStagingDir\sm-plugins\winget.ps1"
}

$PkgScripts = Join-Path $RepoRoot "configuration\package_scripts\windows"
Copy-Item (Join-Path $PkgScripts "install.ps1")   "$ZipStagingDir\install.ps1"
Copy-Item (Join-Path $PkgScripts "uninstall.ps1") "$ZipStagingDir\uninstall.ps1"

$ZipFile = Join-Path $OutputDir "tedge_${Version}_${Arch}-installer.zip"
Write-Host "Zipping: $ZipFile"
Compress-Archive -Path "$ZipStagingDir\*" -DestinationPath $ZipFile -Force
Remove-Item $ZipStagingDir -Recurse -Force

Write-Host ""
Write-Host "Packages produced in: $OutputDir"
Write-Host "  tedge_${Version}_${Arch}.msix          — Desktop / IoT Enterprise (MSIX)"
Write-Host "  tedge_${Version}_${Arch}.appx          — IoT Core / legacy sideload (APPX)"
Write-Host "  tedge_${Version}_${Arch}-installer.zip — ZIP installer, no certificate required"
Write-Host ""
if ($SigningCertThumbprint) {
    Write-Host "Packages are signed. Install with:"
    Write-Host "  Add-AppxPackage '$(Join-Path $OutputDir "tedge_${Version}_${Arch}.msix")'"
} else {
    Write-Host "Packages are UNSIGNED. MSIX install options:"
    Write-Host "  Add-AppxPackage -AllowUnsigned '$(Join-Path $OutputDir "tedge_${Version}_${Arch}.msix")'"
    Write-Host "  (requires Developer Mode or sideloading policy)"
    Write-Host ""
    Write-Host "To install without any certificate policy restriction:"
    Write-Host "  Expand-Archive tedge_${Version}_${Arch}-installer.zip"
    Write-Host "  PowerShell -ExecutionPolicy Bypass -File install.ps1"
}
