# thin-edge.io diagnostic plugin: Mosquitto service, configuration and persistence file
#
# Usage: 07_mosquitto.ps1 collect --output-dir <dir> [--config-dir <dir>]
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

$Service = Get-Service -Name "mosquitto" -ErrorAction SilentlyContinue
$InstallDir = Join-Path $env:ProgramFiles "mosquitto"
if (-not $Service -and -not (Test-Path $InstallDir)) {
    [Console]::Error.WriteLine("mosquitto not found")
    exit 2
}
if ($Service) {
    $Service | Format-List Name, Status, StartType | Out-String
}
& (Join-Path $InstallDir "mosquitto.exe") -h 2>&1 | Select-Object -First 1

$Conf = Join-Path $InstallDir "mosquitto.conf"
if (Test-Path $Conf) {
    New-Item -ItemType Directory -Path (Join-Path $OutputDir "mosquitto") -Force | Out-Null
    # Passwords are redacted
    (Get-Content $Conf) -replace "password\s.*", "password <redacted>" |
        Set-Content (Join-Path $OutputDir "mosquitto\mosquitto.conf")
}

# Persistence file, if persistence is enabled (see the Windows install guide)
$Db = Get-ChildItem -Path (Join-Path $env:ProgramData "mosquitto"), $InstallDir -Filter "mosquitto.db" -ErrorAction SilentlyContinue |
    Select-Object -First 1
$SizeLimit = 100MB
if ($Db -and $Db.Length -lt $SizeLimit) {
    [Console]::Error.WriteLine("copying mosquitto db file: path=$($Db.FullName), size=$($Db.Length)B")
    Copy-Item $Db.FullName $OutputDir
} elseif ($Db) {
    [Console]::Error.WriteLine("skipping the mosquitto db file as it is too large: path=$($Db.FullName), size=$($Db.Length)B")
} else {
    [Console]::Error.WriteLine("mosquitto db file not found: persistence is disabled")
}

exit 0
