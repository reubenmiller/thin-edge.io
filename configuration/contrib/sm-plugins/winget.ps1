#Requires -Version 5.1
# thin-edge.io software management plugin for winget (Windows Package Manager)
# Invoked by the sm-agent as:
#   powershell.exe -NoProfile -ExecutionPolicy Bypass -File winget.ps1 <command> [args...]
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
$InstallScope = ''   # '' lets winget choose the scope; 'machine' for system-wide, 'user' for current user
                     # Note: specifying a scope also restricts package discovery — winget will
                     # only find packages that have an installer for that scope. Leave empty
                     # when unsure to avoid "No package found" errors.

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
            if ($null -eq $Command)      { $Command = $arg }
            elseif ($null -eq $Module)   { $Module  = $arg }
        }
    }
    $i++
}

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

function Assert-WinGetModule {
    if (-not (Get-Module -ListAvailable -Name Microsoft.WinGet.Client)) {
        Write-Error ("Microsoft.WinGet.Client module is not installed. " +
                     "Run: Install-Module Microsoft.WinGet.Client -Force -AllowClobber")
        exit 1
    }
    Import-Module Microsoft.WinGet.Client -ErrorAction Stop
}

function ConvertTo-InstallExitCode([string]$status) {
    switch ($status) {
        'Ok'                   { return 0 }
        'AlreadyInstalled'     { return 0 }
        'NoApplicableUpgrade'  { return 0 }
        'DownloadError'        { return 3 }
        default                { return 2 }
    }
}

function ConvertTo-UninstallExitCode([string]$status) {
    switch ($status) {
        'Ok'           { return 0 }
        'NotInstalled' { return 0 }
        default        { return 2 }
    }
}

# ---------------------------------------------------------------------------
# Commands
# ---------------------------------------------------------------------------

switch ($Command) {

    'list' {
        Assert-WinGetModule
        $packages = Get-WinGetPackage
        foreach ($pkg in $packages) {
            if ($pkg.InstalledVersion) {
                Write-Output "$($pkg.Id)`t$($pkg.InstalledVersion)"
            } else {
                Write-Output "$($pkg.Id)"
            }
        }
        exit 0
    }

    'prepare' {
        # Refresh all winget source metadata before a sequence of operations,
        # equivalent to apt-get update in the Debian plugin.
        winget source update
        if ($LASTEXITCODE -ne 0) {
            Write-Error "winget source update failed (exit $LASTEXITCODE)"
            exit 2
        }
        exit 0
    }

    'finalize' {
        # winget has no transaction or post-install cache to clean up.
        exit 0
    }

    'update-list' {
        # Returning 1 signals to sm-agent that this plugin does not implement
        # bulk operations; it will fall back to individual install/remove calls.
        exit 1
    }

    'install' {
        Assert-WinGetModule

        if ($null -ne $FilePath) {
            # File-based install: tedge-agent downloaded the binary and passes us
            # the local path. Trust/signing configuration is the operator's concern.
            if (-not (Test-Path -LiteralPath $FilePath)) {
                Write-Error "Install file not found: $FilePath"
                exit 2
            }
            Write-Output "Installing from file: $FilePath"
            $fileArgs = @('install', '--silent', '--accept-package-agreements')
            if ($InstallScope) { $fileArgs += '--scope', $InstallScope }
            $fileArgs += $FilePath
            winget @fileArgs
            $code = $LASTEXITCODE
            # APPINSTALLER_CLI_ERROR_PACKAGE_ALREADY_INSTALLED = 0x8A15002C
            if ($code -eq 0 -or $code -eq -1978335188) {
                exit 0
            }
            Write-Error "winget install --file failed (exit $code)"
            exit 2
        }

        # Source-based install via the PowerShell module (structured result, no screen-scraping).
        $params = [ordered]@{
            Id   = $Module
            Mode = 'Silent'
        }
        # Only set Scope when explicitly configured; the module expects PascalCase.
        if ($InstallScope) { $params['Scope'] = (Get-Culture).TextInfo.ToTitleCase($InstallScope) }
        # 'latest' means no version constraint — let winget pick the newest available.
        if ($null -ne $ModuleVersion -and $ModuleVersion -ne 'latest') { $params['Version'] = $ModuleVersion }

        try {
            $result = Install-WinGetPackage @params
            $code = ConvertTo-InstallExitCode $result.Status
            if ($code -ne 0) {
                Write-Error "Install failed with status: $($result.Status)"
            }
            exit $code
        } catch {
            $msg = $_.Exception.Message
            if ($msg -match 'network|download|connect|timeout') { exit 3 }
            Write-Error $msg
            exit 2
        }
    }

    'remove' {
        Assert-WinGetModule

        # Idempotent: not installed is not an error.
        $existing = Get-WinGetPackage -Id $Module -ErrorAction SilentlyContinue
        if (-not $existing) {
            exit 0
        }

        $params = [ordered]@{
            Id   = $Module
            Mode = 'Silent'
        }
        if ($null -ne $ModuleVersion) { $params['Version'] = $ModuleVersion }

        try {
            $result = Uninstall-WinGetPackage @params
            $code = ConvertTo-UninstallExitCode $result.Status
            if ($code -ne 0) {
                Write-Error "Uninstall failed with status: $($result.Status)"
            }
            exit $code
        } catch {
            Write-Error $_.Exception.Message
            exit 2
        }
    }

    default {
        $valid = 'list, prepare, finalize, install, remove, update-list'
        Write-Error "Unknown command: '$Command'. Valid commands: $valid"
        exit 1
    }
}
