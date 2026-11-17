## Why

thin-edge.io builds and passes most unit tests on Windows (branch `feat/windows-port-phase1`),
but the port stops at "it compiles".
Several features fail silently on Windows,
the agent and mappers cannot be stopped cleanly as Windows Services,
child-device operations break because external IDs contain `:`,
private keys are readable by every local user,
and nothing tests thin-edge.io end to end on Windows.
Industrial customers run Windows gateways and IPCs,
so Windows needs to be a supported platform with a defined feature set and CI coverage,
not a best-effort build.

## Proposed solution

thin-edge.io runs on Windows 10 / Windows Server 2019 and later (x64 and arm64)
as Windows Services, with Mosquitto for Windows as the local MQTT broker
and the built-in MQTT bridge for cloud connectivity:

```powershell
# Mosquitto is a prerequisite
winget install EclipseFoundation.Mosquitto

# Configure and connect, as on Linux
tedge config set c8y.url example.cumulocity.com
tedge config set mqtt.bridge.built_in true
tedge cert create --device-id my-windows-gateway
tedge connect c8y

# Services are managed by the Windows Service Control Manager
Get-Service tedge-agent, tedge-mapper-c8y
Restart-Service tedge-agent
```

Plugins are written as `.ps1`, `.cmd`, `.bat` or `.exe` files.
The extension is not part of the plugin name,
so `C:\ProgramData\tedge\sm-plugins\winget.ps1` provides the `winget` software type,
exactly as `/etc/tedge/sm-plugins/winget` would on Linux.

Features that depend on Linux-only facilities are reported as unsupported
instead of failing at runtime:
`tedge-apt-plugin`, the `shell_execute` operation, the PKCS#11 socket proxy (`tedge-p11-server`),
the Mosquitto bridge configuration (`mqtt.bridge.built_in = false`),
and SIGHUP / SIGUSR1 reloads.

The full unit test suite and a Windows Robot Framework suite run on every pull request
on a `windows-latest` GitHub Actions runner.

## What Changes

- A documented Windows support matrix:
  every thin-edge.io component and feature is listed as supported, supported with differences, or unsupported.
- Windows Service integration: `tedge-agent`, `tedge-mapper-<cloud>` and `tedge run all`
  run under the Service Control Manager.
  A Stop request drains the components through the supervisor,
  so health status `down` is published and MQTT connections close cleanly.
  Console Ctrl+C, Ctrl+Break, close and shutdown events trigger the same graceful shutdown.
- Process control: workflow scripts, operation plugins and custom operations that exceed their timeout
  are terminated with their whole process tree (`taskkill /T`, then `taskkill /T /F`),
  replacing the current "wait and give up" behavior.
- Plugin discovery: plugins are recognised by extension (`.exe`, `.bat`, `.cmd`, `.ps1`, `.com`, case-insensitive),
  the extension is stripped from the plugin name,
  and scripts are launched through their interpreter.
  One shared implementation replaces three copies.
- Child devices: external IDs are encoded when used as directory names under `operations/c8y`
  (`:` becomes `%3A`), and decoded when read back.
  Linux keeps the current layout.
- File access control: the configuration directory `C:\ProgramData\tedge` is restricted
  to `SYSTEM` and `Administrators`,
  the Windows equivalent of the `tedge:tedge` ownership and `0o750`/`0o600` modes used on Linux.
- Unsupported features fail with an explicit "not supported on Windows" error;
  the agent does not register `shell_execute` on Windows.
- CI: the Windows workflow runs on pull requests and `main`,
  runs `cargo clippy` and the unit tests for the Windows target with no warnings,
  and runs the Windows Robot Framework suite.
- Robot Framework: the `local` device adapter and the `ThinEdgeIO` library gain Windows support,
  and a `tests/RobotFramework/tests/windows/` suite covers installation, services, telemetry, operations and plugins.

## Capabilities

### New Capabilities

- `windows-platform-support`: the Windows support matrix, configuration and data locations,
  the MQTT broker prerequisite, file access control, child-device directory encoding,
  and how unsupported features behave.
- `windows-process-control`: Windows Service lifecycle, console events, graceful shutdown,
  and termination of process trees on timeout.
- `windows-plugins`: discovery, naming and invocation of software, configuration, log and diagnostic plugins on Windows.
- `windows-system-tests`: unit test and lint coverage of the Windows target in CI,
  and the Robot Framework suite running on a Windows host.

### Modified Capabilities

- `single-process-supervisor`: the supervisor accepts shutdown requests that do not arrive as Unix signals
  (Windows console events and Service Control Manager stop requests).
- `standalone-supervisor`: same as above for standalone `tedge-agent` and `tedge-mapper` processes.

## Impact

- Code: `tedge_supervisor`, `tedge_utils` (`signals`, `file`, new `plugins` module), `tedge_api` (`logged_command`),
  `tedge_script_ext`, `plugin_sm`, `tedge_log_manager`, `tedge_config_manager`, `c8y_mapper_ext` (`supported_operations`),
  `tedge` (`main`, `windows_service_control`, `windows_init`, `init`, `diag`), `tedge_agent`, `tedge-shell-plugin`, `tedge-p11-server`.
- No change of behavior on Linux or macOS.
- New dependency: none beyond `windows-service` and `tracing-appender` introduced by phase 1.
- CI: `.github/workflows/ci-windows.yml` runs on every pull request (adds roughly 25 minutes of runner time).
- Robot Framework: `ThinEdgeIO` library, a Windows-aware local adapter, a new Windows suite.
- Related changes: `windows-msi-packaging` (MSIX package and service registration)
  and `windows-sm-plugin` (winget software management plugin) are unchanged and complementary.
