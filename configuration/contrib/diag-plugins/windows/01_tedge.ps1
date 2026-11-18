# thin-edge.io diagnostic plugin: thin-edge.io version, services, service logs and configuration
#
# Usage: 01_tedge.ps1 collect --output-dir <dir> [--config-dir <dir>]
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

"version: $(tedge --version)"

# Services and their state
Get-Service -Name "tedge*", "mosquitto" -ErrorAction SilentlyContinue |
    Format-Table -AutoSize Name, Status, StartType, DisplayName |
    Out-File (Join-Path $OutputDir "services.txt")

# Service logs: the last 1000 lines, as the files are not rotated
$LogDir = Join-Path $ConfigDir "log"
Get-ChildItem -Path $LogDir -Filter "*.log" -File -ErrorAction SilentlyContinue | ForEach-Object {
    Get-Content $_.FullName -Tail 1000 | Out-File (Join-Path $OutputDir $_.Name)
}

foreach ($cloud in @("c8y", "az", "aws")) {
    tedge config get "$cloud.url" *> $null
    if ($LASTEXITCODE -eq 0) {
        tedge bridge inspect $cloud *> (Join-Path $OutputDir "tedge-bridge-inspect-$cloud.log")
    }
}

Copy-Item (Join-Path $ConfigDir "tedge.toml") $OutputDir -ErrorAction SilentlyContinue
$Mappers = Join-Path $ConfigDir "mappers"
if (Test-Path $Mappers) {
    Copy-Item $Mappers (Join-Path $OutputDir "mappers") -Recurse
}
tedge config list --all *> (Join-Path $OutputDir "tedge-config-list.log")

exit 0
