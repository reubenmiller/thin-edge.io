# Windows feature inventory

Every thin-edge.io feature that is missing or incomplete on Windows,
with an estimate of whether to port it.
Fill in the **Decision** column; the estimates are suggestions.

Estimate values:

- **Port now**: needed for a credible Windows release, or cheap enough to include in this change.
- **Port later**: worth doing, in a separate change.
- **Don't port**: an existing Windows feature covers it, or the cost is out of proportion.
- **N/A**: has no meaning on Windows.

Effort: **S** up to a day, **M** a few days, **L** a week or more.

## Missing features

| # | Feature | Today on Windows | Estimate | Effort | Reasoning | Decision |
|---|---|---|---|---|---|---|
| 1 | `shell_execute` operation (`c8y_Command`, Cumulocity shell) | Not registered by the agent | Port now | M | One of the most used Cumulocity device features. A non-interactive command needs no pseudo-terminal: run `powershell -Command` or `cmd /c`, read the output with threads instead of `poll`, and stop it with `taskkill /T`, which already exists. The Unix code relies on process groups and `flock`, so the Windows code is a separate implementation. | |
| 2 | Service recovery actions (restart a crashed service) | None: a crashed service stays stopped | Port now | S | Replaces the systemd `Restart=` of the Linux units. `sc.exe failure <service> reset= 86400 actions= restart/5000/restart/5000/restart/30000` in `install.ps1`. | |
| 3 | Single-instance lock (`flockfile`) | Present, but does not lock: Rust opens files in shared mode, so a second agent or mapper starts | Port now | S | A bug, not a gap. Use `File::try_lock` (stable since Rust 1.89). | |
| 4 | Diagnostic plugins (`01_tedge.sh` … `08_truststore.sh`) | `tedge diag collect` runs, but finds no plugins | Port now | M | First thing support asks for. Rewrite in PowerShell: versions, services, `tedge config list`, MQTT snapshot, workflows, entities, Mosquitto, trust store. | |
| 5 | Windows Event Log as a log type | No equivalent of the `journald` and `dmesg` log plugins | Port now | S | A `log-plugins\eventlog.ps1` plugin using `Get-WinEvent` (System, Application, and the service's provider). | |
| 6 | Default log types for the thin-edge.io service logs | Only `software-management` is declared | Port now | S | Add `C:\ProgramData\tedge\log\tedge-agent.log`, `tedge-mapper-*.log` and `tedge.log` to the generated `tedge-log-plugin.toml` on Windows. | |
| 7 | Default configuration type for `mosquitto.conf` | Not declared | Port now | S | Add `C:\Program Files\mosquitto\mosquitto.conf` to the generated `tedge-configuration-plugin.toml` on Windows. | |
| 8 | Self-update of thin-edge.io from Cumulocity | Untested; the running `tedge.exe` and its services are replaced by the update | Port now | M–L | Fleet updates are a core use case. An MSIX or winget upgrade stops the services that run the update; it needs a workflow that restarts and reports the result after the upgrade, like the Linux `tedge_upgrade` flow. | |
| 9 | `root_cert_path` default for custom mappers (`mapper.toml`) | `/etc/ssl/certs` | Port now | S | Task 6.15. Same fix as the built-in mappers. | |
| 10 | PowerShell completions (`tedge completions`) | Bash, zsh and fish only | Port now | S | `clap_complete` already generates PowerShell completions. | |
| 11 | Windows on ARM64 | Built and packaged, never run | Port now | S | Add the `windows-11-arm` GitHub runner to the unit and system test jobs. | |
| 12 | Log level reload without a restart (SIGHUP) | Not supported: restart the service | Port later | S | Handle the SCM `ParamChange` control (supported by the `windows-service` crate) or watch `system.toml`. A restart works in the meantime. | |
| 13 | Local broker configuration managed by thin-edge.io (`mosquitto-conf`: listeners, local TLS, client authentication, persistence) | Ignored: the user edits `mosquitto.conf` by hand | Port later | M | Generate an include file and document one `include_dir` line in `mosquitto.conf`. Would also turn persistence on by default, so retained capabilities survive a broker restart. Then `tedge connect` would need to restart Mosquitto again. | |
| 14 | MQTT broker installed with thin-edge.io | Prerequisite installed by the user | Decide | M | Options: keep Mosquitto a prerequisite; have `install.ps1` install it with winget; or embed a broker. MSIX cannot declare a winget dependency. | |
| 15 | Windows system metrics (CPU, memory, disk, network) | None (`collectd` has no Windows build) | Port later | M | Replaces the collectd mapper on Windows. A flow or a small script reading performance counters and publishing `te/.../m/...` measurements. | |
| 16 | Dedicated service account instead of `LocalSystem` | All services run as `LocalSystem` | Port later | M | Smaller attack surface for the mapper. But software installation and reboot need administrator rights, so the agent would need a privileged helper, like `sudo` on Linux. | |
| 17 | OS updates (Windows Update) as an operation | Not available | Port later | L | The firmware plugin works but has no meaning for a Windows host. Patch management through the Windows Update API is a new feature, not a port. | |
| 18 | Windows versions and editions | Tested only on the Windows Server image of the GitHub `windows-latest` runner | Port later | S per target | Likely targets: Windows 10/11 IoT Enterprise (LTSC), Windows Server 2019+. Server Core has no MSIX support before 2019. | |
| 19 | System proxy settings (WinHTTP or Internet Options) | Unverified: `tedge.toml` proxy and environment variables work | Port later | S | Check whether the HTTP client reads the Windows system proxy; if not, document `proxy.address`. | |
| 20 | Remote access to Windows targets (RDP, Windows OpenSSH) | Works through `c8y-remote-access-plugin`, not documented | Port now | S | Documentation only: the passthrough protocol is generic. | |
| 21 | Mosquitto bridge (`mqtt.bridge.built_in = false`) | Not supported: explicit error | Don't port | — | The built-in bridge does the same job and needs no broker restart. | |
| 22 | PKCS#11 socket proxy (`tedge-p11-server`) | Not supported: explicit error | Don't port | L | Would need a named-pipe transport. Loading the PKCS#11 module directly (`device.cryptoki.mode = "module"`) works on Windows. | |
| 23 | Mapper restart signal (SIGUSR1) | Not supported | Don't port | — | `Restart-Service` does the same. | |
| 24 | `tedge-watchdog` (systemd watchdog) | Does nothing | Don't port | — | Replaced by SCM recovery actions (#2). | |
| 25 | Per-file ownership and modes (`user`, `group`, `mode`) | Ignored; the directory ACL applies | Don't port | L | Mapping Unix owners to Windows principals adds little over the directory ACL. | |
| 26 | `tedge-apt-plugin` | Not available | N/A | — | The winget plugin replaces it. | |
| 27 | `tedge init` (symbolic links, `tedge` user and group) | Not needed | N/A | — | Components run as `tedge.exe run <component>`; directories are created at service start. | |
| 28 | Init system plugins (systemd, OpenRC, BSD) | Not needed | N/A | — | `tedge` controls Windows services through the SCM. | |
| 29 | `sudo` for plugins and `tedge-write` | Not needed | N/A | — | Services run as `LocalSystem` (see #16). | |
| 30 | OS image updates (Rugix, Mender) | Not available | N/A | — | Linux image-based update systems. | |

## Test gaps on features that already work

These features work on Windows, but some of their tests are ignored there.

| # | Area | Gap | Effort | Task |
|---|---|---|---|---|
| 31 | Tests with hard-coded `/etc/tedge` paths | Ignored on Windows | M | 7.1 |
| 32 | TLS rejection tests | Windows reports `WSAECONNABORTED` | S | 7.2 |
| 33 | File watching (`notify`, `ReadDirectoryChangesW`) | Events differ from inotify; tests ignored | M | 7.3 |
| 34 | All other `cfg_attr(windows, ignore)` | Not audited | M | 7.4 |

## Already supported

For reference, these features run on Windows and are covered by the Windows CI:
`tedge` CLI (`config`, `cert`, `mqtt`, `connect`, `disconnect`, `reconnect`, `http`, `upload`, `diag`, `hsm`),
`tedge-agent`, the Cumulocity, Azure, AWS, local and custom mappers, flows,
the built-in bridge, `tedge run all`, Windows services,
the file transfer service, child devices, health status, device restart, workflows,
configuration and log management (`file` plugins), software management (winget),
`c8y-firmware-plugin`, `c8y-remote-access-plugin`, the Cumulocity proxy, and PKCS#11 in module mode.
