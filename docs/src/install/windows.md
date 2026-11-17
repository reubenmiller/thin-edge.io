---
title: Windows
tags: [Installation, Windows]
sidebar_position: 2
description: Install and run %%te%% on Windows
---

%%te%% runs on Windows 10 (version 1809 or later) and Windows Server 2019 or later,
on x64 and arm64.
The components run as Windows Services,
use Mosquitto for Windows as the local MQTT broker,
and connect to the cloud with the built-in MQTT bridge.

## Prerequisites

Install Mosquitto for Windows.
Its installer registers the `mosquitto` Windows Service, listening on `127.0.0.1:1883`.

```powershell
winget install EclipseFoundation.Mosquitto
Start-Service mosquitto
```

All the commands below run in a PowerShell window started as administrator.

## Install

Download the ZIP package of your architecture, extract it, and run the install script:

```powershell
.\install.ps1
```

The script:

- copies `tedge.exe` to `C:\Program Files\tedge\bin` and adds it to the system `PATH`,
- creates the configuration directory `C:\ProgramData\tedge`,
  and restricts it to `SYSTEM` and `Administrators`,
- writes a default `tedge.toml` with the built-in bridge enabled,
- registers the `tedge-agent`, `tedge-mapper-c8y` and `tedge` Windows Services.

The MSIX package registers the same services.

## Connect to Cumulocity

```powershell
tedge config set c8y.url example.cumulocity.com
tedge cert create --device-id my-windows-gateway
tedge cert upload c8y --user "<username>"
tedge connect c8y
```

`tedge connect` enables and starts the `tedge-mapper-c8y` service.
Only the built-in bridge (`mqtt.bridge.built_in = true`) is supported on Windows:
`tedge connect` fails if it is disabled.

## Services

| Service | Command line | Purpose |
|---|---|---|
| `tedge-agent` | `tedge.exe run tedge-agent` | Agent: operations and workflows |
| `tedge-mapper-c8y` | `tedge.exe run tedge-mapper c8y` | Cumulocity mapper and built-in bridge |
| `tedge` | `tedge.exe run all` | Agent and mappers in one process. Use it instead of the two services above. |

Manage them with the usual cmdlets:

```powershell
Get-Service tedge-agent, tedge-mapper-c8y
Restart-Service tedge-mapper-c8y
```

Stopping a service publishes the health status `down` and closes the MQTT connections before the process exits.
Each service writes its log to `C:\ProgramData\tedge\log\<service-name>.log`.

## Files and directories

| Path | Content |
|---|---|
| `C:\ProgramData\tedge\tedge.toml` | Configuration |
| `C:\ProgramData\tedge\device-certs` | Device certificate and private key |
| `C:\ProgramData\tedge\operations` | Workflows and Cumulocity supported operations |
| `C:\ProgramData\tedge\sm-plugins` | Software management plugins |
| `C:\ProgramData\tedge\config-plugins`, `log-plugins` | Configuration and log plugins |
| `C:\ProgramData\tedge\log` | Service and operation logs |

On Windows, file ownership and Unix modes (`user`, `group`, `mode` settings) are not applied.
Access is controlled by the ACL of `C:\ProgramData\tedge`,
which grants access to `SYSTEM` and `Administrators` only.
The ACL is set when the directory is created;
the `tedge` CLI must therefore run in an elevated PowerShell window.

### Child devices

A child device's supported operations live in a directory named after its external ID.
Windows does not allow `:` and other characters in file names,
so on Windows these characters are percent-encoded:
the operations of the child device `my-gateway:device:child1` are in
`C:\ProgramData\tedge\operations\c8y\my-gateway%3Adevice%3Achild1`.

The encoded characters are `%` (`%25`), `<` (`%3C`), `>` (`%3E`), `:` (`%3A`), `"` (`%22`),
`/` (`%2F`), `\` (`%5C`), `|` (`%7C`), `?` (`%3F`) and `*` (`%2A`).

## Plugins

Plugins are recognised by their extension: `.exe`, `.bat`, `.cmd`, `.ps1` or `.com`.
Other files in a plugin directory are ignored.
The extension is not part of the plugin name:
`C:\ProgramData\tedge\sm-plugins\winget.ps1` provides the `winget` software type.

`.bat` and `.cmd` plugins run with `cmd.exe /c`,
`.ps1` plugins with `powershell.exe -NoProfile -ExecutionPolicy Bypass -File`.

## Differences with Linux

| Feature | Windows |
|---|---|
| `tedge` CLI, `tedge-agent`, `tedge-mapper` (`c8y`, `aws`, `az`, custom mappers, flows) | Supported |
| Built-in MQTT bridge | Supported, required |
| Mosquitto bridge (`mqtt.bridge.built_in = false`) | Not supported |
| `tedge run all` | Supported, as the `tedge` service |
| `tedge-file-config-plugin`, `tedge-file-log-plugin`, `tedge-flows-plugin` | Supported. The `file` plugins are `file.cmd` wrappers regenerated at each service start. |
| Device restart | Supported. The default `system.reboot` command is `shutdown /r /t 0`. |
| `c8y-firmware-plugin` | Supported |
| `c8y-remote-access-plugin` | Supported. One process per session: no reuse of a running instance. |
| `tedge-apt-plugin` | Not supported. Use the winget plugin. |
| `shell_execute` operation (`c8y_Command`) | Not supported. The agent does not register it. |
| PKCS#11 module loaded by the mapper (`device.cryptoki.mode = "module"`) | Supported |
| PKCS#11 socket proxy (`tedge-p11-server`) | Not supported |
| SIGHUP log level reload, SIGUSR1 mapper restart | Not supported. Restart the service. |
| `tedge init` symbolic links | Not needed: components run as `tedge.exe run <component>` |
| Graceful termination on timeout | Console scripts cannot be asked to close: they are killed, with their child processes, when the timeout expires, without the extra time a Unix script gets after SIGTERM. |
| File ownership and modes | Replaced by the ACL of `C:\ProgramData\tedge` |
