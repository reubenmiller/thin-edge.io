#Requires -RunAsAdministrator
<#
.SYNOPSIS
    Trust the thin-edge.io developer certificate so its MSIX/APPX packages
    can be installed without developer mode.

.DESCRIPTION
    Imports the bundled tedge-dev.cer into the local machine's Trusted People
    certificate store.  This is a one-time step per machine.  After running
    this script the signed .msix or .appx can be installed normally via
    Add-AppxPackage or double-clicking the file.

    Trusted People is the correct store for sideloaded MSIX packages — it is
    less sensitive than Trusted Root and is specifically intended for this use.

.PARAMETER CertFile
    Path to the .cer file distributed alongside the MSIX package.
    Defaults to tedge-dev.cer in the same directory as this script.
#>
param(
    [string]$CertFile = (Join-Path $PSScriptRoot "tedge-dev.cer")
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not (Test-Path $CertFile)) {
    Write-Error "Certificate file not found: $CertFile`nDownload tedge-dev.cer from the same release as the MSIX."
}

$cert = Import-Certificate -FilePath $CertFile -CertStoreLocation "Cert:\LocalMachine\TrustedPeople"
Write-Host "Trusted: $($cert.Subject)  [$($cert.Thumbprint)]"
Write-Host ""
Write-Host "You can now install the thin-edge.io MSIX package:"
Write-Host "  Add-AppxPackage .\tedge_*.msix"
