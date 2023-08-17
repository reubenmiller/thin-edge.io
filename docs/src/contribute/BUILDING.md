---
title: Build thin-edge
tags: [Contribute, Build]
sidebar_position: 1
---

# Building thin-edge.io

## Requirements

You can use any OS to build from source (below has been tested on Ubuntu, but we also use Debian, macOS, and FreeBSD successfully).

Our recommended development environments include:

* Ubuntu 20.04
* Debian 11 (Bullseye)
* Debian 12 (Bookworm)
* VSCode Dev Container (included in the repository)

Following tools and packages are required for building and testing the platform.

* Rust
* git
* curl
* gcc
* build-essential
* python3
* python3-pip
* [just](https://just.systems/man/en/chapter_5.html)

The following tooling will also be automatically installed when it is required.

* [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild)
* [ziglang 0.10.1](https://ziglang.org/)
* [nfpm](https://nfpm.goreleaser.com/install/)

:::note
If you are having problems with any of the tooling, then you can manually install the tools using the instructions on the associated project pages.
:::

A list of our test platforms can be found [here](../references/supported-platforms.md).

### Setting up your development environment

1. Install the base tooling

    You can install the required tools using the following commands:

    ```sh tab={"label":"Debian/Ubuntu"}
    sudo apt-get update
    sudo apt-get install -y \
        curl \
        git \
        build-essential \
        python3 \
        python3-pip
    sudo curl --proto '=https' --tlsv1.2 -sSf https://just.systems/install.sh | bash -s -- --to /usr/bin/
    ```

2. Install Rust

    ```sh
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    ```

    Following the instructions shown by the script, but generally speaking accepting the default settings is a good way to go.

    You should also be prompted to run a single command after the installation to enable Rust/cargo in the current shell. The command should look something like the following:

    ```sh
    source "$HOME/.cargo/env"
    ```

## Get the code

thin-edge.io code is in git repository on github to acquire the code use following command:

```sh tab={"label":"HTTPS"}
git clone https://github.com/thin-edge/thin-edge.io.git
cd thin-edge.io
```

```sh tab={"label":"SSH"}
git clone git@github.com:thin-edge/thin-edge.io.git
cd thin-edge.io
```

```sh tab={"label":"GitHub CLI"}
gh repo clone thin-edge/thin-edge.io
cd thin-edge.io
```

## Compiling

As we are using  `cargo workspace` for all our crates. All compiled files are put in `./target/` directory with target's name eg: `./target/<target>/debug` or `./target/<target>/release`.

By default the target will be automatically selected based your host machine's architecture. You can see the default target, by using `just info`.

```sh
just info
```

```text title="Output"
OS:             linux
OS_FAMILY:      unix
HOST_ARCH:      aarch64
VERSION:        0.12.1~101+g4ea00b61
DEFAULT_TARGET: aarch64-unknown-linux-musl
```

:::note
thin-edge.io uses [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild) by default to provide an easier way to build build for multiple Linux architectures with minimal dependencies.
:::

### Compiling dev (for debugging only)

To compile the dev profile (with debug symbols) we use following command:

```sh
just build
```

The task will print out the directory where the compiled binaries can be found:

```text title="Example"
Binaries are stored under:

  target/aarch64-unknown-linux-musl/debug
```

Binaries can be run using:

```sh
./target/aarch64-unknown-linux-musl/debug/tedge
```

Alternatively, you can build and run an executable in a single command using:

```sh
just run tedge

# or you can pass arguments to the underlying command using:
just run tedge -- --help
```

:::note
If the build fails with an error message which includes "killed", then it is likely that your device is running out of memory whilst building the project. Since the build files are cached, try running the same build command again as this will consume less memory than the previous time.
:::

### Compiling release

To compile the release profile and linux packages, the following command is used:

```sh
just release
```

Build artifacts can be found in `./target/<target>/release` and will include executables:

```sh
ls -l ./target/aarch64-unknown-linux-musl/release/tedge*
```

```text title="Output"
-rwxrwxr-x   2 user user 11111 Jan 1 00:00 tedge
-rwxrwxr-x   2 user user 11111 Jan 1 00:00 tedge-mapper
```

Binaries can be run using:

```sh
./target/aarch64-unknown-linux-musl/release/tedge
```

The linux packages are built under `target/<target>/packages`

```sh
ls -l target/aarch64-unknown-linux-musl/packages
```

```text title="Output"
-rw-r--r--@ 1 developer  staff   2826330 Aug 14 15:51 c8y-configuration-plugin-0.12.1~86+g15543d0e.aarch64.rpm
-rw-r--r--@ 1 developer  staff   2803796 Aug 14 15:51 c8y-configuration-plugin_0.12.1~86+g15543d0e_aarch64.apk
-rw-r--r--@ 1 developer  staff   2248802 Aug 14 15:51 c8y-configuration-plugin_0.12.1~86+g15543d0e_arm64.deb
-rw-r--r--@ 1 developer  staff   2488366 Aug 14 15:51 c8y-firmware-plugin-0.12.1~86+g15543d0e.aarch64.rpm
-rw-r--r--@ 1 developer  staff   2489527 Aug 14 15:51 c8y-firmware-plugin_0.12.1~86+g15543d0e_aarch64.apk
-rw-r--r--@ 1 developer  staff   2017564 Aug 14 15:51 c8y-firmware-plugin_0.12.1~86+g15543d0e_arm64.deb
-rw-r--r--@ 1 developer  staff   2792811 Aug 14 15:51 c8y-log-plugin-0.12.1~86+g15543d0e.aarch64.rpm
-rw-r--r--@ 1 developer  staff   2772364 Aug 14 15:51 c8y-log-plugin_0.12.1~86+g15543d0e_aarch64.apk
-rw-r--r--@ 1 developer  staff   2223488 Aug 14 15:51 c8y-log-plugin_0.12.1~86+g15543d0e_arm64.deb
-rw-r--r--@ 1 developer  staff   1737190 Aug 14 15:51 c8y-remote-access-plugin-0.12.1~86+g15543d0e.aarch64.rpm
-rw-r--r--@ 1 developer  staff   1740730 Aug 14 15:51 c8y-remote-access-plugin_0.12.1~86+g15543d0e_aarch64.apk
-rw-r--r--@ 1 developer  staff   1417056 Aug 14 15:51 c8y-remote-access-plugin_0.12.1~86+g15543d0e_arm64.deb
-rw-r--r--@ 1 developer  staff   1288204 Aug 14 15:51 sawtooth-publisher_0.12.1~86+g15543d0e_arm64.deb
-rw-r--r--@ 1 developer  staff   2462751 Aug 14 15:51 tedge-0.12.1~86+g15543d0e.aarch64.rpm
-rw-r--r--@ 1 developer  staff   2870722 Aug 14 15:51 tedge-agent-0.12.1~86+g15543d0e.aarch64.rpm
-rw-r--r--@ 1 developer  staff   2852667 Aug 14 15:51 tedge-agent_0.12.1~86+g15543d0e_aarch64.apk
-rw-r--r--@ 1 developer  staff   2294428 Aug 14 15:51 tedge-agent_0.12.1~86+g15543d0e_arm64.deb
-rw-r--r--@ 1 developer  staff   1162328 Aug 14 15:51 tedge-apt-plugin-0.12.1~86+g15543d0e.aarch64.rpm
-rw-r--r--@ 1 developer  staff   1161387 Aug 14 15:51 tedge-apt-plugin_0.12.1~86+g15543d0e_aarch64.apk
-rw-r--r--@ 1 developer  staff    911546 Aug 14 15:51 tedge-apt-plugin_0.12.1~86+g15543d0e_arm64.deb
-rw-r--r--@ 1 developer  staff   1372220 Aug 14 15:51 tedge-dummy-plugin_0.12.1~86+g15543d0e_arm64.deb
-rw-r--r--@ 1 developer  staff   3002148 Aug 14 15:51 tedge-mapper-0.12.1~86+g15543d0e.aarch64.rpm
-rw-r--r--@ 1 developer  staff   3011313 Aug 14 15:51 tedge-mapper_0.12.1~86+g15543d0e_aarch64.apk
-rw-r--r--@ 1 developer  staff   2394466 Aug 14 15:51 tedge-mapper_0.12.1~86+g15543d0e_arm64.deb
-rw-r--r--@ 1 developer  staff   1676180 Aug 14 15:51 tedge-watchdog-0.12.1~86+g15543d0e.aarch64.rpm
-rw-r--r--@ 1 developer  staff   1669413 Aug 14 15:51 tedge-watchdog_0.12.1~86+g15543d0e_aarch64.apk
-rw-r--r--@ 1 developer  staff   1373226 Aug 14 15:51 tedge-watchdog_0.12.1~86+g15543d0e_arm64.deb
-rw-r--r--@ 1 developer  staff  19141066 Aug 14 15:51 tedge_0.12.1~86+g15543d0e_aarch64-unknown-linux-musl.tar.gz
-rw-r--r--@ 1 developer  staff   2463989 Aug 14 15:51 tedge_0.12.1~86+g15543d0e_aarch64.apk
-rw-r--r--@ 1 developer  staff   1975922 Aug 14 15:51 tedge_0.12.1~86+g15543d0e_arm64.deb
```

## Cross compiling

thin-edge.io uses [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild) to provide easy way to cross compile for other linux targets.

```sh tab={"label":"x86_64"}
just release x86_64-unknown-linux-musl
```

```sh tab={"label":"aarch64"}
just release aarch64-unknown-linux-musl
```

```sh tab={"label":"armv7"}
just release armv7-unknown-linux-musleabihf
```

```sh tab={"label":"armv6"}
just release arm-unknown-linux-musleabihf
```

Build artifacts can be found in `./target/<target>/release` and will include executables:

```sh
ls -l ./target/armv7-unknown-linux-musleabihf/release/tedge*
```

```text title="Output"
-rwxrwxr-x   2 user user 11111 Jan 1 00:00 tedge
-rwxrwxr-x   2 user user 11111 Jan 1 00:00 tedge-mapper
```

## Running tests

When contributing to thin-edge.io, we ask you to write tests for the code you have written. The tests will be run by build pipeline when you create pull request, but you can easily run all the tests when you are developing with following command:

```sh
just test
```

This will run all tests from the repository and sometime may take long time, `cargo` allows you to run specific test or set of tests for binary:

```sh
just test --bin tedge
```

## Linux Packaging

We use [nfpm](https://github.com/goreleaser/nfpm) to build our linux packages (deb, rpm and apk).

Follow the [nfpm install instructions](https://nfpm.goreleaser.com/install/) to install the dependency. The linux packages will automatically be built when running `just release`.

The virtual packages (e.g. tedge-full and tedge-minimal) can be built using the following command:

To build all of the targets and linux packages the following commands can be used.

```sh
just release x86_64-unknown-linux-musl
just release aarch64-unknown-linux-musl
just release armv7-unknown-linux-musleabihf
just release arm-unknown-linux-musleabihf
just release-linux-virtual
```
