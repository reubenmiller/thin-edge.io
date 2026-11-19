#Requires -RunAsAdministrator
<#
.SYNOPSIS
    Install thin-edge.io on Windows without an MSIX package.

.DESCRIPTION
    Copies binaries and plugins to the install directory, bootstraps
    C:\ProgramData\tedge\, and registers tedge-agent and tedge-mapper-c8y
    as Windows Services via sc.exe.

    This script requires no code-signing certificate and no MSIX policy.
    It is the recommended installation path when MSIX sideloading is blocked
    by Group Policy.

    All operations are idempotent: re-running upgrades the binary while
    preserving existing configuration.

.PARAMETER InstallDir
    Directory where tedge.exe is placed. Defaults to C:\Program Files\tedge\bin.

.PARAMETER ConfigDir
    Root for configuration and data. Defaults to C:\ProgramData\tedge.

.PARAMETER SourceExe
    The tedge.exe to install. Defaults to bin\tedge.exe next to this script (ZIP package layout).

.PARAMETER StartServices
    Start tedge-agent and tedge-mapper-c8y immediately after registering them.
    Defaults to $true.
#>
param(
    [string]$InstallDir    = "C:\Program Files\tedge\bin",
    [string]$ConfigDir     = "C:\ProgramData\tedge",
    [string]$SourceExe     = "",
    [switch]$StartServices = $true
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path

function Write-Step([string]$Msg) { Write-Host "==> $Msg" }

# --- Install binary ---
Write-Step "Installing tedge.exe to $InstallDir"
New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
if (-not $SourceExe) { $SourceExe = Join-Path $ScriptDir "bin\tedge.exe" }
Copy-Item $SourceExe (Join-Path $InstallDir "tedge.exe") -Force

# Add install dir to system PATH if not already present
$syspath = [Environment]::GetEnvironmentVariable("Path", "Machine")
if ($syspath -notlike "*$InstallDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$syspath;$InstallDir", "Machine")
    Write-Host "    Added $InstallDir to system PATH"
}

# --- Bootstrap data directories ---
Write-Step "Bootstrapping $ConfigDir"
# Restrict a new configuration directory to SYSTEM (S-1-5-18) and Administrators (S-1-5-32-544),
# removing the read access that C:\ProgramData grants to all users.
# SIDs are used as the group names depend on the Windows display language.
if (-not (Test-Path $ConfigDir)) {
    New-Item -ItemType Directory -Path $ConfigDir -Force | Out-Null
    & icacls $ConfigDir /inheritance:r /grant:r '*S-1-5-18:(OI)(CI)F' /grant:r '*S-1-5-32-544:(OI)(CI)F' | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "Failed to restrict the access to $ConfigDir (icacls exit code $LASTEXITCODE)" }
    Write-Host "    Restricted access to SYSTEM and Administrators"
}
foreach ($sub in @("data", "log", "tmp", "sm-plugins", "config-plugins", "log-plugins", "diag-plugins")) {
    New-Item -ItemType Directory -Path (Join-Path $ConfigDir $sub) -Force | Out-Null
}

# Write default tedge.toml only if absent (preserve user config on upgrade)
$TomlPath = Join-Path $ConfigDir "tedge.toml"
if (-not (Test-Path $TomlPath)) {
    $dataDir          = (Join-Path $ConfigDir "data")           -replace '\\', '/'
    $logDir           = (Join-Path $ConfigDir "log")            -replace '\\', '/'
    $tmpDir           = (Join-Path $ConfigDir "tmp")            -replace '\\', '/'
    $configPluginsDir = (Join-Path $ConfigDir "config-plugins") -replace '\\', '/'
    $logPluginsDir    = (Join-Path $ConfigDir "log-plugins")    -replace '\\', '/'
    $diagPluginsDir   = (Join-Path $ConfigDir "diag-plugins")   -replace '\\', '/'
    Set-Content -Path $TomlPath -Encoding UTF8 -Value @"
[data]
path = '$dataDir'

[logs]
path = '$logDir'

[tmp]
path = '$tmpDir'

[configuration]
plugin_paths = '$configPluginsDir'

[log]
plugin_paths = '$logPluginsDir'

[diag]
plugin_paths = '$diagPluginsDir'

[mqtt.bridge]
built_in = true
"@
    Write-Host "    Created $TomlPath"
} else {
    Write-Host "    Preserved existing $TomlPath"
}

