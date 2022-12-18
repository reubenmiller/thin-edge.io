"""Project tasks"""

from pathlib import Path
import sys
from invoke import task

from dotenv import load_dotenv

project_dotenv = Path(__file__).parent.parent.parent.joinpath(".env")

load_dotenv(project_dotenv, ".env")
load_dotenv(".env")

# pylint: disable=invalid-name


@task
def lint(c):
    """Run linter"""
    c.run(f"{sys.executable} -m pylint libraries")


@task(name="format")
def formatcode(c):
    """Format python code"""
    c.run(f"{sys.executable} -m black libraries")


@task(name="build")
def build(c, name="debian-systemd"):
    """Build the docker integration test image"""
    context = "../images/debian-systemd"
    c.run(
        f"docker build -t {name} -f {context}/debian-systemd.dockerfile {context}"
    )


@task(
    help={
        "variables": ("Variables file used to control the test"),
        "modules": (
            "Only run tests which match this expression. "
            "This argument is passed to the -m option of pytest"
        ),
        "pattern": "Only include test where their names match the given pattern",
    }
)
def test(c, variables="", modules="", pattern="", runs=1):
    """Run tests

    Examples

        # run all tests
        invoke test

        # run all tests using a given variables file
        invoke test --variables variables.tst.json

        # exclude control related tests
        invoke test --variables variables.tst.json -m "not control"

        # exclude both control and events tests
        invoke test --variables variables.tst.json -m "not measurements and not events"

        # run only measurements tests
        invoke test --variables variables.tst.json -m "measurements"

        # run only tests matching a filter
        invoke test --pattern "test_inventory_models"
    """
    # pylint: disable=too-many-arguments
    command = [
        sys.executable,
        "-m",
        "pytest",
    ]

    env_file = ".env"
    if variables:
        command.append(f"--variables={variables}")

    if modules:
        command.append(f"-m='{modules}'")

    if pattern:
        command.append(f"-k='{pattern}'")

    if runs and runs > 1:
        command.append("--flake-finder")
        command.append(f"--flake-runs={int(runs)}")

    if env_file:
        load_dotenv(env_file)

    command.append("--color=yes")
    command.append("integration")
    c.run(" ".join(command))
