<#
.SYNOPSIS
    Create a self-signed developer code-signing certificate for thin-edge.io.

.DESCRIPTION
    Creates a self-signed code-signing certificate in the CurrentUser\My store,
    exports the public key as a .cer file (safe to distribute), and prints the
    thumbprint for use with package.ps1 -SigningCertThumbprint.

    The .cer file is imported by trust-dev-cert.ps1 on each target machine so
    that MSIX packages signed with this certificate can be installed without
    developer mode or a purchased code-signing certificate.

    Intended for CI builds and internal distribution only. For public
    distribution, use a certificate from a trusted CA.

.PARAMETER CertFile
    Path where the public-key .cer file is written. Defaults to
    target\packages\tedge-dev.cer relative to the repo root.

.PARAMETER Subject
    Certificate subject (CN). Defaults to "CN=thin-edge.io".

.PARAMETER ValidityYears
    How many years the certificate is valid. Defaults to 3.

.OUTPUTS
    Writes the certificate thumbprint to stdout and sets CERT_THUMBPRINT in
    $env:GITHUB_ENV when running inside GitHub Actions.
#>
param(
    [string]$CertFile      = "",
    [string]$Subject       = "CN=thin-edge.io",
    [int]$ValidityYears    = 3
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$RepoRoot = (Get-Item $PSScriptRoot).Parent.Parent.Parent.FullName
if (-not $CertFile) {
    $CertFile = Join-Path $RepoRoot "target\packages\tedge-dev.cer"
}

New-Item -ItemType Directory -Path (Split-Path -Parent $CertFile) -Force | Out-Null

# Create the self-signed code-signing certificate
$cert = New-SelfSignedCertificate `
    -Type CodeSigningCert `
    -Subject $Subject `
    -KeyUsage DigitalSignature `
    -FriendlyName "thin-edge.io Developer Certificate" `
    -CertStoreLocation "Cert:\CurrentUser\My" `
    -HashAlgorithm SHA256 `
    -NotAfter (Get-Date).AddYears($ValidityYears)

# Export public key (.cer) — safe to distribute alongside signed packages
Export-Certificate -Cert $cert -FilePath $CertFile -Type CERT | Out-Null
Write-Host "Certificate created:  $($cert.Thumbprint)"
Write-Host "Public key exported:  $CertFile"
Write-Host ""
Write-Host "Pass to package.ps1:  -SigningCertThumbprint $($cert.Thumbprint)"

# Expose thumbprint to subsequent GitHub Actions steps
if ($env:GITHUB_ENV) {
    "CERT_THUMBPRINT=$($cert.Thumbprint)" | Out-File -Append -Encoding utf8 $env:GITHUB_ENV
    Write-Host "Set CERT_THUMBPRINT in GITHUB_ENV"
}