# Copy winget SM plugin only if absent
$WingetDst = Join-Path $ConfigDir "sm-plugins\winget.ps1"
$WingetSrc = Join-Path $ScriptDir "sm-plugins\winget.ps1"
if ((Test-Path $WingetSrc) -and -not (Test-Path $WingetDst)) {
    Copy-Item $WingetSrc $WingetDst
    Write-Host "    Installed winget.ps1"
} elseif (Test-Path $WingetDst) {
    Write-Host "    Preserved existing winget.ps1"
}

# The diagnostic, Event Log and tedge update plugins are owned by thin-edge.io: always replace them.
# They are next to this script in the installer ZIP, and under configuration\contrib in the repository.
$ContribDir = Join-Path $ScriptDir "..\..\contrib"
$BundledPlugins = @{
    "diag-plugins" = @((Join-Path $ScriptDir "diag-plugins\*.ps1"), (Join-Path $ContribDir "diag-plugins\windows\*.ps1"))
    "log-plugins"  = @((Join-Path $ScriptDir "log-plugins\*.ps1"), (Join-Path $ContribDir "log-plugins\eventlog.ps1"))
    # The plugin updating thin-edge.io itself (software type "tedge")
    "sm-plugins"   = @((Join-Path $ScriptDir "sm-plugins\tedge.ps1"), (Join-Path $ContribDir "sm-plugins\tedge.ps1"))
}
foreach ($sub in $BundledPlugins.Keys) {
    $src = $BundledPlugins[$sub] | Where-Object { Test-Path $_ } | Select-Object -First 1
    if ($src) {
        Copy-Item $src (Join-Path $ConfigDir $sub) -Force
        Write-Host "    Installed $sub"
    } else {
        Write-Warning "No $sub found next to the installer"
    }
}

# --- Register Windows Services ---
$TedgeExe = Join-Path $InstallDir "tedge.exe"

function Register-TedgeService {
    # Not named $Args, which is a PowerShell automatic variable
    param([string]$Name, [string]$DisplayName, [string]$Arguments)

    $binPath = "`"$TedgeExe`" $Arguments"
    $existing = sc.exe query $Name 2>$null
    if ($LASTEXITCODE -eq 0) {
        Write-Step "Updating service: $Name"
        sc.exe stop $Name 2>$null | Out-Null
        sc.exe config $Name binPath= $binPath DisplayName= $DisplayName start= auto | Out-Null
    } else {
        Write-Step "Registering service: $Name"
        sc.exe create $Name binPath= $binPath DisplayName= $DisplayName start= auto obj= LocalSystem | Out-Null
    }
    sc.exe description $Name "thin-edge.io — $DisplayName" | Out-Null
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Failed to register service '$Name'"
    }

    # Restart after a crash or a non-zero exit code, like Restart=on-failure of the systemd units:
    # after 5 s twice, then every 60 s. The failure count is reset after a day without failure.
    sc.exe failure $Name reset= 86400 actions= restart/5000/restart/5000/restart/60000 | Out-Null
    sc.exe failureflag $Name 1 | Out-Null
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Failed to set the recovery actions of service '$Name'"
    }
}

Register-TedgeService -Name "tedge-agent"      -DisplayName "thin-edge.io Agent"             -Arguments "run tedge-agent"
Register-TedgeService -Name "tedge-mapper-c8y" -DisplayName "thin-edge.io Mapper (Cumulocity)" -Arguments "run tedge-mapper c8y"
# User-defined flows (C:\ProgramData\tedge\mappers\local\flows), registered but not started, as on Linux
Register-TedgeService -Name "tedge-mapper-local" -DisplayName "thin-edge.io Mapper (local flows)" -Arguments "run tedge-mapper local"
sc.exe config tedge-mapper-local start= demand | Out-Null
# Single-process mode (`tedge run all`), registered but not started: use it instead of the services above
Register-TedgeService -Name "tedge" -DisplayName "thin-edge.io" -Arguments "run all"
sc.exe config tedge start= demand | Out-Null

# --- Optionally start services ---
if ($StartServices) {
    Write-Step "Starting services"
    sc.exe start tedge-agent      | Out-Null
    sc.exe start tedge-mapper-c8y | Out-Null
    Write-Host "    Services started. Check status with: sc query tedge-agent"
}

Write-Host ""
Write-Host "thin-edge.io installed successfully."
Write-Host "Configure cloud connectivity:"
Write-Host "  tedge config set c8y.url <your-tenant>.cumulocity.com"
