## ADDED Requirements

### Requirement: plugin executable
The `winget` plugin SHALL be a PowerShell script named `winget.ps1` installed in the sm-plugins directory (`C:\ProgramData\tedge\sm-plugins\`). It SHALL be discoverable by the sm-agent as a plugin named `winget` (extension is stripped on Windows).

#### Scenario: plugin is discovered by sm-agent
- **WHEN** the sm-agent scans the sm-plugins directory on Windows
- **THEN** `winget.ps1` is loaded as a plugin named `winget`

---

### Requirement: module prerequisite check
The plugin SHALL verify that `Microsoft.WinGet.Client` is available when the `list` command is run. If the module is absent, the plugin SHALL print a clear diagnostic message to stderr and exit with code `1`, removing itself from the active plugin list rather than failing silently during a later operation.

#### Scenario: module is missing at list time
- **WHEN** `winget list` is called and `Microsoft.WinGet.Client` is not installed
- **THEN** the plugin writes a diagnostic message to stderr and exits with code `1`

#### Scenario: module is present
- **WHEN** `winget list` is called and `Microsoft.WinGet.Client` is installed
- **THEN** the plugin proceeds normally and exits with code `0`

---

### Requirement: list command
The `list` command SHALL output all packages currently installed via winget, one per line, as tab-separated `name\tversion` pairs. If a package has no version recorded, the version field SHALL be omitted (no trailing tab required).

#### Scenario: list returns installed packages
- **WHEN** `winget list` is called and packages are installed
- **THEN** each installed package is printed as `<id>\t<version>` on its own line and the plugin exits with code `0`

#### Scenario: list returns empty output when no packages installed
- **WHEN** `winget list` is called and no packages have been installed via winget
- **THEN** no lines are written to stdout and the plugin exits with code `0`

#### Scenario: list includes packages without a version
- **WHEN** `winget list` is called and an installed package has no recorded version
- **THEN** that package is listed with only its ID and no trailing tab, and the plugin exits with code `0`

---

### Requirement: prepare command
The `prepare` command SHALL run `winget source update` to refresh all configured package source metadata before a sequence of install/remove operations. If the source update fails, `prepare` SHALL exit with a non-zero code, causing the sm-agent to cancel the planned operation sequence.

#### Scenario: prepare refreshes sources successfully
- **WHEN** `winget prepare` is called
- **THEN** `winget source update` is executed and the plugin exits with code `0`

#### Scenario: prepare failure cancels the operation sequence
- **WHEN** `winget prepare` is called and `winget source update` fails (e.g. no network)
- **THEN** the plugin exits with a non-zero code and the sm-agent does not proceed with installs or removals

---

### Requirement: finalize command
The `finalize` command SHALL exit immediately with code `0`. winget has no transaction, rollback, or post-install cache that requires cleanup.

#### Scenario: finalize always succeeds
- **WHEN** `winget finalize` is called
- **THEN** the plugin exits with code `0` without performing any action

---

### Requirement: install from winget source
When called as `winget install <id> [--module-version <version>]` without `--file`, the plugin SHALL install the package identified by `<id>` from the configured winget sources non-interactively (`--silent`). If `--module-version` is provided and is not the special value `latest`, it SHALL be passed as the exact version constraint. If `--module-version latest` is provided, or if no version is given, the plugin SHALL install the most recent available version without passing a version constraint to winget.

#### Scenario: install package by ID
- **WHEN** `winget install Microsoft.VisualStudioCode` is called
- **THEN** the latest available version of the package is installed and the plugin exits with code `0`

#### Scenario: install package with exact version
- **WHEN** `winget install Microsoft.VisualStudioCode --module-version 1.85.0` is called
- **THEN** version `1.85.0` of the package is installed and the plugin exits with code `0`

#### Scenario: install with version 'latest' fetches most recent version
- **WHEN** `winget install Microsoft.VisualStudioCode --module-version latest` is called
- **THEN** the version constraint is omitted and the most recent available version is installed, exiting with code `0`

#### Scenario: install unknown package ID
- **WHEN** `winget install com.example.DoesNotExist` is called
- **THEN** the plugin writes an error to stderr and exits with code `2`

#### Scenario: install is idempotent
- **WHEN** `winget install <id>` is called for a package that is already installed at the requested version
- **THEN** the plugin exits with code `0` without error

---

### Requirement: install from local file
When called as `winget install <id> --file <path>` (with `--file` present), the plugin SHALL install the package from the local file path using `winget install --silent <path>`. The `--file` argument takes precedence over source resolution; the `<id>` and `--module-version` arguments are accepted for logging but do not change the install source. The caller (tedge-agent) is responsible for downloading the file; the plugin assumes the file exists at the given path and that any required signing trust has been configured on the device.

#### Scenario: install from local exe file
- **WHEN** `winget install MyApp --file C:\tedge\tmp\myapp-1.0.exe` is called
- **THEN** the plugin installs from the local file path and exits with code `0`

#### Scenario: install from local msix file
- **WHEN** `winget install MyApp --file C:\tedge\tmp\myapp.msix` is called
- **THEN** the plugin installs from the local file path and exits with code `0`

#### Scenario: install from local msi file
- **WHEN** `winget install MyApp --file C:\tedge\tmp\myapp.msi` is called
- **THEN** the plugin installs from the local file path and exits with code `0`

#### Scenario: install fails if file path does not exist
- **WHEN** `winget install MyApp --file C:\tedge\tmp\missing.exe` is called and the file does not exist
- **THEN** the plugin writes an error to stderr and exits with code `2`

---

### Requirement: remove command
The `remove` command SHALL uninstall the package identified by `<id>`, non-interactively. If `--module-version` is provided, only that version SHALL be targeted (relevant when multiple versions co-exist). Removing a package that is not installed SHALL NOT be treated as an error.

#### Scenario: remove installed package
- **WHEN** `winget remove Microsoft.VisualStudioCode` is called and the package is installed
- **THEN** the package is uninstalled and the plugin exits with code `0`

#### Scenario: remove package that is not installed
- **WHEN** `winget remove Microsoft.VisualStudioCode` is called and the package is not installed
- **THEN** the plugin exits with code `0` without error

#### Scenario: remove with version
- **WHEN** `winget remove Microsoft.VisualStudioCode --module-version 1.85.0` is called
- **THEN** only version `1.85.0` is targeted for removal and the plugin exits with code `0`

#### Scenario: remove fails if package cannot be uninstalled
- **WHEN** `winget remove <id>` is called and the uninstall fails (e.g. the package blocks removal)
- **THEN** the plugin writes an error to stderr and exits with code `2`

---

### Requirement: update-list command
The `update-list` command SHALL exit with code `1` immediately, signalling to the sm-agent that the plugin does not implement bulk operations. The sm-agent will then fall back to individual `install` and `remove` calls.

#### Scenario: update-list triggers sm-agent fallback
- **WHEN** `winget update-list` is called
- **THEN** the plugin exits with code `1` and the sm-agent retries using individual install/remove commands

---

### Requirement: configurable install scope
The plugin SHALL expose a single `$InstallScope` configuration variable. When set to a non-empty value (`machine` or `user`) the value is forwarded to winget as `--scope`. When empty (the default), no `--scope` argument is passed and winget selects the scope automatically based on what the package supports.

Operators MUST be aware that specifying a scope also restricts package discovery: winget only finds packages that have an installer for the requested scope. Leave `$InstallScope` empty when unsure to avoid false "No package found" errors.

#### Scenario: install with no scope set uses winget default
- **WHEN** `winget install <id>` is called and `$InstallScope` is empty
- **THEN** winget selects the scope automatically and the plugin exits with code `0`

#### Scenario: install with explicit scope forwards it to winget
- **WHEN** `winget install <id>` is called and `$InstallScope` is set to `machine` or `user`
- **THEN** `--scope <value>` is appended to the winget command and the plugin exits with code `0`

---

### Requirement: exit codes
The plugin SHALL use the exit codes defined by the plugin API: `0` success, `1` usage/not-implemented, `2` failure (no retry), `3` retry (transient failure). Network errors during source update or package download SHOULD exit with code `3`.

#### Scenario: transient network failure during install
- **WHEN** `winget install <id>` is called and the package cannot be downloaded due to a network error
- **THEN** the plugin exits with code `3` so the sm-agent may retry
