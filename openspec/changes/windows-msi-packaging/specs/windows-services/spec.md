## ADDED Requirements

### Requirement: services registered by the MSIX package

The MSIX package SHALL register the following Windows Services via `desktop6:Extension` declarations in `AppxManifest.xml`:

| Service name       | Invocation                                  | Start type |
|--------------------|---------------------------------------------|------------|
| `tedge-agent`      | `tedge.exe run tedge-agent`                 | Automatic  |
| `tedge-mapper-c8y` | `tedge.exe run tedge-mapper c8y`            | Automatic  |

Both services SHALL run under the `LocalSystem` account.

#### Scenario: services present after install
- **WHEN** the MSIX is installed
- **THEN** `sc query tedge-agent` and `sc query tedge-mapper-c8y` SHALL both return a result without error

#### Scenario: services absent before install
- **WHEN** the MSIX has not been installed on the machine
- **THEN** `sc query tedge-agent` SHALL return a service-not-found error

---

### Requirement: services start automatically on boot

Both `tedge-agent` and `tedge-mapper-c8y` SHALL be configured with `StartupType="auto"`, causing Windows to start them automatically during system startup without manual intervention.

#### Scenario: services start on reboot
- **WHEN** the machine is rebooted after the MSIX is installed
- **THEN** `tedge-agent` and `tedge-mapper-c8y` SHALL reach the `RUNNING` state without manual `sc start` commands

---

### Requirement: services use the multi-call binary pattern

Each service SHALL invoke `tedge.exe` with `run <service-name>` as its command-line arguments. No separate per-service binary or service wrapper (winsw, NSSM) is used.

#### Scenario: tedge-agent invocation
- **WHEN** Windows starts the `tedge-agent` service
- **THEN** the process image SHALL be `tedge.exe` and the command-line SHALL include `run tedge-agent`

#### Scenario: tedge-mapper-c8y invocation
- **WHEN** Windows starts the `tedge-mapper-c8y` service
- **THEN** the process image SHALL be `tedge.exe` and the command-line SHALL include `run tedge-mapper c8y`

---

### Requirement: services manageable via Windows Service Control Manager

Both services SHALL be controllable through standard Windows Service Control Manager interfaces: `sc.exe`, PowerShell `*-Service` cmdlets, and the Services MMC snap-in.

#### Scenario: stop a service via sc.exe
- **WHEN** a user runs `sc stop tedge-agent`
- **THEN** the `tedge-agent` service SHALL transition to the `STOPPED` state

#### Scenario: start a service via sc.exe
- **WHEN** the `tedge-agent` service is stopped and a user runs `sc start tedge-agent`
- **THEN** the `tedge-agent` service SHALL transition to the `RUNNING` state

#### Scenario: restart a service via PowerShell
- **WHEN** a user runs `Restart-Service tedge-mapper-c8y`
- **THEN** the `tedge-mapper-c8y` service SHALL stop and restart, returning to the `RUNNING` state

---

### Requirement: services unregistered on package removal

When the MSIX is uninstalled, Windows SHALL automatically unregister both `tedge-agent` and `tedge-mapper-c8y` from the Service Control Manager. No manual `sc delete` step is required.

#### Scenario: services absent after uninstall
- **WHEN** the MSIX is uninstalled
- **THEN** `sc query tedge-agent` and `sc query tedge-mapper-c8y` SHALL return a service-not-found error

---

### Requirement: service identity matches thin-edge health topic convention

The Windows Service name for each service SHALL match the thin-edge health topic identity used on Linux, so that service names are consistent across platforms and existing cloud-side integrations that reference service names by string do not require changes.

| Windows service name | thin-edge health topic                                        |
|----------------------|---------------------------------------------------------------|
| `tedge-agent`        | `te/device/main/service/tedge-agent/status/health`            |
| `tedge-mapper-c8y`   | `te/device/main/service/tedge-mapper-c8y/status/health`       |

#### Scenario: health topic uses Windows service name
- **WHEN** `tedge-agent` is running on Windows
- **THEN** it SHALL publish health status on `te/device/main/service/tedge-agent/status/health`, matching the Linux service name
