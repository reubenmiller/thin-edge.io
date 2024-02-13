#!/usr/bin/env bash
set -e

help() {
    cat << EOT
Build the release artifacts using clang

TARGETS

The following targets are known to work when compiling on either MacOS or Linux

  * i686-unknown-linux-musl
  * x86_64-unknown-linux-musl
  * aarch64-unknown-linux-musl
  * armv7-unknown-linux-musleabihf
  * arm-unknown-linux-musleabihf

Linux only
  * arm-unknown-linux-musleabi
  * riscv64gc-unknown-linux-gnu
  * armv5te-unknown-linux-musleabi
EOT
}

if command -v brew >/dev/null 2>&1; then
    PATH="$(brew --prefix)/opt/llvm/bin:$PATH"
    export PATH
fi

TARGET="$1"
rustup toolchain add --profile=minimal stable
rustup target add --toolchain=stable "$TARGET"

# shellcheck disable=SC1091
. ./ci/build_scripts/version.sh

./mk/cargo.sh +stable build -p tedge --target="$TARGET" --release

./ci/build_scripts/build.sh "$TARGET" --skip-build --skip-test-packages
