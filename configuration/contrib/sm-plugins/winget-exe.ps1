#Requires -Version 5.1
# thin-edge.io software management plugin for winget (Windows Package Manager)
# This variant uses winget.exe directly — no Microsoft.WinGet.Client module required.
#
# Invoked by the sm-agent as:
#   powershell.exe -NoProfile -ExecutionPolicy Bypass -File winget-exe.ps1 <command> [args...]
#
# Exit codes per the plugin API:
#   0 - success
#   1 - usage error / command not implemented (triggers sm-agent fallback for update-list)
#   2 - failure (permanent, no retry)
#   3 - failure (transient, sm-agent may retry)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# ---------------------------------------------------------------------------
# Configuration — change scope here to affect all install operations
# ---------------------------------------------------------------------------
$InstallScope = 'machine'   # 'machine' for system-wide installs, 'user' for current user

# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------

$Command       = $null
$Module        = $null
$ModuleVersion = $null
$FilePath      = $null

$i = 0
while ($i -lt $args.Count) {
    $arg = $args[$i]
    switch ($arg) {
        '--module-version' { $i++; $ModuleVersion = $args[$i] }
        '-v'               { $i++; $ModuleVersion = $args[$i] }
        '--file'           { $i++; $FilePath      = $args[$i] }
        default {
            if ($null -eq $Command)    { $Command = $arg }
            elseif ($null -eq $Module) { $Module  = $arg }
        }
    }
    $i++
}

# ---------------------------------------------------------------------------
# Exit-code sets (source: https://github.com/microsoft/winget-cli/blob/master/doc/windows/package-manager/winget/returnCodes.md)
# ---------------------------------------------------------------------------

# These codes from winget install all mean "package is already at the desired state".
$INSTALL_OK_CODES = @(
    0,
    -1978335135,   # APPINSTALLER_CLI_ERROR_PACKAGE_ALREADY_INSTALLED (0x8A150061)
    -1978335189    # APPINSTALLER_CLI_ERROR_UPDATE_NOT_APPLICABLE      (0x8A15002B)
)

# These codes indicate a transient failure worth retrying.
$INSTALL_RETRY_CODES = @(
    -1978334969,   # APPINSTALLER_CLI_ERROR_INSTALL_NO_NETWORK  (0x8A150107)
    -1978335224    # APPINSTALLER_CLI_ERROR_DOWNLOAD_FAILED     (0x8A150008)
)

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

function Assert-Winget {
    if (-not (Get-Command winget.exe -ErrorAction SilentlyContinue)) {
        Write-Error "winget.exe not found in PATH. Install Windows Package Manager (App Installer)."
        exit 1
    }
}

function Invoke-Winget {
    param([string[]]$Arguments)
    # Disable progress spinners; redirect stderr so it doesn't pollute stdout.
    $result = & winget.exe @Arguments 2>&1
    return $result
}

function Test-PackageInstalled {
    param([string]$Id)
    # winget list always exits 0 even when nothing is found; check the output text.
    $output = (Invoke-Winget @('list', '--id', $Id, '--exact', '--disable-interactivity')) | Out-String
    return ($output -match [regex]::Escape($Id))
}

# ---------------------------------------------------------------------------
# Commands
# ---------------------------------------------------------------------------

switch ($Command) {

    'list' {
        Assert-Winget
        # winget list has no --output json option; winget export writes structured
        # JSON (schema: packages.schema.2.0.json) and is the recommended machine-
        # readable interface for the installed package inventory.
        $tmpFile = [System.IO.Path]::GetTempFileName() + '.json'
        try {
            winget export --output $tmpFile --disable-interactivity --accept-source-agreements | Out-Null
            if ($LASTEXITCODE -ne 0) {
                Write-Error "winget export failed (exit $LASTEXITCODE)"
                exit 2
            }
            $data = Get-Content $tmpFile -Raw | ConvertFrom-Json
            foreach ($source in $data.Sources) {
                foreach ($pkg in $source.Packages) {
                    $id  = $pkg.PackageIdentifier
                    $ver = $pkg.Version
                    if ($ver) { Write-Output "$id`t$ver" }
                    else      { Write-Output $id }
                }
            }
        } finally {
            if (Test-Path $tmpFile) { Remove-Item $tmpFile -Force }
        }
        exit 0
    }

    'prepare' {
        Assert-Winget
        winget source update --disable-interactivity
        if ($LASTEXITCODE -ne 0) {
            Write-Error "winget source update failed (exit $LASTEXITCODE)"
            exit 2
        }
        exit 0
    }

    'finalize' {
        exit 0
    }

    'update-list' {
        # Returning 1 signals to sm-agent that this plugin does not implement
        # bulk operations; it will fall back to individual install/remove calls.
        exit 1
    }

    'install' {
        Assert-Winget

        if ($null -ne $FilePath) {
            if (-not (Test-Path -LiteralPath $FilePath)) {
                Write-Error "Install file not found: $FilePath"
                exit 2
            }
            Write-Output "Installing from file: $FilePath"
            winget install --silent --scope $InstallScope --accept-package-agreements $FilePath
            $code = $LASTEXITCODE
            if ($INSTALL_OK_CODES -contains $code) { exit 0 }
            Write-Error "winget install --file failed (exit $code)"
            exit 2
        }

        $installArgs = @(
            'install',
            '--id', $Module,
            '--silent',
            '--scope', $InstallScope,
            '--accept-package-agreements',
            '--accept-source-agreements',
            '--disable-interactivity'
        )
        if ($null -ne $ModuleVersion) {
            $installArgs += '--version'
            $installArgs += $ModuleVersion
        }

        winget @installArgs
        $code = $LASTEXITCODE
        if ($INSTALL_OK_CODES -contains $code)    { exit 0 }
        if ($INSTALL_RETRY_CODES -contains $code) { exit 3 }
        Write-Error "winget install failed (exit $code)"
        exit 2
    }

    'remove' {
        Assert-Winget

        # Idempotent: not installed is not an error.
        if (-not (Test-PackageInstalled $Module)) { exit 0 }

        $removeArgs = @(
            'uninstall',
            '--id', $Module,
            '--silent',
            '--accept-source-agreements',
            '--disable-interactivity'
        )
        if ($null -ne $ModuleVersion) {
            $removeArgs += '--version'
            $removeArgs += $ModuleVersion
        }

        winget @removeArgs
        $code = $LASTEXITCODE
        if ($code -eq 0) { exit 0 }
        Write-Error "winget uninstall failed (exit $code)"
        exit 2
    }

    default {
        $valid = 'list, prepare, finalize, install, remove, update-list'
        Write-Error "Unknown command: '$Command'. Valid commands: $valid"
        exit 1
    }
}
