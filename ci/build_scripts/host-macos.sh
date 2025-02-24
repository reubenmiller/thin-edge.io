#!/usr/bin/env bash
set -e
target="$1"

usage() {
    cat <<EOT
aarch64-unknown-linux-gnu
aarch64-unknown-linux-musl

arm-unknown-linux-gnueabi

arm-unknown-linux-gnueabihf
arm-unknown-linux-musleabihf

armv7-unknown-linux-gnueabihf
armv7-unknown-linux-musleabihf

i686-unknown-linux-gnu
i686-unknown-linux-musl

mipsel-unknown-linux-gnu

x86_64-unknown-linux-gnu
x86_64-unknown-linux-musl
EOT
}

# brew tap messense/macos-cross-toolchains
# https://github.com/messense/homebrew-macos-cross-toolchains

AWS_LC_SYS_INCLUDES=
BINDGEN_EXTRA_CLANG_ARGS=

check_dependency() {
    if ! command -V "$1" >/dev/null 2>&1; then
        echo "Missing dependency. $1" >&2
        exit 1
    fi
}

check_dependency brew
check_dependency cargo
check_dependency cmake
check_dependency clang

if ! command -V bindgen >/dev/null 2>&1; then
    cargo install --force --locked bindgen-cli
fi

brew install "messense/macos-cross-toolchains/$target"

AWS_LC_SYS_INCLUDES=$(find "$(brew --prefix)/Cellar/$target" -type d -name include | tr '\n' ':')

SYS_ROOT=$(find "$(brew --prefix)/Cellar/$target" -type d -name sysroot | tr '\n' ':')
if [ -n "$SYS_ROOT" ]; then
    BINDGEN_EXTRA_CLANG_ARGS="--sysroot=${SYS_ROOT}"
fi

export BINDGEN_EXTRA_CLANG_ARGS
export AWS_LC_SYS_INCLUDES
just release "$target"
