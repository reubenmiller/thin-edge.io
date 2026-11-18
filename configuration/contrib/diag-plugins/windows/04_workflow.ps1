# thin-edge.io diagnostic plugin: workflow and operation logs
#
# Usage: 04_workflow.ps1 collect --output-dir <dir> [--config-dir <dir>]
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

$LogsPath = tedge config get logs.path
$AgentLogs = Join-Path $LogsPath "agent"
if (Test-Path $AgentLogs) {
    Get-ChildItem -Path $AgentLogs -File -Recurse | Copy-Item -Destination $OutputDir
} else {
    [Console]::Error.WriteLine("$AgentLogs not found")
}

exit 0
