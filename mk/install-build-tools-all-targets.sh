#!/usr/bin/env bash
set -e

TARGETS=(
    i686-unknown-linux-musl
    x86_64-unknown-linux-musl
    aarch64-unknown-linux-musl
    armv7-unknown-linux-musleabihf
    arm-unknown-linux-musleabihf
    arm-unknown-linux-musleabi
    riscv64gc-unknown-linux-gnu
    armv5te-unknown-linux-musleabi
)

for TARGET in "${TARGETS[@]}"; do
    mk/install-build-tools.sh --target="$TARGET"
done
