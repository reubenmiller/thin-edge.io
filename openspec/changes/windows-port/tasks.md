## 1. Rebase phase 1 onto main

- [x] 1.1 Rebase `feat/windows-port-phase1` onto `main` (squashed) and resolve conflicts with the `Utf8Path`, `tracing` and supervisor changes
- [x] 1.2 Make new `main` crates build for Windows: `tedge_supervisor`, `tedge-shell-plugin`, `tedge_test_utils`
- [x] 1.3 `cargo clippy --workspace --all-targets --all-features --target x86_64-pc-windows-gnu` passes with no warnings

## 2. Process control

- [x] 2.1 Add `tedge_supervisor::request_shutdown()` and a Windows listener for Ctrl+C, Ctrl+Break, close and shutdown events
- [x] 2.2 Windows Service `Stop`/`Shutdown` requests call `request_shutdown()`, report `StopPending`, then `Stopped` with the exit code; 30 s fallback exit
- [x] 2.3 Run `tedge run all` as the `tedge` Windows Service; one `windows_service_name()` for dispatcher and registration
- [x] 2.4 Terminate process trees on timeout with `taskkill /T` and `taskkill /T /F` in `tedge_utils::signals` and `tedge_script_ext`
- [x] 2.5 Start `LoggedCommand` and `ScriptActor` commands in their own process group on Unix only
- [x] 2.6 Unit test: `request_shutdown()` before and after the supervisor starts drains all units

## 3. Plugins

- [x] 3.1 Add `tedge_utils::plugins` (`plugin_name`, `is_plugin_candidate`, `has_windows_plugin_extension`) with case-insensitive extensions
- [x] 3.2 Use it in `plugin_sm`, `tedge_log_manager`, `tedge_config_manager` and `tedge diag collect`
- [x] 3.3 Test helper creating a platform-specific plugin script (`.cmd` on Windows, shell script on Unix) in `tedge_test_utils`
- [x] 3.4 Re-enable the log manager and configuration manager tests ignored because of bash plugins

## 4. Platform support

- [x] 4.1 Percent-encode child-device external IDs in `operations/c8y` directory names on Windows and decode them in `xid_from_path`
- [x] 4.2 Re-enable the child-device tests in `c8y_mapper_ext` ignored on Windows
- [x] 4.3 Restrict `C:\ProgramData\tedge` to SYSTEM and Administrators when `ensure_windows_data_dirs` creates it
- [x] 4.4 Apply the same ACL in `install.ps1` / `postinstall.ps1`
- [x] 4.5 `tedge connect` fails with an explicit error on Windows when `mqtt.bridge.built_in` is `false`
- [x] 4.6 Default `mqtt.bridge.built_in` to `true` in the Windows first-run `tedge.toml`
- [x] 4.7 `tedge-shell-plugin` and `tedge-p11-server` exit with "not supported on this platform" on Windows
- [x] 4.8 The agent does not register `shell_execute` on Windows
- [x] 4.10 Detect a Windows reboot from `Win32_OperatingSystem.LastBootUpTime`, default `system.reboot` to `shutdown /r /t 0`
- [x] 4.9 Write the support matrix and the Windows install guide in `docs/src/` and replace `docs/windows-porting-status.md`

## 5. CI

- [x] 5.1 Run `ci-windows.yml` on pull requests and `main`
- [x] 5.2 Add a `cargo clippy -D warnings` step for the Windows target
- [x] 5.3 Build the release `tedge.exe` once and pass it to the system test job as an artifact
- [x] 5.4 Add a system test job: install Mosquitto, install tedge from the build, run the Windows Robot Framework suite, upload the reports
- [x] 5.5 Exclude the `platform:windows` tag from the Linux Robot Framework runs

## 6. Robot Framework on Windows

- [x] 6.1 Windows local adapter: PowerShell command execution, environment passed to the process, no sudo, `copy_to` with `shutil`
- [x] 6.2 Register the Windows adapter from `ThinEdgeIO` when running on Windows
- [x] 6.3 MQTT recorder: paho-mqtt subscription to `#` started in `Setup`, used by `mqtt_match_messages` on Windows
- [x] 6.4 Windows implementations of the service keywords and of `Get Logs`
- [x] 6.5 `tests/RobotFramework/tests/windows/` suite: install and ACL, services and health, CLI, plugins, workflow timeout, `shell_execute` absent
- [x] 6.6 Windows Cumulocity tests (connect, telemetry, software list, config snapshot, log upload), skipped without credentials
- [x] 6.8 Windows mapper tests without cloud credentials: flows translation and the `tedge-mapper-local` service (start, flow reload, stop)
- [x] 6.9 Skip the Cumulocity lookups of `Get Logs` and the suite cleanup when `C8Y_BASEURL` is not set; fail the Cumulocity suite instead of skipping it where the secrets are expected (`C8Y_REQUIRED`)
- [x] 6.10 `tedge connect`/`tedge disconnect` do not restart Mosquitto on Windows, which does not read the thin-edge.io broker configuration and drops the retained capabilities on restart
- [x] 6.11 `tedge connect` starts the agent and mapper services on Windows, where they are `tedge.exe` subcommands rather than executables on the `PATH`
- [x] 6.12 Service logs of the agent and mappers in `C:\ProgramData\tedge\log\<service>.log`
- [x] 6.13 Encode Windows-invalid characters of the file transfer service paths and of the c8y mapper download files (log types such as `robot_log::robotlog`), with `tedge_utils::file_name`
- [ ] 6.14 Default `c8y.root_cert_path` to a Windows location: `/etc/ssl/certs` is logged as unreadable on each configuration load
- [ ] 6.7 Upstream the Windows local adapter to `device-test-core` and remove the local copy

## 7. Remaining Windows test gaps

- [ ] 7.1 Replace hard-coded `/etc/tedge` fixtures in tests ignored on Windows with `TempTedgeDir` paths
- [ ] 7.2 Match the Windows connection-abort error (`WSAECONNABORTED`) in the TLS rejection tests
- [ ] 7.3 Add a timeout to `assert_rx_stream` and align the `notify` tests with `ReadDirectoryChangesW` events
- [x] 7.5 Run the Windows workflow on GitHub Actions and fix the failures it reports
- [ ] 7.4 Audit every remaining `cfg_attr(windows, ignore)` and record the reason category in the support matrix
