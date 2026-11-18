# Software management plugin updating thin-edge.io itself on Windows (software type "tedge").
#
# The software "tedge" is installed from a URL to either the installer ZIP (tedge_<version>_<arch>-installer.zip)
# or a tedge.exe. The running tedge.exe cannot be overwritten, but it can be renamed:
# it is renamed to tedge.exe.old, and the new tedge.exe is copied in its place.
# The mapper services are restarted here; tedge-agent, which runs this plugin, detects
# that tedge.exe has changed once the operation is reported, and exits to be restarted
# by the Service Control Manager (see the recovery actions set by install.ps1).
#
# Usage:
#   tedge.ps1 list
#   tedge.ps1 prepare | finalize
#   tedge.ps1 install tedge [--module-version <version>] --file <path>
#   tedge.ps1 remove tedge
#
# Exit codes: 0 success, 1 unsupported command, 2 failure

$ErrorActionPreference = "Stop"

$Command = $null
$Module = $null
$ModuleVersion = $null
$FilePath = $null
for ($i = 0; $i -lt $args.Count; $i++) {
    switch ($args[$i]) {
        "--module-version" { $i++; $ModuleVersion = $args[$i] }
        "--file" { $i++; $FilePath = $args[$i] }
        default {
            if ($null -eq $Command) { $Command = $args[$i] }
            elseif ($null -eq $Module) { $Module = $args[$i] }
        }
    }
}

$ConfigDir = if ($env:TEDGE_CONFIG_DIR) { $env:TEDGE_CONFIG_DIR } else { Join-Path $env:ProgramData "tedge" }

# The tedge.exe run by the services, which is the one to replace
function Get-InstalledTedge {
    foreach ($name in @("tedge-agent", "tedge")) {
        $service = Get-CimInstance Win32_Service -Filter "Name = '$name'" -ErrorAction SilentlyContinue
        if ($service -and $service.PathName -match '^"([^"]+)"') {
            return $Matches[1]
        }
    }
    return (Get-Command tedge.exe -ErrorAction Stop).Source
}

function Get-TedgeVersion([string]$Exe) {
    $output = & $Exe --version
    if ($LASTEXITCODE -ne 0 -or -not ("$output" -match '^tedge (\S+)')) {
        throw "'$Exe --version' failed: $output"
    }
    $Matches[1]
}

# The new tedge.exe and, from an installer ZIP, the directory with its bundled plugins
function Expand-Update([string]$File, [string]$WorkDir) {
    $header = New-Object byte[] 2
    $stream = [System.IO.File]::OpenRead($File)
    try { [void]$stream.Read($header, 0, 2) } finally { $stream.Close() }
    $magic = [System.Text.Encoding]::ASCII.GetString($header)
    if ($magic -eq "PK") {
        Expand-Archive -Path $File -DestinationPath $WorkDir -Force
        $exe = Get-ChildItem -Path $WorkDir -Filter "tedge.exe" -Recurse | Select-Object -First 1
        if (-not $exe) { throw "The installer ZIP contains no tedge.exe" }
        return @{ Exe = $exe.FullName; Root = $WorkDir }
    }
    if ($magic -eq "MZ") {
        $exe = Join-Path $WorkDir "tedge.exe"
        Copy-Item $File $exe
        return @{ Exe = $exe; Root = $null }
    }
    throw "The file is neither an installer ZIP nor tedge.exe"
}

function Install-Tedge {
    if (-not $FilePath) {
        throw "No file to install: set the URL of the installer ZIP or of tedge.exe in the software version"
    }
    $installed = Get-InstalledTedge
    $currentVersion = Get-TedgeVersion $installed

    $workDir = Join-Path $ConfigDir "tmp\tedge-update"
    Remove-Item $workDir -Recurse -Force -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Path $workDir -Force | Out-Null
    try {
        $update = Expand-Update $FilePath $workDir
        $newVersion = Get-TedgeVersion $update.Exe
        if ($ModuleVersion -and $ModuleVersion -ne $newVersion) {
            Write-Warning "Installing tedge $newVersion, while version $ModuleVersion was requested"
        }
        Write-Output "Updating tedge $currentVersion to $newVersion ($installed)"

        # A tedge.exe.old left by the previous update is in use if a process still runs it
        $old = "$installed.old"
        if (Test-Path $old) {
            try { Remove-Item $old -Force } catch { $old = "$installed.$(Get-Date -Format yyyyMMddHHmmss).old" }
        }
        Move-Item $installed $old
        try {
            Copy-Item $update.Exe $installed
        } catch {
            Move-Item $old $installed -Force
            throw
        }

        # Plugins owned by thin-edge.io, shipped with the installer ZIP
        if ($update.Root) {
            foreach ($sub in @("diag-plugins", "log-plugins")) {
                $src = Join-Path $update.Root $sub
                if (Test-Path $src) {
                    Copy-Item (Join-Path $src "*.ps1") (Join-Path $ConfigDir $sub) -Force
                }
            }
            $plugin = Join-Path $update.Root "sm-plugins\tedge.ps1"
            if (Test-Path $plugin) {
                Copy-Item $plugin (Join-Path $ConfigDir "sm-plugins\tedge.ps1") -Force
            }
        }
    } finally {
        Remove-Item $workDir -Recurse -Force -ErrorAction SilentlyContinue
    }

    # The agent restarts itself once the operation is reported; the mappers are restarted now
    Get-Service -Name "tedge-mapper-*" -ErrorAction SilentlyContinue |
        Where-Object Status -eq "Running" |
        ForEach-Object {
            Write-Output "Restarting $($_.Name)"
            Restart-Service -Name $_.Name -Force
        }
}

try {
    switch ($Command) {
        "list" {
            Write-Output "tedge`t$(Get-TedgeVersion (Get-InstalledTedge))"
        }
        "prepare" { }
        "finalize" { }
        "install" {
            if ($Module -ne "tedge") { throw "Unknown module: $Module. This plugin only installs 'tedge'" }
            Install-Tedge
        }
        "remove" {
            throw "thin-edge.io cannot be removed from Cumulocity: run uninstall.ps1 on the device"
        }
        default {
            [Console]::Error.WriteLine("Unsupported command: $Command")
            exit 1
        }
    }
} catch {
    [Console]::Error.WriteLine("Error: $_")
    exit 2
}
exit 0
