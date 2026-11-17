## ADDED Requirements

### Requirement: Supported Windows versions and architectures

thin-edge.io SHALL support Windows 10 version 1809 or later and Windows Server 2019 or later,
on the `x86_64-pc-windows-msvc` and `aarch64-pc-windows-msvc` targets.

#### Scenario: Build for both architectures
- **WHEN** CI builds the `tedge` package for `x86_64-pc-windows-msvc` and `aarch64-pc-windows-msvc`
- **THEN** both builds succeed and produce `tedge.exe`

### Requirement: Windows support matrix

The documentation SHALL list every thin-edge.io component and feature with one of three statuses on Windows:
supported, supported with documented differences, or unsupported.
The matrix SHALL contain at least:

| Feature | Status on Windows |
|---|---|
| `tedge` CLI (`config`, `cert`, `connect`, `mqtt`, `http`, `diag`, `flows`) | supported |
| `tedge-agent` (software, configuration, log, restart, device profile, firmware operations, workflows) | supported |
| `tedge-mapper` (`c8y`, `aws`, `az`, custom mappers, flows) | supported |
| Built-in MQTT bridge (`mqtt.bridge.built_in = true`) | supported |
| Mosquitto bridge (`mqtt.bridge.built_in = false`) | unsupported |
| `tedge run all` | supported |
| `tedge-file-config-plugin`, `tedge-file-log-plugin`, `tedge-flows-plugin` | supported |
| `c8y-remote-access-plugin` | supported with differences: one process per session, no socket reuse |
| `c8y-firmware-plugin` | supported |
| `tedge-apt-plugin` | unsupported |
| `shell_execute` operation (`tedge-shell-plugin`) | unsupported |
| PKCS#11 direct module access | supported |
| PKCS#11 socket proxy (`tedge-p11-server`) | unsupported |
| SIGHUP log level reload, SIGUSR1 mapper restart | unsupported: restart the service instead |
| `tedge init` symlinks to the multicall binary | unsupported: not needed, components run as `tedge run <component>` |
| Ownership and mode of files (`user`, `group`, `mode`) | supported with differences: directory ACL, see access control requirement |
| Graceful termination on timeout | supported with differences: console processes are killed at the forceful timeout |

#### Scenario: Matrix covers every binary
- **WHEN** a new component binary or multicall entry is added to the `tedge` package
- **THEN** the support matrix lists its Windows status

### Requirement: Configuration and data locations

On Windows the default configuration directory SHALL be `C:\ProgramData\tedge`,
with `data`, `log`, `tmp`, `sm-plugins`, `config-plugins` and `log-plugins` below it.
`tedge.toml` SHALL store paths with forward slashes in TOML literal strings, so Windows paths are not read as escape sequences.

#### Scenario: First run creates the directory tree
- **WHEN** any `tedge` component runs for the first time on a machine without `C:\ProgramData\tedge`
- **THEN** the directory and its sub-directories exist
- **AND** `tedge.toml` sets `data.path`, `logs.path`, `tmp.path`, `configuration.plugin_paths` and `log.plugin_paths` below it

#### Scenario: Existing configuration is preserved
- **WHEN** a component starts and `C:\ProgramData\tedge\tedge.toml` exists
- **THEN** the file content is unchanged

### Requirement: MQTT broker prerequisite

thin-edge.io on Windows SHALL use an MQTT broker listening on the configured `mqtt.client.host` and `mqtt.client.port`
(default `127.0.0.1:1883`), and SHALL connect to clouds through the built-in bridge.
Mosquitto for Windows SHALL be the documented and tested broker.

#### Scenario: Connecting with the built-in bridge
- **WHEN** Mosquitto runs as a Windows Service, `mqtt.bridge.built_in` is `true`, and the user runs `tedge connect c8y`
- **THEN** the mapper connects to the cloud and `tedge connect c8y --test` succeeds

