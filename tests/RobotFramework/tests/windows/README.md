# Windows system tests

The tests in this directory check thin-edge.io on Windows.
They are tagged `platform:windows`,
and the Linux test runs (`invoke test`, `just integration-test`) exclude them.

The tests run on the Windows host itself with the `local` device adapter:
the commands run with PowerShell
and the MQTT messages are recorded by an MQTT client subscribed to `#`
(the Mosquitto logging plugin used on Linux has no Windows build).
They change the thin-edge.io configuration and services of the host,
so run them on a disposable machine or VM,
such as a GitHub Actions `windows-latest` runner.

## Prerequisites

Run all the commands below in an elevated PowerShell (Run as Administrator),
as the services and `C:\ProgramData\tedge` are only accessible to administrators.

1. Install Python 3 and PowerShell 7 (`pwsh`).
   The tests run the commands with `pwsh.exe` when it is installed,
   as Windows PowerShell 5.1 removes the double quotes from the arguments given to programs,
   for example the JSON payloads given to `tedge mqtt pub`.

2. Install Mosquitto for Windows, listening on `127.0.0.1:1883`,
   and start its service (`mosquitto`).

3. Install `tedge.exe` and register the `tedge-agent` and `tedge-mapper-c8y` services
   with the installation script, from the repository root:

   ```powershell
   configuration\package_scripts\windows\install.ps1 -SourceExe target\release\tedge.exe
   ```

   The script adds `C:\Program Files\tedge\bin` to the system `PATH`
   (open a new shell afterwards),
   creates `C:\ProgramData\tedge` restricted to `SYSTEM` and `Administrators`,
   enables the built-in bridge,
   and starts the services.
   The installation test checks the access restriction,
   so `C:\ProgramData\tedge` must not exist before the installation.

4. Install the Python dependencies, from `tests\RobotFramework`:

   ```powershell
   python -m venv .venv
   .venv\Scripts\Activate.ps1
   pip install -r requirements\requirements.txt -r requirements\requirements.adapter-local.txt
   ```

5. Optionally, to run the Cumulocity tests,
   create the `.env` file in `tests\RobotFramework` (see `devdata\env.template`)
   or set the `C8Y_BASEURL`, `C8Y_USER`, `C8Y_PASSWORD` and `C8Y_TENANT` environment variables.
   The Cumulocity tests are skipped when `C8Y_BASEURL` is not set.

## Running the tests

From `tests\RobotFramework`, with the virtual environment activated:

```powershell
$env:DEVICE_ADAPTER = "local"
python -m robot --pythonpath libraries --include platform:windows --outputdir output tests\windows
```

The test reports are written to the `output` directory.
On failure, the service logs (`C:\ProgramData\tedge\log\*.log`),
the agent operation logs and the recorded MQTT messages are included in the report.
