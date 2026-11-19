# thin-edge.io diagnostic plugin: internal state of the agent
#
# Usage: 06_internal.ps1 collect --output-dir <dir> [--config-dir <dir>]
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

$StateDir = tedge config get agent.state.path
$Candidates = @(
    (Join-Path $StateDir "entity_store.jsonl"),
    (Join-Path $ConfigDir ".agent\entity_store.jsonl"),
    (Join-Path $ConfigDir ".tedge-mapper-c8y\entity_store.jsonl")
)
$Found = $Candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
if ($Found) {
    Copy-Item $Found $OutputDir
} else {
    [Console]::Error.WriteLine("entity_store.jsonl not found in: $($Candidates -join ', ')")
}

exit 0
