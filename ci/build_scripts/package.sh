#!/usr/bin/env bash
set -e

help() {
  cat <<EOF
Build linux packages

NOTE: This script is intended to be called from the build.sh script

TODO:
* [x] Slight difference in version number generation, check which is better
    * NEW: 0.12.0~13-gea7d1cf3
    * OLD: 0.12.0-13-gea7d1cf3

* [ ] Port debian start stop services in maintainer scripts
* [ ] Rename tarball as the name is confusing, as it mirrors the tedge deb/apk/rpm package.

Usage:
    $0 <CMD> <ARCH> [...PACKAGE]

Args:
    CMD      Packaging command. Accepted values: [build, build_meta]
             build   Build the linux packages
             build_virt  Build the virtual linux packages which make it easier for users to install, e.g. "tedge-full" just references all the tedge packages

    ARCH     RUST target architecture which can be a value listed from the command 'rustc --print target-list'
             If left blank then the TARGET will be set to the linux musl variant appropriate for your machine.
             For example, if building on MacOS M1, 'aarch64-unknown-linux-musl' will be selected, for linux x86_64,
             'x86_64-unknown-linux-musl' will be selected.
    
    PACKAGE  List of packages to build, e.g. tedge, tedge-agent, tedge-mapper etc. More than 1 can be provided

    Example ARCH (target) values:

        MUSL variants
        * x86_64-unknown-linux-musl
        * aarch64-unknown-linux-musl
        * armv7-unknown-linux-musleabihf
        * arm-unknown-linux-musleabihf

Flags:
    --help|-h   Show this help
    --version               Print the automatic version which will be used (this does not build the project)
    --output <path>         Output directory where the packages will be written to
    --types <csv_string>    CSV list of packages types. Accepted values: deb, rpm, apk, tarball
    --clean                 Clean the output directory before writing any packges to it

Env:
    GIT_SEMVER      Use a custom version when building the packages. Only use for dev/testing purposes!

Examples:
    $0 build aarch64-unknown-linux-musl tedge tedge-agent tedge-mapper
    # Package

    $0 aarch64-unknown-linux-musl tedge-agent
    # Package the tedge-agent for aarch64

    $0 aarch64-unknown-linux-musl tedge tedge-agent --version 0.0.1
    # Package using an manual version
EOF
}

#
# Package settings (what can be referenced in the nfpm configuration files)
#
export CI_PROJECT_URL="https://github.com/thin-edge/thin-edge.io"

#
# Script settings
#
OUTPUT_DIR=${OUTPUT_DIR:-dist}
TARGET=
VERSION=0.0.0
CLEAN=1
PACKAGES=()
COMMAND=
PACKAGE_TYPES="deb,apk,rpm,tarball"

while [ $# -gt 0 ]
do
    case "$1" in
        --output)
            OUTPUT_DIR="$2"
            shift
            ;;
        --version)
            VERSION="$2"
            shift
            ;;
        --types)
            PACKAGE_TYPES="$2"
            shift
            ;;
        --clean)
            CLEAN=1
            ;;
        --no-clean)
            CLEAN=0
            ;;
        -h|--help)
            help
            exit 0
            ;;
        *)
            if [ -z "$COMMAND" ]; then
                COMMAND="$1"
            elif [ -z "$TARGET" ]; then
                TARGET="$1"
            else
                PACKAGES+=("$1")
            fi
            ;;
    esac
    shift
done

# Normalize output dir
OUTPUT_DIR="${OUTPUT_DIR%/}"

build_package() {
    name="$1"
    target="$2"

    package_arch=$(get_package_arch "$target")
    source_dir="target/$target/release"

    export PKG_ARCH="$package_arch"

    # Use symlinks to allow a fixed base directory in the nfpm.yaml definition
    rm -f .build
    ln -s "$source_dir" .build
    echo
    echo "Building: pkg_arch=$PKG_ARCH, source=$source_dir"

    COMMON_ARGS=(
        package
        -f "configuration/packaging/nfpm.$name.yaml"
        --target "$OUTPUT_DIR"
    )

    #
    # Debian/Ubuntu
    #
    # Special case for arm v6 on debian, since there is a name clash
    # * arm6 => armhf
    # * arm7 => armhf
    if [[ "$PACKAGE_TYPES" =~ deb ]]; then
        if [ "$package_arch" == "arm6" ]; then
            nfpm "${COMMON_ARGS[@]}" --packager deb --target "${OUTPUT_DIR}/${name}_${GIT_SEMVER}_armv6.deb"
        else
            nfpm "${COMMON_ARGS[@]}" --packager deb
        fi
    fi

    # RPM for CentOS/RHEL/RockyLinux
    if [[ "$PACKAGE_TYPES" =~ rpm ]]; then
        nfpm "${COMMON_ARGS[@]}" --packager rpm
    fi

    # Alpine
    if [[ "$PACKAGE_TYPES" =~ apk ]]; then
        nfpm "${COMMON_ARGS[@]}" --packager apk
    fi
}