#### Scenario: Mosquitto bridge is rejected
- **WHEN** `mqtt.bridge.built_in` is `false` and the user runs `tedge connect c8y` on Windows
- **THEN** the command fails with an error stating that the Mosquitto bridge is not supported on Windows and that `mqtt.bridge.built_in` must be `true`

### Requirement: Access control of the configuration directory

On Windows, when the configuration directory is created, thin-edge.io SHALL restrict it so that only `SYSTEM` (S-1-5-18)
and `BUILTIN\Administrators` (S-1-5-32-544) have access, with full control inherited by all files and sub-directories,
and without permissions inherited from the parent directory.
An existing configuration directory SHALL NOT have its ACL changed by a component at startup.

#### Scenario: Private key is not readable by standard users
- **WHEN** `tedge cert create` writes the device private key below `C:\ProgramData\tedge`
- **THEN** a non-administrator user cannot read the key file

#### Scenario: ACL applied with SIDs
- **WHEN** the configuration directory is created on a Windows installation in a language other than English
- **THEN** the ACL is applied, because principals are identified by SID and not by name

### Requirement: Child-device operation directories on Windows

On Windows, a child-device directory under the Cumulocity operations directory SHALL be named
after the device external ID with the characters `%`, `<`, `>`, `:`, `"`, `/`, `\`, `|`, `?`, `*`
percent-encoded as `%` followed by two upper-case hexadecimal digits.
The mapper SHALL decode directory names to external IDs when it reads the operations directory.
On other platforms, directory names SHALL be the raw external IDs.

#### Scenario: Supported operations of a child device
- **WHEN** the mapper registers supported operations for the child device `main:device:child1` on Windows
- **THEN** the operation files are created in `operations\c8y\main%3Adevice%3Achild1`
- **AND** the mapper publishes the supported operations for `main:device:child1`

#### Scenario: Directory created by a user
- **WHEN** a user creates `operations\c8y\main%3Adevice%3Achild2\c8y_Restart` on Windows
- **THEN** the mapper declares `c8y_Restart` as supported by the child device `main:device:child2`

#### Scenario: Linux layout unchanged
- **WHEN** the mapper registers supported operations for `main:device:child1` on Linux
- **THEN** the operation files are created in `operations/c8y/main:device:child1`

### Requirement: Unsupported features fail explicitly

A feature listed as unsupported on Windows SHALL fail with an error message that names the feature and states it is not supported on this platform.
It SHALL NOT be advertised to the cloud.

#### Scenario: shell_execute is not registered
- **WHEN** `tedge-agent` starts on Windows with `agent.enable.shell_execute = true`
- **THEN** the agent does not register the `shell_execute` operation
- **AND** the Cumulocity mapper does not declare `c8y_Command`

#### Scenario: Unsupported binary
- **WHEN** a user runs `tedge-p11-server.exe` or `tedge-shell-plugin.exe` on Windows
- **THEN** the process exits with a non-zero code and an error stating the component is not supported on this platform

### Requirement: Device restart on Windows

On Windows, the default `system.reboot` command SHALL be `shutdown /r /t 0`.
After the agent restarts, it SHALL determine whether the device rebooted by comparing the time the restart was requested
with the last boot time reported by `Win32_OperatingSystem`, since the agent `tmp` directory is not cleared on reboot.

#### Scenario: Restart operation succeeds after a reboot
- **WHEN** a restart operation reboots the Windows device and the agent starts again
- **THEN** the restart operation is reported as successful

#### Scenario: Agent restarted without a reboot
- **WHEN** the agent restarts during a restart operation but Windows did not reboot
- **THEN** the restart operation is reported as failed with "The agent has been restarted but not the device"

### Requirement: Windows file names

Files created by thin-edge.io on Windows SHALL NOT contain characters that are invalid in Windows file names.
Timestamps embedded in file names SHALL use `-` instead of `:`.

#### Scenario: Operation log file
- **WHEN** the agent creates a log file for an operation on Windows
- **THEN** the file name contains no `:` character
