## ADDED Requirements

### Requirement: Components run as Windows Services

`tedge-agent`, `tedge-mapper <cloud>` and `tedge run all` SHALL detect when they are started by the Windows Service Control Manager
and register with it under the service names `tedge-agent`, `tedge-mapper-<cloud>` (or `tedge-mapper-<cloud>@<profile>` for a profile) and `tedge` respectively.
When started from a console, they SHALL run as console processes.

#### Scenario: Service reaches Running
- **WHEN** the SCM starts the `tedge-agent` service
- **THEN** the service reports `Running`
- **AND** the agent publishes its health status `up`

#### Scenario: Console start
- **WHEN** a user runs `tedge-agent` or `tedge run tedge-agent` in a PowerShell window
- **THEN** the agent runs in the foreground and logs to stderr

### Requirement: Graceful stop of a Windows Service

On a `Stop` or `Shutdown` control request, a thin-edge.io service SHALL report `StopPending`,
drain its components through the supervisor, and report `Stopped` once the process work is done.
If the drain does not complete within 30 seconds, the process SHALL report `Stopped` with a non-zero exit code and exit.

#### Scenario: Stop-Service publishes health down
- **WHEN** an administrator runs `Stop-Service tedge-agent`
- **THEN** the agent publishes its health status `down` before the process exits
- **AND** the service state becomes `Stopped`

#### Scenario: Restart-Service
- **WHEN** an administrator runs `Restart-Service tedge-mapper-c8y`
- **THEN** the mapper stops gracefully and starts again

### Requirement: Graceful stop on console events

A thin-edge.io component running in a console SHALL shut down gracefully on Ctrl+C, Ctrl+Break, console close and system shutdown events.

#### Scenario: Ctrl+C
- **WHEN** the user presses Ctrl+C in the console running `tedge-mapper c8y`
- **THEN** the mapper publishes its health status `down` and exits with code 0

### Requirement: Logging of Windows Services

A component running as a Windows Service SHALL write its log to `<config_dir>\log\<service-name>.log`,
honouring the log level configured in `system.toml` and the `--log-level` and `--debug` flags.

#### Scenario: Service log file
- **WHEN** the `tedge-agent` service runs
- **THEN** `C:\ProgramData\tedge\log\tedge-agent.log` receives the agent log records

### Requirement: Service management through system.toml

On Windows, the default service manager configuration SHALL drive the SCM through PowerShell cmdlets
(`Start-Service`, `Stop-Service`, `Restart-Service`, `Set-Service -StartupType`, `Get-Service`),
and SHALL be replaceable by a `[init]` section in `system.toml` as on Linux.

#### Scenario: tedge connect restarts the mapper service
- **WHEN** the user runs `tedge connect c8y` on Windows
- **THEN** the `tedge-mapper-c8y` service is enabled and restarted through the SCM

### Requirement: Termination of process trees on timeout

When a workflow script, an operation plugin or a custom operation exceeds its graceful timeout on Windows, thin-edge.io SHALL request the termination of the process and all its descendants (`taskkill /T`).
When the forceful timeout expires, it SHALL forcefully terminate the process tree (`taskkill /T /F`).
No process started by the command SHALL remain running after the forceful timeout.

#### Scenario: Script exceeding its timeout
- **WHEN** a workflow script starts a child process and runs longer than its timeout plus the forceful timeout extension
- **THEN** the script and its child process are terminated
- **AND** the operation step is reported as failed on timeout

### Requirement: Scripts launched through their interpreter

On Windows, a program whose file extension is `.bat` or `.cmd` SHALL be launched with `cmd.exe /c`,
and a `.ps1` program with `powershell.exe -NoProfile -ExecutionPolicy Bypass -File`.
`sudo` SHALL NOT be used on Windows.

#### Scenario: PowerShell plugin
- **WHEN** the agent invokes the software management plugin `C:\ProgramData\tedge\sm-plugins\winget.ps1 list`
- **THEN** the plugin runs through PowerShell and its output is parsed as the software list
