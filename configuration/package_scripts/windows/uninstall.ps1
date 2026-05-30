#Requires -RunAsAdministrator
<#
.SYNOPSIS
    Uninstall thin-edge.io installed via the ZIP installer.

.DESCRIPTION
    Stops and removes the Windows Services, removes the installed binary,
    and optionally removes C:\ProgramData\tedge\ (off by default to preserve
    device configuration and operational history).

.PARAMETER InstallDir
    Directory where tedge.exe was installed. Defaults to C:\Program Files\tedge\bin.

.PARAMETER RemoveData
    Also remove C:\ProgramData\tedge\ (config, logs, data). Defaults to $false.
    Pass -RemoveData to do a full clean uninstall.

.PARAMETER ConfigDir
    Data root, only used when -RemoveData is set. Defaults to C:\ProgramData\tedge.
#>
param(
    [string]$InstallDir = "C:\Program Files\tedge\bin",
    [string]$ConfigDir  = "C:\ProgramData\tedge",
    [switch]$RemoveData
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Write-Step([string]$Msg) { Write-Host "==> $Msg" }

function Remove-TedgeService([string]$Name) {
    $existing = sc.exe query $Name 2>$null
    if ($LASTEXITCODE -ne 0) {
        Write-Host "    Service '$Name' not found, skipping"
        return
    }
    Write-Step "Stopping and removing service: $Name"
    sc.exe stop $Name 2>$null | Out-Null
    # Wait briefly for the service to stop
    $deadline = (Get-Date).AddSeconds(10)
    while ((sc.exe query $Name | Select-String "RUNNING") -and (Get-Date) -lt $deadline) {
        Start-Sleep -Milliseconds 500
    }
    sc.exe delete $Name | Out-Null
}

Remove-TedgeService "tedge-agent"
Remove-TedgeService "tedge-mapper-c8y"

# --- Remove binary and install directory ---
Write-Step "Removing $InstallDir"
if (Test-Path $InstallDir) {
    Remove-Item $InstallDir -Recurse -Force
    # Remove parent C:\Program Files\tedge if now empty
    $parent = Split-Path -Parent $InstallDir
    if ((Test-Path $parent) -and -not (Get-ChildItem $parent)) {
        Remove-Item $parent -Force
    }
}

# Remove from system PATH
$syspath = [Environment]::GetEnvironmentVariable("Path", "Machine")
if ($syspath -like "*$InstallDir*") {
    $newpath = ($syspath -split ';' | Where-Object { $_ -ne $InstallDir }) -join ';'
    [Environment]::SetEnvironmentVariable("Path", $newpath, "Machine")
    Write-Host "    Removed $InstallDir from system PATH"
}

# --- Optionally remove data ---
if ($RemoveData) {
    Write-Step "Removing $ConfigDir"
    if (Test-Path $ConfigDir) {
        Remove-Item $ConfigDir -Recurse -Force
    }
} else {
    Write-Host ""
    Write-Host "Configuration and data preserved at: $ConfigDir"
    Write-Host "Run with -RemoveData to also remove it."
}

Write-Host ""
Write-Host "thin-edge.io uninstalled."
