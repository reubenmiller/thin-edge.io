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

.PARAMETER StartServices
    Start tedge-agent and tedge-mapper-c8y immediately after registering them.
    Defaults to $true.
#>
param(
    [string]$InstallDir    = "C:\Program Files\tedge\bin",
    [string]$ConfigDir     = "C:\ProgramData\tedge",
    [switch]$StartServices = $true
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path

function Write-Step([string]$Msg) { Write-Host "==> $Msg" }

# --- Install binary ---
Write-Step "Installing tedge.exe to $InstallDir"
New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
Copy-Item (Join-Path $ScriptDir "bin\tedge.exe") (Join-Path $InstallDir "tedge.exe") -Force

# Add install dir to system PATH if not already present
$syspath = [Environment]::GetEnvironmentVariable("Path", "Machine")
if ($syspath -notlike "*$InstallDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$syspath;$InstallDir", "Machine")
    Write-Host "    Added $InstallDir to system PATH"
}

# --- Bootstrap data directories ---
Write-Step "Bootstrapping $ConfigDir"
foreach ($sub in @("data", "log", "tmp", "sm-plugins")) {
    New-Item -ItemType Directory -Path (Join-Path $ConfigDir $sub) -Force | Out-Null
}

# Write default tedge.toml only if absent (preserve user config on upgrade)
$TomlPath = Join-Path $ConfigDir "tedge.toml"
if (-not (Test-Path $TomlPath)) {
    $dataDir = (Join-Path $ConfigDir "data") -replace '\\', '/'
    $logDir  = (Join-Path $ConfigDir "log")  -replace '\\', '/'
    $tmpDir  = (Join-Path $ConfigDir "tmp")  -replace '\\', '/'
    Set-Content -Path $TomlPath -Encoding UTF8 -Value @"
[data]
path = '$dataDir'

[logs]
path = '$logDir'

[tmp]
path = '$tmpDir'
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

# --- Register Windows Services ---
$TedgeExe = Join-Path $InstallDir "tedge.exe"

function Register-TedgeService {
    param([string]$Name, [string]$DisplayName, [string]$Args)

    $binPath = "`"$TedgeExe`" $Args"
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
}

Register-TedgeService -Name "tedge-agent"      -DisplayName "thin-edge.io Agent"             -Args "run tedge-agent"
Register-TedgeService -Name "tedge-mapper-c8y" -DisplayName "thin-edge.io Mapper (Cumulocity)" -Args "run tedge-mapper c8y"

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
