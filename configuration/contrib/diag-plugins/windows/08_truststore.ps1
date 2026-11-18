# thin-edge.io diagnostic plugin: trusted root certificates
#
# Usage: 08_truststore.ps1 collect --output-dir <dir> [--config-dir <dir>]
# Files are written to the output directory; the standard output is saved as output.log.
# Exit code 2 means the plugin does not apply to this device.

$ErrorActionPreference = "Continue"
$OutputDir = ""
$ConfigDir = if ($env:TEDGE_CONFIG_DIR) { $env:TEDGE_CONFIG_DIR } else { Join-Path $env:ProgramData "tedge" }
$Command = ""
for ($i = 0; $i -lt $args.Count; $i++) {
    switch ($args[$i]) {
        "--output-dir" { $i++; $OutputDir = $args[$i] }
        "--config-dir" { $i++; $ConfigDir = $args[$i] }
        "collect" { $Command = "collect" }
    }
}
if ($Command -ne "collect") {
    [Console]::Error.WriteLine("Unknown command")
    exit 1
}
if (-not $OutputDir -or -not (Test-Path -PathType Container $OutputDir)) {
    [Console]::Error.WriteLine("Error: Output directory does not exist: $OutputDir")
    exit 1
}

"Windows trusted root certificates (LocalMachine\Root)"
Get-ChildItem Cert:\LocalMachine\Root |
    Sort-Object Subject |
    Format-Table -AutoSize Thumbprint, NotAfter, Subject |
    Out-File -Width 400 (Join-Path $OutputDir "windows-root-certificates.txt")

foreach ($cloud in @("c8y", "az", "aws")) {
    $Path = tedge config get "$cloud.root_cert_path"
    if ($Path -and (Test-Path $Path)) {
        "$cloud.root_cert_path: $Path"
        Get-ChildItem -Path $Path | Format-Table Name, Length, LastWriteTime | Out-String
    }
}

exit 0
