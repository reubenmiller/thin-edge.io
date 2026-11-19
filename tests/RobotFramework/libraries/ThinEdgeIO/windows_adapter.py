"""Windows variant of the device-test-core local device adapter

The ``local`` adapter of device-test-core is POSIX only: it runs commands via
``sudo -E env K=V /bin/sh -c <cmd>`` and copies files with ``mkdir`` and ``tar``.
This module provides a subclass which runs commands with Windows PowerShell and
copies files with the host file system API, and a function to register it in
place of the original adapter.

This is an interim solution until the Windows support is upstreamed to
device-test-core (https://github.com/thin-edge/device-test-core).
"""

import base64
import functools
import glob
import logging
import os
import shutil
import subprocess
from pathlib import Path
from typing import Any, List

from device_test_core.command import CmdOutput
from device_test_core.local.device import LocalDeviceAdapter
from device_test_core.utils import to_str

log = logging.getLogger(__name__)

TEDGE_CONFIG_DIR = r"C:\ProgramData\tedge"
TEDGE_LOG_DIR = os.path.join(TEDGE_CONFIG_DIR, "log")

# Prepended to each command:
# * output UTF-8 so the output can be decoded by the test library
# * disable progress bars, which are otherwise written to stderr as CLIXML
# * pass arguments containing double quotes (e.g. JSON payloads) unchanged to
#   native programs (PowerShell >= 7.3 only; Windows PowerShell 5.1 strips them)
_PREAMBLE = (
    "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8\n"
    "$OutputEncoding = [System.Text.Encoding]::UTF8\n"
    "$ProgressPreference = 'SilentlyContinue'\n"
    "$PSNativeCommandArgumentPassing = 'Standard'\n"
    "$global:LASTEXITCODE = 0\n"
)

# Appended to each command. `powershell.exe -Command` only exits with 0 or 1,
# so the exit code of the last native command is passed on explicitly
# (e.g. `tedge.exe ...` exiting with 2 results in an exit code of 2).
# Note: like `sh -c`, the result reflects the last command, except that a
# failed native command followed by a successful cmdlet still reports the
# native exit code.
_EPILOGUE = (
    "\n$__ok = $?\n"
    "if ($global:LASTEXITCODE -ne 0) { exit $global:LASTEXITCODE }\n"
    "if (-not $__ok) { exit 1 }\n"
    "exit 0\n"
)


def default_shell() -> str:
    """PowerShell 7 (pwsh.exe) if installed, otherwise Windows PowerShell 5.1

    PowerShell 7 is preferred as Windows PowerShell 5.1 strips the double quotes
    from arguments passed to native programs, e.g. JSON payloads given to
    `tedge mqtt pub`.
    """
    return "pwsh.exe" if shutil.which("pwsh.exe") else "powershell.exe"


def _kill_process_tree(pid: int):
    """Kill a process and all of its children"""
    subprocess.run(
        ["taskkill", "/T", "/F", "/PID", str(pid)],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=False,
    )