build_meta_package() {
    name="$1"
    COMMON_ARGS=(
        package
        -f "configuration/packaging/meta/nfpm.$name.yaml"
        --target "$OUTPUT_DIR"
    )

    if [[ "$PACKAGE_TYPES" =~ deb ]]; then
        nfpm "${COMMON_ARGS[@]}" --packager deb
    fi

    if [[ "$PACKAGE_TYPES" =~ rpm ]]; then
        nfpm "${COMMON_ARGS[@]}" --packager rpm
    fi

    if [[ "$PACKAGE_TYPES" =~ apk ]]; then
        nfpm "${COMMON_ARGS[@]}" --packager apk
    fi
}

get_package_arch() {
    case "$1" in
        x86_64-unknown-linux-musl) pkg_arch=amd64 ;;
        aarch64-unknown-linux-musl) pkg_arch=arm64 ;;
        armv7-unknown-linux-musleabihf) pkg_arch=arm7 ;;
        arm-unknown-linux-musleabihf) pkg_arch=arm6 ;;
        *)
            echo "Unknown package architecture. value=$1"
            exit 1
            ;;
    esac
    echo "$pkg_arch"
}

build_tarball() {
    local name="$1"
    local target="$2"
    source_dir="target/$target/release"

    rm -f "$source_dir/$name"*tar.gz

    # Use underscores as a delimiter between version and target/arch to make it easier to parse
    # package_arch=$(get_package_arch "$target")
    # TAR_FILE="target/$ARCH/${name}_${VERSION}_${package_arch}.tar.gz"
    TAR_FILE="${OUTPUT_DIR}/${name}_${VERSION}_${target}.tar.gz"

    echo ""
    echo "Building: pkg_arch=$target, source=$source_dir"
    echo "using tarball packager..."
    tar cfz "$TAR_FILE" -C "$source_dir" --files-from <(printf "%s\n" "${PACKAGES[@]}")
    echo "created package: $TAR_FILE"
}

cmd_build() {
    for name in "${PACKAGES[@]}"; do
        build_package "$name" "$TARGET"
    done

    if [[ "$PACKAGE_TYPES" =~ tarball ]]; then
        build_tarball "tedge" "$TARGET" "${PACKAGES[@]}"
    fi
}

cmd_build_meta() {
    build_meta_package "tedge-full"
    build_meta_package "tedge-minimal"
}

prepare() {
    if [ "$CLEAN" = "1" ]; then
        rm -rf "$OUTPUT_DIR"
    fi
    mkdir -p "$OUTPUT_DIR"
}

banner() {
    local purpose="$1"
    echo ""
    echo "-----------------------------------------------------"
    echo "thin-edge.io packager: $purpose"
    echo "-----------------------------------------------------"
    echo "Parameters"
    echo ""
    echo "  packages: ${PACKAGES[*]}"
    echo "  version: $VERSION"
    echo "  types: $PACKAGE_TYPES"
    echo "  output_dir: $OUTPUT_DIR"
    echo ""
}

check_prerequisites() {
    if ! nfpm --version >/dev/null 2>&1; then
        echo "Missing dependency: nfpm"
        echo "  Please install nfpm and try again: https://nfpm.goreleaser.com/install/"
        exit 1
    fi
}

check_prerequisites

export GIT_SEMVER="$VERSION"

case "$COMMAND" in
    build)
        banner "build"
        prepare
        cmd_build
        ;;
    build_meta)
        # Note: build_meta does not support tarballs
        banner "build_meta"
        prepare
        cmd_build_meta
        ;;
    *)
        echo "Unknown command. Accepted commands are: [build, build_meta]"
        help
        exit 1
        ;;
esac

echo
echo "Successfully created packages"
echo
