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

Mosquitto for Windows does not keep retained messages when it restarts.
%%te%% publishes the capabilities of the device as retained messages,
so enable persistence in `C:\Program Files\mosquitto\mosquitto.conf`:

```text
persistence true
persistence_location C:\ProgramData\mosquitto\
```

%%te%% does not change the Mosquitto configuration on Windows:
`tedge connect` and `tedge disconnect` do not restart Mosquitto.

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
- registers the `tedge-agent`, `tedge-mapper-c8y`, `tedge-mapper-local` and `tedge` Windows Services.

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
| `tedge-mapper-local` | `tedge.exe run tedge-mapper local` | User-defined flows in `C:\ProgramData\tedge\mappers\local\flows`. Not started by default. |
| `tedge` | `tedge.exe run all` | Agent and mappers in one process. Use it instead of the two services above. |

Manage them with the usual cmdlets:

```powershell
Get-Service tedge-agent, tedge-mapper-c8y
Restart-Service tedge-mapper-c8y
```

Stopping a service publishes the health status `down` and closes the MQTT connections before the process exits.
Each service writes its log to `C:\ProgramData\tedge\log\<service-name>.log`.
A service that crashes or exits with an error is restarted after 5 seconds, twice,
then every 60 seconds; the count is reset after a day without failure.
Change this with `sc.exe failure <service-name>`.

Only one instance of each service runs at a time:
a second `tedge-agent` or mapper of the same cloud exits with the error `Another instance of ... is running`.
The lock files are in `C:\ProgramData\tedge\run\lock` (`run.path`).

## Files and directories

| Path | Content |
|---|---|
| `C:\ProgramData\tedge\tedge.toml` | Configuration |
| `C:\ProgramData\tedge\device-certs` | Device certificate and private key |
| `C:\ProgramData\tedge\ca-certificates` | Extra trusted root certificates (empty by default) |
| `C:\ProgramData\tedge\operations` | Workflows and Cumulocity supported operations |
| `C:\ProgramData\tedge\sm-plugins` | Software management plugins |
| `C:\ProgramData\tedge\config-plugins`, `log-plugins` | Configuration and log plugins |
| `C:\ProgramData\tedge\diag-plugins` | Diagnostic plugins run by `tedge diag collect` |
| `C:\ProgramData\tedge\run\lock` | Lock files preventing a second instance of a service |
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

thin-edge.io installs these plugins and replaces them on each upgrade:

| Plugin | Purpose |
|---|---|
| `log-plugins\file.cmd` | Log files declared in `plugins\tedge-log-plugin.toml`. The thin-edge.io service logs are declared by default (types `tedge-agent`, `tedge-mapper` and `tedge`). |
| `log-plugins\eventlog.ps1` | The `System`, `Application` and `Setup` event logs (types `System::eventlog`, …), the Windows counterpart of the `journald` plugin |
| `config-plugins\file.cmd` | Configuration files declared in `plugins\tedge-configuration-plugin.toml`, including `mosquitto.conf` |
| `diag-plugins\01_tedge.ps1` … `08_truststore.ps1` | `tedge diag collect`: services and their logs, configuration, OS, MQTT messages, workflows, entities, Mosquitto and trusted certificates |

`sm-plugins\winget.ps1` is installed only if it is absent, so local changes are kept.

## Remote access

The `c8y-remote-access-plugin` connects Cumulocity Cloud Remote Access to any TCP service of the device.
To give access to the Windows host itself, add an endpoint in Cumulocity for one of these services:

| Protocol | Endpoint | Enable it on Windows |
|---|---|---|
| RDP | `127.0.0.1:3389` | Settings > System > Remote Desktop |
| SSH | `127.0.0.1:22` | `Add-WindowsCapability -Online -Name OpenSSH.Server~~~~0.0.1.0; Start-Service sshd` |
| VNC | `127.0.0.1:5900` | A VNC server, such as TightVNC |

## Shell commands

The `shell_execute` operation (the Cumulocity shell, `c8y_Command`) runs commands with
`powershell.exe -NoProfile -NonInteractive -Command <command>`, as `SYSTEM`.
The output is captured as UTF-8.
A command still running after `shell.timeout` is terminated with all the processes it started.

To use another shell, set `shell.path`:

| `shell.path` | Command run |
|---|---|
| `C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe` (default) | `powershell.exe -NoProfile -NonInteractive -Command <command>` |
| `C:\Program Files\PowerShell\7\pwsh.exe` | `pwsh.exe -NoProfile -NonInteractive -Command <command>` |
| `C:\Windows\System32\cmd.exe` | `cmd.exe /d /s /c "<command>"` |
| Any other shell, such as Git Bash | `<shell> -c -- <command>` |

A command restarting the agent or the device declares its result first with
`tedge run tedge-shell-plugin set-result --outcome successful`,
as there is no `tedge-shell-plugin` executable on Windows.

## Update thin-edge.io from Cumulocity

thin-edge.io updates itself with the software type `tedge`, provided by `sm-plugins\tedge.ps1`.
In the Cumulocity software repository, create the software `tedge` with the type `tedge`,
and a version whose file is the installer ZIP (`tedge_<version>_<arch>-installer.zip`) or a `tedge.exe`.
Installing it on the device:

1. replaces `C:\Program Files\tedge\bin\tedge.exe`, renaming the running one to `tedge.exe.old`,
   as a running executable cannot be overwritten;
2. updates the diagnostic, Event Log and `tedge` plugins from the installer ZIP;
3. restarts the running mapper services;
4. reports the operation, after which `tedge-agent` detects the new version,
   exits, and is restarted by the Service Control Manager with the new `tedge.exe`.

This applies to an installation made with `install.ps1`.
An MSIX installation is updated by installing the new MSIX package.

## Shell completion

Load the PowerShell completion of `tedge` in the current session,
or add this line to your PowerShell profile (`$PROFILE`) to load it in every session:

```powershell
tedge completions powershell | Out-String | Invoke-Expression
```

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
| `shell_execute` operation (`c8y_Command`) | Supported. Commands run in Windows PowerShell by default (see [Shell commands](#shell-commands)). |
| PKCS#11 module loaded by the mapper (`device.cryptoki.mode = "module"`) | Supported |
| PKCS#11 socket proxy (`tedge-p11-server`) | Not supported |
| SIGHUP log level reload, SIGUSR1 mapper restart | Not supported. Restart the service. |
| `tedge init` symbolic links | Not needed: components run as `tedge.exe run <component>` |
| Graceful termination on timeout | Console scripts cannot be asked to close: they are killed, with their child processes, when the timeout expires, without the extra time a Unix script gets after SIGTERM. |
| File ownership and modes | Replaced by the ACL of `C:\ProgramData\tedge` |
| Trusted root certificates | The Windows certificate store is always trusted. `c8y.root_cert_path`, `az.root_cert_path` and `aws.root_cert_path` default to `C:\ProgramData\tedge\ca-certificates` instead of `/etc/ssl/certs`; add PEM files there to trust extra authorities. |
