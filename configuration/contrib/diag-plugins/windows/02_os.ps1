# thin-edge.io diagnostic plugin: operating system information
#
# Usage: 02_os.ps1 collect --output-dir <dir> [--config-dir <dir>]
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

"Operating system"
Get-CimInstance Win32_OperatingSystem |
    Format-List Caption, Version, BuildNumber, OSArchitecture, LastBootUpTime, FreePhysicalMemory, TotalVisibleMemorySize |
    Out-String
"Computer"
Get-CimInstance Win32_ComputerSystem | Format-List Manufacturer, Model, NumberOfLogicalProcessors | Out-String
"Disks"
Get-CimInstance Win32_LogicalDisk -Filter "DriveType=3" | Format-Table DeviceID, Size, FreeSpace | Out-String

exit 0
