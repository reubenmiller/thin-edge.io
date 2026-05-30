## ADDED Requirements

### Requirement: package format and minimum OS version

thin-edge.io for Windows SHALL be distributed as an MSIX package. The package SHALL declare a minimum OS version of Windows 10 version 2004 (build 19041) in its `AppxManifest.xml`, which is the minimum required to support packaged Windows Services via the `desktop6` extension.

#### Scenario: install rejected on unsupported OS
- **WHEN** a user attempts to install the MSIX on Windows 10 version 1909 or earlier
- **THEN** Windows SHALL reject the install with a version incompatibility error

#### Scenario: install succeeds on supported OS
- **WHEN** a user installs the MSIX on Windows 10 version 2004 or later
- **THEN** the install SHALL complete without OS version errors

---

### Requirement: package capabilities

The MSIX SHALL declare the `runFullTrust` restricted capability and the `localSystemServices` restricted capability in `AppxManifest.xml`. These are required to run services as `LocalSystem` with unrestricted filesystem access to `C:\ProgramData\`.

#### Scenario: package installs without capability errors
- **WHEN** the MSIX is installed on a supported OS
- **THEN** Windows SHALL not reject the install due to missing or undeclared capabilities

---

### Requirement: binary contents

The MSIX package SHALL contain a single `bin\tedge.exe` multi-call binary. No per-service executables are shipped — all services invoke `tedge.exe` with different arguments.

#### Scenario: tedge.exe is present after install
- **WHEN** the MSIX is installed
- **THEN** `tedge.exe` SHALL be accessible from the installed package directory under `bin\tedge.exe`

---

### Requirement: post-install configuration bootstrap

On first install, the package SHALL place the following files under `C:\ProgramData\tedge\` if they do not already exist:

- `tedge.toml` — base configuration with explicit Windows paths
- `sm-plugins\winget.ps1` — winget software management plugin

These files SHALL NOT be overwritten on upgrade if they already exist, preserving any user edits.

The initial `tedge.toml` SHALL contain:

```toml
[data]
path = 'C:\ProgramData\tedge\data'

[logs]
path = 'C:\ProgramData\tedge\log'

[tmp]
path = 'C:\ProgramData\tedge\tmp'
```

#### Scenario: tedge.toml created on fresh install
- **WHEN** the MSIX is installed on a machine with no existing `C:\ProgramData\tedge\tedge.toml`
- **THEN** `C:\ProgramData\tedge\tedge.toml` SHALL be created with the Windows path defaults

#### Scenario: tedge.toml preserved on upgrade
- **WHEN** the MSIX is upgraded and `C:\ProgramData\tedge\tedge.toml` already exists
- **THEN** the existing `tedge.toml` SHALL NOT be overwritten

#### Scenario: winget.ps1 placed on fresh install
- **WHEN** the MSIX is installed on a machine with no existing `C:\ProgramData\tedge\sm-plugins\winget.ps1`
- **THEN** `winget.ps1` SHALL be placed at `C:\ProgramData\tedge\sm-plugins\winget.ps1`

#### Scenario: winget.ps1 preserved on upgrade
- **WHEN** the MSIX is upgraded and `C:\ProgramData\tedge\sm-plugins\winget.ps1` already exists
- **THEN** the existing `winget.ps1` SHALL NOT be overwritten

---

### Requirement: install via winget and Add-AppxPackage

The MSIX SHALL be installable using both `winget install` (when published to a winget source) and `Add-AppxPackage` (for direct/sideloaded installs). For sideloaded installs without a trusted code-signing certificate, `Add-AppxPackage -AllowUnsigned` SHALL be usable on machines with developer mode or sideloading enabled.

#### Scenario: install via Add-AppxPackage
- **WHEN** a user runs `Add-AppxPackage .\tedge.msix`
- **THEN** the package installs, services are registered, and `C:\ProgramData\tedge\` is bootstrapped

#### Scenario: install via winget
- **WHEN** the package is available in a configured winget source and a user runs `winget install tedge`
- **THEN** the package installs and services are registered

---

### Requirement: upgrade preserves service state

When the MSIX is upgraded to a new version, Windows SHALL stop the registered services before replacing package files and restart them after the upgrade completes.

#### Scenario: services restart after upgrade
- **WHEN** the MSIX is upgraded from one version to the next
- **THEN** `tedge-agent` and `tedge-mapper-c8y` SHALL be running after the upgrade completes

---

### Requirement: uninstall removes binaries but not data

When the MSIX is uninstalled, the package binaries are removed by Windows. The `C:\ProgramData\tedge\` directory and its contents (configuration, logs, data) SHALL NOT be automatically removed, preserving device configuration and operational history.

#### Scenario: uninstall removes the package
- **WHEN** a user uninstalls the MSIX via `winget uninstall`, `Remove-AppxPackage`, or Windows Settings
- **THEN** `tedge-agent` and `tedge-mapper-c8y` services are unregistered and `tedge.exe` is no longer accessible

#### Scenario: uninstall preserves C:\ProgramData\tedge
- **WHEN** the MSIX is uninstalled
- **THEN** `C:\ProgramData\tedge\` and its contents SHALL remain on disk