class WindowsLocalDeviceAdapter(LocalDeviceAdapter):
    """Local device adapter for a Windows host

    * Commands run with ``pwsh.exe`` (or ``powershell.exe`` if PowerShell 7 is not
      installed) ``-NoProfile -NonInteractive``. The shell can be set with the
      ``shell_bin`` adapter setting
    * The adapter environment variables are passed to the process (never via sudo)
    * Files are copied with :mod:`shutil`
    """

    def __init__(self, *args, **kwargs):
        # sudo does not exist on Windows. The tests run from an elevated shell instead
        kwargs["use_sudo"] = False
        super().__init__(*args, **kwargs)

    def use_sudo(self) -> bool:
        return False

    def _process_env(self) -> dict:
        return {
            **os.environ,
            **{
                key: str(value) for key, value in self._env.items() if value is not None
            },
        }

    def execute_command(
        self, cmd: str, log_output: bool = True, shell: bool = True, **kwargs
    ) -> CmdOutput:
        """Execute a command on the Windows host

        Args:
            cmd (str): PowerShell command. If shell is False, then the command is
                executed directly (a list is passed as the program arguments).
            log_output (bool, optional): Log the stdout after the command has executed
            shell (bool, optional): Execute the command in PowerShell
            **kwargs (Any, optional): Additional keyword arguments. ``timeout`` in seconds
                (defaults to 120). ``sudo`` is ignored.

        Returns:
            CmdOutput: Command output details, e.g. stdout, stderr and return_code
        """
        kwargs.pop("sudo", None)
        timeout = kwargs.pop("timeout", 120)

        if shell:
            script = cmd if isinstance(cmd, str) else subprocess.list2cmdline(cmd)
            # -EncodedCommand has the same semantics as -Command but avoids the
            # quoting rules of the Windows command line
            encoded = base64.b64encode(
                (_PREAMBLE + script + _EPILOGUE).encode("utf-16-le")
            ).decode("ascii")
            run_cmd: Any = [
                self._config.get("shell_bin") or default_shell(),
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-EncodedCommand",
                encoded,
            ]
        else:
            run_cmd = cmd

        with subprocess.Popen(
            run_cmd,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=self._process_env(),
        ) as proc:
            try:
                stdout, stderr = proc.communicate(timeout=timeout)
            except subprocess.TimeoutExpired:
                _kill_process_tree(proc.pid)
                proc.communicate()
                raise

        if log_output:
            log.info(
                "cmd: %s, exit code: %d\nstdout:\n%s\nstderr:\n%s",
                cmd,
                proc.returncode,
                to_str(stdout) or "<<empty>>",
                to_str(stderr) or "<<empty>>",
            )
        return CmdOutput(stdout=stdout, stderr=stderr, return_code=proc.returncode)

    def copy_to(self, src: str, dst: str):
        """Copy files to the device (the local host)

        Args:
            src (str): Source file, folder or glob pattern
            dst (str): Destination. If it ends with a path separator, is an
                existing directory, or multiple sources match, then the
                sources are copied into it
        """
        has_pattern = any(char in src for char in "*?[")
        sources = glob.glob(src) if has_pattern else [src]
        sources = [path for path in sources if os.path.exists(path)]
        if not sources:
            raise FileNotFoundError(f"No files matched the source pattern: {src}")

        into_dir = (
            len(sources) > 1
            or dst.endswith(("/", "\\"))
            or dst in (".", "..")
            or os.path.isdir(dst)
        )
        for source in sources:
            target = os.path.join(dst, Path(source).name) if into_dir else dst
            if os.path.exists(target) and os.path.samefile(source, target):
                continue
            parent = os.path.dirname(os.path.abspath(target))
            os.makedirs(parent, exist_ok=True)
            if os.path.isdir(source):
                shutil.copytree(source, target, dirs_exist_ok=True)
            else:
                shutil.copy2(source, target)

    def restart(self):
        """Restart device (not supported)"""
        raise NotImplementedError(
            "Restarting a Windows host is not supported by the local adapter, "
            "as the test runner runs on the same host"
        )

    def get_logs(self, since: Any = None) -> List[str]:
        """Get the thin-edge.io service logs

        The services write their logs to C:\\ProgramData\\tedge\\log\\<service>.log.
        The ``since`` filter is not applied as the log files are not parsed.

        Returns:
            List[str]: Log lines prefixed with the file name
        """
        lines = []
        for path in sorted(glob.glob(os.path.join(TEDGE_LOG_DIR, "*.log"))):
            try:
                with open(path, encoding="utf8", errors="replace") as file:
                    name = Path(path).stem
                    lines.extend(f"{name}: {line.rstrip()}" for line in file)
            except OSError as ex:
                log.warning("Could not read log file. path=%s, error=%s", path, ex)
        return lines


def register_windows_local_adapter():
    """Use WindowsLocalDeviceAdapter for the ``local`` adapter on a Windows host

    It is a no-op on other operating systems.
    """
    if os.name != "nt":
        return

    # pylint: disable=import-outside-toplevel
    from device_test_core.local import factory

    if factory.LocalDeviceAdapter is WindowsLocalDeviceAdapter:
        return

    factory.LocalDeviceAdapter = WindowsLocalDeviceAdapter
    create_device = factory.LocalDeviceFactory.create_device

    @functools.wraps(create_device)
    def create_device_without_sudo(self, *args, **kwargs):
        kwargs.setdefault("use_sudo", False)
        return create_device(self, *args, **kwargs)

    factory.LocalDeviceFactory.create_device = create_device_without_sudo
