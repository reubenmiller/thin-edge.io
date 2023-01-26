"""Project tasks"""

from pathlib import Path
import sys
import os
from invoke import task

from dotenv import load_dotenv

output_path = Path(__file__).parent / "output"
project_dir = Path(__file__).parent.parent.parent
project_dotenv = project_dir.joinpath(".env")

load_dotenv(project_dotenv, ".env")

# pylint: disable=invalid-name


def is_ci():
    """Check if being run under ci"""
    return bool(os.getenv("CI"))


@task
def lint(c):
    """Run linter"""
    c.run(f"{sys.executable} -m pylint libraries")


@task(name="format")
def formatcode(c):
    """Format python code"""
    c.run(f"{sys.executable} -m black libraries")


@task(name="reports")
def start_server(c, port=9000):
    """Start simple webserver used to display the test reports"""
    print("Starting local webserver: \n\n", file=sys.stderr)
    path = str(output_path)
    print(
        f"   Go to the reports in your browser: http://localhost:{port}/log.html\n\n",
        file=sys.stderr,
    )
    c.run(f"{sys.executable} -m http.server {port} --directory '{path}'")


@task(name="build")
def build(c, name="debian-systemd"):
    """Build the docker integration test image"""
    context = "../images/debian-systemd"
    c.run(f"docker build -t {name} -f {context}/debian-systemd.dockerfile {context}")


@task(
    help={
        "file": ("Robot file or directory to run"),
        "outputdir": ("Output directory where the reports will be saved to"),
        "processes": ("Number of processes to use when running tests"),
        "include": ("Only run tests which match the given tag"),
    }
)
def test(c, file="tests", outputdir=None, processes=None, include=""):
    """Run tests

    Examples

        # run all tests
        invoke test

        # Run only tests defined in tests/myfile.robot
        invoke test --file=tests/myfile.robot
    """
    if not processes:
        processes = 10

    if not outputdir:
        outputdir = output_path

    command = [
        sys.executable,
        "-m",
        "pabot.pabot",
        "--processes",
        str(processes),
        "--outputdir",
        str(outputdir),
    ]

    if include:
        command.extend(
            [
                "--include",
                str(include),
            ]
        )

    env_file = ".env"
    if env_file:
        load_dotenv(env_file)

    if not is_ci():
        command.extend(
            [
                "--consolecolors",
                "on",
                "--consolemarkers",
                "on",
            ]
        )

    if file:
        command.append(file)

    print(" ".join(command))
    c.run(" ".join(command))
