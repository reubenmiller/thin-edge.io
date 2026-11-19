#Requires -RunAsAdministrator
<#
.SYNOPSIS
    Bootstrap C:\ProgramData\tedge\ after a fresh thin-edge.io MSIX install.

.DESCRIPTION
    Creates required directories and seeds default config files.
    All operations are idempotent: existing files are never overwritten,
    so user edits survive re-runs and package upgrades.

    This script is also called automatically at service startup via the
    first-run init embedded in the tedge binary (ensure_windows_data_dirs),
    so manual execution is only needed if the service has not yet started.

.PARAMETER ConfigDir
    Root for thin-edge configuration and data. Defaults to C:\ProgramData\tedge.

.PARAMETER PackageRoot
    Location of the installed MSIX package files (bin\, sm-plugins\, ...).
    Defaults to the parent directory of this script's location.
#>
param(
    [string]$ConfigDir  = "C:\ProgramData\tedge",
    [string]$PackageRoot = (Split-Path -Parent $PSScriptRoot)
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Ensure-Dir([string]$Path) {
    if (-not (Test-Path $Path)) {
        New-Item -ItemType Directory -Path $Path -Force | Out-Null
        Write-Host "Created: $Path"
    }
}

function Write-IfAbsent([string]$Path, [string]$Content) {
    if (-not (Test-Path $Path)) {
        Set-Content -Path $Path -Value $Content -Encoding UTF8
        Write-Host "Created: $Path"
    } else {
        Write-Host "Preserved: $Path"
    }
}

function Copy-IfAbsent([string]$Src, [string]$Dst) {
    if ((Test-Path $Src) -and -not (Test-Path $Dst)) {
        Copy-Item -Path $Src -Destination $Dst
        Write-Host "Copied:   $Src -> $Dst"
    } elseif (-not (Test-Path $Src)) {
        Write-Warning "Source not found, skipping: $Src"
    } else {
        Write-Host "Preserved: $Dst"
    }
}

# --- Directories ---
# Restrict a new configuration directory to SYSTEM (S-1-5-18) and Administrators (S-1-5-32-544),
# removing the read access that C:\ProgramData grants to all users.
# SIDs are used as the group names depend on the Windows display language.
# An existing directory is left as is, so an administrator can relax the ACL.
if (-not (Test-Path $ConfigDir)) {
    Ensure-Dir $ConfigDir
    & icacls $ConfigDir /inheritance:r /grant:r '*S-1-5-18:(OI)(CI)F' /grant:r '*S-1-5-32-544:(OI)(CI)F' | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "Failed to restrict the access to $ConfigDir (icacls exit code $LASTEXITCODE)" }
    Write-Host "Restricted access: $ConfigDir"
}
Ensure-Dir $ConfigDir
Ensure-Dir "$ConfigDir\data"
Ensure-Dir "$ConfigDir\log"
Ensure-Dir "$ConfigDir\tmp"
Ensure-Dir "$ConfigDir\sm-plugins"
Ensure-Dir "$ConfigDir\config-plugins"
Ensure-Dir "$ConfigDir\log-plugins"

# --- Default tedge.toml ---
$ConfigDataDir          = "$ConfigDir\data"           -replace '\\', '/'
$ConfigLogDir           = "$ConfigDir\log"            -replace '\\', '/'
$ConfigTmpDir           = "$ConfigDir\tmp"            -replace '\\', '/'
$ConfigPluginsDir       = "$ConfigDir\config-plugins" -replace '\\', '/'
$LogPluginsDir          = "$ConfigDir\log-plugins"    -replace '\\', '/'

$DefaultToml = @"
[data]
path = '$ConfigDataDir'

[logs]
path = '$ConfigLogDir'

[tmp]
path = '$ConfigTmpDir'

[configuration]
plugin_paths = '$ConfigPluginsDir'

[log]
plugin_paths = '$LogPluginsDir'

[mqtt.bridge]
built_in = true
"@

Write-IfAbsent -Path "$ConfigDir\tedge.toml" -Content $DefaultToml

# --- winget SM plugin ---
Copy-IfAbsent `
    -Src "$PackageRoot\sm-plugins\winget.ps1" `
    -Dst "$ConfigDir\sm-plugins\winget.ps1"

Write-Host ""
Write-Host "thin-edge.io bootstrap complete."
Write-Host "Configure cloud connectivity with: tedge config set c8y.url <your-tenant>.cumulocity.com"
