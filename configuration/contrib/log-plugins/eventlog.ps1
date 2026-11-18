# Log plugin for the Windows Event Log, the Windows counterpart of the journald plugin.
#
# Usage:
#   eventlog.ps1 list
#       List the event logs this plugin supports, one per line
#
#   eventlog.ps1 get <log-name> [--since <timestamp>] [--until <timestamp>]
#       Print the events of an event log, oldest first.
#       Timestamps are in seconds since the Unix epoch; the default is the last 24 hours.
#
# Examples:
#   eventlog.ps1 list
#   eventlog.ps1 get System --since 1696250000 --until 1696260000

$ErrorActionPreference = "Stop"

# Event logs offered as log types. Others can still be fetched with "get".
$LogNames = @("System", "Application", "Setup")

function ConvertFrom-UnixTime([string]$Value) {
    [DateTimeOffset]::FromUnixTimeSeconds([int64]$Value).LocalDateTime
}

function Write-Usage {
    [Console]::Error.WriteLine("Usage: eventlog.ps1 list | get <log-name> [--since <timestamp>] [--until <timestamp>]")
}

function Get-LogTypes {
    foreach ($name in $LogNames) {
        # Only list the logs that exist on this edition of Windows
        if (Get-WinEvent -ListLog $name -ErrorAction SilentlyContinue) {
            $name
        }
    }
}

function Get-Logs([string[]]$Arguments) {
    if ($Arguments.Count -lt 1) {
        [Console]::Error.WriteLine("Error: 'get' expects a mandatory <log-name> argument")
        Write-Usage
        exit 1
    }
    $logName = $Arguments[0]
    $since = (Get-Date).AddDays(-1)
    $until = Get-Date

    $i = 1
    while ($i -lt $Arguments.Count) {
        switch ($Arguments[$i]) {
            "--since" { $i++; $since = ConvertFrom-UnixTime $Arguments[$i] }
            "--until" { $i++; $until = ConvertFrom-UnixTime $Arguments[$i] }
            default {
                [Console]::Error.WriteLine("Unknown option: $($Arguments[$i])")
                exit 1
            }
        }
        $i++
    }

    if (-not (Get-WinEvent -ListLog $logName -ErrorAction SilentlyContinue)) {
        [Console]::Error.WriteLine("Error: event log not found: $logName")
        exit 1
    }

    # Get-WinEvent fails with NoMatchingEventsFound when the time range is empty
    $events = Get-WinEvent -FilterHashtable @{ LogName = $logName; StartTime = $since; EndTime = $until } -ErrorAction SilentlyContinue
    if ($events) {
        # Get-WinEvent returns the newest event first
        [array]::Reverse($events)
        foreach ($e in $events) {
            $message = if ($e.Message) { $e.Message -replace "\r?\n", " " } else { "" }
            "{0} {1} {2}[{3}]: {4}" -f $e.TimeCreated.ToString("yyyy-MM-ddTHH:mm:sszzz"), $e.LevelDisplayName, $e.ProviderName, $e.Id, $message
        }
    }
}

if ($args.Count -lt 1) {
    Write-Usage
    exit 1
}

switch ($args[0]) {
    "list" { Get-LogTypes }
    "get" { Get-Logs ($args | Select-Object -Skip 1) }
    default {
        [Console]::Error.WriteLine("Error: Unknown command '$($args[0])'")
        Write-Usage
        exit 1
    }
}
exit 0
