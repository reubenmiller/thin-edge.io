#!/usr/bin/env python
#
# Generate linux maintainer scripts used for linux packaging, e.g. post/pre install/remove
#
# Example:
#   python3 generate.py ./builder.json
#

import json
import sys
from typing import Dict, List
from dataclasses import dataclass
from pathlib import Path
import logging

log = logging.getLogger()


@dataclass
class Service:
    name: str = ""
    enable: bool = True
    start: bool = True
    stop_on_upgrade: bool = True
    restart_after_upgrade: bool = True


def get_template(name, default=""):
    file = Path(name)
    if file.exists():
        return Path(name).read_text(encoding="utf8")
    return default


def replace_variables(
    contents: str, variables: Dict[str, str], wrap: bool = False
) -> str:
    expanded_contents = contents
    for key, value in variables.items():
        var_name = f"#{key}#".upper()
        expanded_contents = expanded_contents.replace(var_name, value)

    if wrap and expanded_contents:
        return "\n".join(
            [
                "# Automatically added by thin-edge.io",
                expanded_contents,
                "# End automatically added section",
            ]
        )
    return expanded_contents


def write_script(
    input_contents, lines: List[str], output_file: Path, debug: bool = True
) -> str:
    non_empty_lines = [line for line in lines if line]
    if not non_empty_lines:
        return input_contents

    contents = replace_variables(
        input_contents,
        {
            "LINUXHELPER": "\n".join(lines),
        },
        wrap=False,
    )

    if debug:
        print(f"---- start {output_file} ----\n")
        print(contents)
        print(f"---- end {output_file} ----\n")

    output_file.write_text(contents, encoding="utf8")
    return contents


def process_package(name: str, manifest: dict, package_type: str, out_dir: Path):
    services = [Service(**service) for service in manifest.get("services", [])]

    postinst = []
    preinst = []
    prerm = []
    postrm = []

    service_names = [
        (service.name or name) + ".service"
        for service in services
    ]

    for service in services:
        log.warning("Processing service: %s", service.name or name)
        service_name = (service.name or name) + ".service"

        variables = {
            "UNITFILE": service_name,
            "UNITFILES": " ".join(service_names),
        }

        # https://github.com/kornelski/cargo-deb/blob/main/src/dh_installsystemd.rs

        

        

        # postinst
        # if service.enable:
        snippet =  {
            True: "postinst-systemd-enable",
            False: "postinst-systemd-dont-enable",
        }[service.enable]
        postinst.append(
            replace_variables(
                get_template(f"{package_type}/{snippet}"),
                variables,
                wrap=True,
            )
        )

    # postrm
    postrm.append(
        replace_variables(
            get_template(f"{package_type}/postrm-systemd-reload-only"),
            variables,
            wrap=True,
        )
    )

    postrm.append(
        replace_variables(
            get_template(f"{package_type}/postrm-systemd"),
            variables,
            wrap=True,
        )
    )    

    if service.restart_after_upgrade:
        snippet = {
            True: ("postinst-systemd-restart", "restart"),
            False: ("postinst-systemd-restartnostart", "try-restart"),
        }[service.start]

        postinst.append(
            replace_variables(
                get_template(f"{package_type}/{snippet[0]}"),
                {
                    **variables,
                    "RESTART_ACTION": snippet[1],
                },
                wrap=True,
            )
        )
    elif service.start:
        postinst.append(
            replace_variables(
                get_template(f"{package_type}/postinst-systemd-start"),
                variables,
                wrap=True,
            )
        )

    # prerm
    if not service.stop_on_upgrade or service.restart_after_upgrade:
        # stop service only on remove
        prerm.append(
            replace_variables(
                get_template(f"{package_type}/prerm-systemd-restart"),
                variables,
                wrap=True,
            )
        )
    elif service.start:
        # always stop service
        prerm.append(
            replace_variables(
                get_template(f"{package_type}/prerm-systemd"),
                variables,
                wrap=True,
            )
        )

    default_t = "\n".join([
        "#!/bin/sh",
        "set -e",
        "#LINUXHELPER#",
    ])

    

    write_script(get_template(f"../{name}/postinst", default_t), postinst, out_dir / "postinst")
    write_script(get_template(f"../{name}/postrm", default_t), postrm, out_dir / "postrm")
    write_script(get_template(f"../{name}/prerm", default_t), prerm, out_dir / "prerm")


def main(file):
    manifests = json.loads(Path(file).read_text("utf8"))
    packages = manifests.get("packages", {})
    package_types = manifests.get("types", [])

    output_dir = Path("../.build")
    output_dir.mkdir(parents=True, exist_ok=True)

    for name, manifest in packages.items():
        for package_type in package_types:
            package_dir = output_dir / name / package_type
            package_dir.mkdir(parents=True, exist_ok=True)
            process_package(name, manifest, package_type, package_dir)


if __name__ == "__main__":
    main(sys.argv[1])
