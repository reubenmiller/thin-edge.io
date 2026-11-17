## ADDED Requirements

### Requirement: Plugin candidates on Windows

On Windows, a file in a software management, configuration, log or diagnostic plugin directory SHALL be treated as a plugin
only if its extension is `exe`, `bat`, `cmd`, `ps1` or `com`, compared case-insensitively.
Other files SHALL be ignored.
On Unix, every file SHALL remain a plugin candidate, with executability checked when the plugin is invoked.

#### Scenario: Non-executable file ignored
- **WHEN** `C:\ProgramData\tedge\sm-plugins` contains `winget.ps1` and `README.txt`
- **THEN** the agent loads one software management plugin, `winget`

#### Scenario: Upper-case extension
- **WHEN** a log plugin directory contains `EVENTLOG.EXE`
- **THEN** the log manager loads the `EVENTLOG` plugin

### Requirement: Plugin names exclude the extension on Windows

On Windows, the name of a plugin SHALL be its file name without the extension.
On Unix, the name SHALL be the full file name.

#### Scenario: Software type of a PowerShell plugin
- **WHEN** the software management plugin `winget.ps1` is loaded on Windows
- **THEN** the software list reports modules of type `winget`

#### Scenario: Log types of a batch plugin
- **WHEN** the log plugin `journal.cmd` reports the log type `app` on Windows
- **THEN** the agent publishes the log type `app::journal`

### Requirement: Built-in file plugins on Windows

On Windows, the built-in `file` configuration and log plugins SHALL be provided as `file.cmd` wrappers
in `config-plugins` and `log-plugins` that call `tedge.exe run tedge-file-config-plugin` and `tedge.exe run tedge-file-log-plugin`.
The wrappers SHALL be regenerated with the absolute path of the running `tedge.exe` each time a component starts.

#### Scenario: Wrapper follows an upgrade
- **WHEN** the package is upgraded to a new install location and the agent starts
- **THEN** `config-plugins\file.cmd` calls the `tedge.exe` of the new location

### Requirement: Shared plugin discovery rules

The software management plugin manager, the configuration manager, the log manager and `tedge diag collect` SHALL use the same rules to find plugins and derive plugin names.

#### Scenario: Same plugin seen by all components
- **WHEN** a file `custom.ps1` is placed in each plugin directory on Windows
- **THEN** every component names the plugin `custom`
