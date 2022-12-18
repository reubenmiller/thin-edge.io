"""ThinEdgeIO Library for Robot Framework

It enables the creation of devices which can be used in tests.
It currently support the creation of Docker devices only
"""
# pylint: disable=invalid-name

import logging
import json
from typing import Any
import time

from c8y_test_core.utils import RandomNameGenerator
from robot.api.deco import keyword, library
from DeviceLibrary import DeviceLibrary
from Cumulocity import Cumulocity

log = logging.getLogger()

devices_lib = DeviceLibrary()
c8y_lib = Cumulocity()


logging.basicConfig(
    level=logging.DEBUG, format="%(asctime)s %(module)s -%(levelname)s- %(message)s"
)
logger = logging.getLogger(__name__)

__version__ = "0.0.1"
__author__ = "Reuben Miller"


def generate_name(prefix: str = "STC") -> str:
    """Generate a random name"""
    generator = RandomNameGenerator()
    return "-".join([prefix, generator.random_name()])


@library(scope="GLOBAL", auto_keywords=False)
class ThinEdgeIO(DeviceLibrary):
    """ThinEdgeIO Library"""

    def end_suite(self, _data: Any, result: Any):
        """End suite hook which is called by Robot Framework
        when the test suite has finished

        Args:
            _data (Any): Test data
            result (Any): Test details
        """
        logger.info("Suite %s (%s) ending", result.name, result.message)
        super().end_suite(_data, result)

    def end_test(self, _data: Any, result: Any):
        """End test hook which is called by Robot Framework
        when the test has ended

        Args:
            _data (Any): Test data
            result (Any): Test details
        """
        logger.info("Listener: detected end of test")
        if not result.passed:
            logger.info("Test '%s' failed: %s", result.name, result.message)

        self.remove_certificate_and_device()
        super().end_test(_data, result)

    @keyword("Get Device Logs")
    def get_logs(self, name: str = None):
        """Get device logs (override base class method to add additional debug info)

        Args:
            name (str, optional): Device name to get logs for. Defaults to None.
        """
        device_sn = name or self.current.get_id()
        try:
            managed_object = c8y_lib.device_mgmt.identity.assert_exists(device_sn)
            logger.info(
                "Managed Object\n%s", json.dumps(managed_object.to_json(), indent=2)
            )
            self.log_operations(managed_object.id)

            # Get agent log files (if they exist)
            logger.info("tedge agent logs: /var/log/tedge/agent/*")
            self.current.execute_command(
                "tail -n +1 /var/log/tedge/agent/* 2>/dev/null || true",
                shell=True,
            )
        except Exception as ex: # pylint: disable=broad-except
            logger.warning("Failed to get device managed object. %s", ex)
        super().get_logs(name)

    def log_operations(self, mo_id: str, status: str = None):
        """Log operations to help with debugging

        Args:
            mo_id (str): Managed object id
            status (str, optional): Operation status. Defaults to None (which means all statuses).
        """
        operations = c8y_lib.c8y.operations.get_all(
            device_id=mo_id, status=status, after=self.test_start_time
        )

        if operations:
            logger.info("%s operations", status or "ALL")
            for i, operation in enumerate(operations):
                # Only treat operations which did not finish
                # as errors (as FAILED might be intended in a few test cases)
                log_method = (
                    logger.info
                    if operation.status in (operation.Status.SUCCESSFUL, operation.Status.FAILED)
                    else logger.warning
                )
                log_method(
                    "Operation %d: (status=%s)\n%s",
                    i + 1,
                    operation.status,
                    json.dumps(operation.to_json(), indent=2),
                )
        else:
            logger.info("No operations found")

    def remove_certificate_and_device(self):
        """Remove trusted certificate"""
        fingerprint = (
            self.execute_command(
                "tedge cert show | grep '^Thumbprint:' | cut -d' ' -f2 | tr A-Z a-z",
            )
            .decode("utf8")
            .strip()
        )
        if fingerprint:
            c8y_lib.trusted_certificate_delete(fingerprint)

            device_sn = self.current.get_id()
            if device_sn:
                c8y_lib.delete_managed_object(device_sn)
            else:
                log.info("Device serial number is empty, so nothing to delete from Cumulocity")

    @keyword("Download From GitHub")
    def download_from_github(self, *run_id: str, arch: str = "aarch64"):
        """Dowload artifacts from a GitHub Run

        Args:
            *run_id (str): Run ids of the artifacts to download
            arch (str, optional): CPU Architecture to download for. Defaults to aarch64
        """

        # pylint: disable=line-too-long
        self.execute_command(
            """
            type -p curl >/dev/null || sudo apt install curl -y
            curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg | sudo dd of=/usr/share/keyrings/githubcli-archive-keyring.gpg \\
            && sudo chmod go+r /usr/share/keyrings/githubcli-archive-keyring.gpg \\
            && echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" | sudo tee /etc/apt/sources.list.d/github-cli.list > /dev/null \\
            && sudo apt-get update \\
            && sudo apt-get -y install gh
        """.lstrip()
        )

        run_ids = []
        # Also support providing values via csv (e.g. when set from variables)
        for i_run_id in run_id:
            run_ids.extend(i_run_id.split(","))

        for i_run_id in run_ids:
            self.execute_command(
                f"""
                gh run download {i_run_id} -n debian-packages-{arch}-unknown-linux-gnu -R thin-edge/thin-edge.io
            """.strip()
            )

    #
    # Tedge commands
    #
    @keyword("Set Tedge Configuration Using CLI")
    def tedge_update_settings(self, name: str, value: str) -> str:
        """Update tedge settings via CLI (`tedge config set`)

        Args:
            name (str): Setting name to update
            value (str): Value to be updated with

        Returns:
            str: Command output
        """
        return self.execute_command(f"tedge config set {name} {value}")

    @keyword("Tedge Connect")
    def tedge_connect(self, mapper: str = "c8y") -> str:
        """Tedge connect a cloud

        Args:
            mapper (str, optional): Mapper name, e.g. c8y, az, etc. Defaults to "c8y".

        Returns:
            str: Command output
        """
        return self.execute_command(f"tedge connect {mapper}")

    @keyword("Tedge Disonnect")
    def tedge_disconnect(self, mapper: str = "c8y") -> str:
        """Tedge connect a cloud

        Args:
            mapper (str, optional): Mapper name, e.g. c8y, az, etc. Defaults to "c8y".

        Returns:
            str: Command output
        """
        return self.execute_command(f"tedge disconnect {mapper}")

    @keyword("Tedge Disconnect Then Connect")
    def tedge_disconnect_connect(self, mapper: str = "c8y", sleep: float = 0.0):
        """Tedge disconnect the connect a cloud

        Args:
            mapper (str, optional): Mapper name, e.g. c8y, az, etc. Defaults to "c8y".
            sleep (float, optional): Time to wait in seconds before connecting. Defaults to 0.0.
        """
        self.tedge_disconnect(mapper)
        if sleep > 0:
            time.sleep(sleep)
        self.tedge_connect(mapper)
