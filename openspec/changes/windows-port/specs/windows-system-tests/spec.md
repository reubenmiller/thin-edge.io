## ADDED Requirements

### Requirement: Windows CI workflow

A GitHub Actions workflow SHALL run on every pull request and every push to `main` on a `windows-latest` runner, and SHALL:
- run `cargo clippy --workspace --all-targets --all-features` for the Windows target with warnings denied,
- run the workspace unit and integration tests with `cargo nextest`,
- build the release `tedge.exe`,
- run the Windows Robot Framework suite against that build.

#### Scenario: Pull request
- **WHEN** a pull request is opened against `main`
- **THEN** the Windows lint, unit test and system test jobs run and report their status on the pull request

#### Scenario: Windows-only compile error
- **WHEN** a change uses a Unix-only API outside `cfg(unix)`
- **THEN** the Windows lint job fails

### Requirement: Ignored tests state their reason

A unit test that does not run on Windows SHALL be marked `#[cfg_attr(windows, ignore = "<reason>")]` or `#[cfg(unix)]` with the reason in a comment,
and SHALL NOT be ignored for a reason that is a Windows defect thin-edge.io can fix.

#### Scenario: Test relying on a bash plugin
- **WHEN** a test needs a plugin script
- **THEN** it creates a `.cmd` script on Windows and a shell script on Unix, and runs on both

### Requirement: Robot Framework device adapter for Windows

The `ThinEdgeIO` library SHALL support the `local` adapter on a Windows host:
- `Execute Command` runs commands with PowerShell and passes the adapter environment variables to the process,
- `sudo` is not used,
- `Transfer To Device` copies files with the host file system API,
- service keywords (`Start Service`, `Stop Service`, `Restart Service`, `Service Should Be Running`, `Service Should Be Stopped`) use the SCM.

#### Scenario: Setup on Windows
- **WHEN** a Windows suite calls `Setup` with `DEVICE_ADAPTER=local` on a Windows host
- **THEN** the library returns the device serial number and subsequent `Execute Command` calls run in PowerShell

### Requirement: MQTT assertions on Windows

On Windows, the `ThinEdgeIO` MQTT keywords (`Should Have MQTT Messages`, `Should Not Have MQTT Messages`, `Should Have Retained MQTT Messages`, `Service Health Status Should Be Up`) SHALL work without the Mosquitto logging plugin, by recording the messages received by an MQTT subscription to `#` opened during `Setup`.
They SHALL accept the same arguments as on Linux.

#### Scenario: Measurement assertion
- **WHEN** a test publishes a measurement with `tedge mqtt pub` and calls `Should Have MQTT Messages` with the measurement topic
- **THEN** the keyword finds the message

### Requirement: Windows Robot Framework suite

The repository SHALL contain a Robot Framework suite tagged `platform:windows` that covers on Windows:
- installation from the release build and first-run directory creation, including the directory ACL,
- start, stop and restart of `tedge-agent` and `tedge-mapper-c8y` as services, including health `down` on stop,
- `tedge config`, `tedge cert create`, `tedge mqtt pub` and `tedge mqtt sub`,
- the built-in bridge connection to Cumulocity, telemetry, and the software list, configuration snapshot, log upload and restart operations, when Cumulocity credentials are available,
- plugins written as `.ps1` and `.cmd` scripts,
- termination of a workflow script on timeout,
- the absence of `shell_execute`.

Linux test runs SHALL exclude the `platform:windows` tag.

#### Scenario: Suite without Cumulocity credentials
- **WHEN** the Windows suite runs without `C8Y_*` secrets (for example on a fork)
- **THEN** the tests that need Cumulocity are skipped and the local tests run
