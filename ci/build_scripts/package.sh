#!/usr/bin/env bash
set -e

export GIT_SEMVER="${GIT_SEMVER:-0.1.1}"
export CI_PROJECT_URL="https://github.com/thin-edge/thin-edge.io"

OUTPUT_DIR=dist/
rm -rf "$OUTPUT_DIR"
mkdir -p "$OUTPUT_DIR"

build_package() {
    name="$1"
    target="$2"

    package_arch=$(get_package_arch "$target")
    source_dir="target/$target/release"

    export PKG_ARCH="$package_arch"

    # Use symlinks to allow a fixed base directory in the nfpm.yaml definition
    rm -f .build
    ln -s "$source_dir" .build
    echo "Building: arch=$PKG_ARCH, source=$source_dir" >&2

    COMMON_ARGS=(
        package
        -f "configuration/packaging/nfpm.$name.yaml"
        --target "$OUTPUT_DIR"
    )

    # Special case for arm v6 on debian, since there is a name clash
    # * arm6 => armhf
    # * arm7 => armhf
    if [ "$package_arch" == "arm6" ]; then
        nfpm "${COMMON_ARGS[@]}" --packager deb --target "${OUTPUT_DIR%/}/${name}_${GIT_SEMVER}_armv6.deb"
    else
        nfpm "${COMMON_ARGS[@]}" --packager deb
    fi
    # nfpm "${COMMON_ARGS[@]}" --packager rpm
    # nfpm "${COMMON_ARGS[@]}" --packager apk   
}

build_meta_package() {
    name="$1"
    COMMON_ARGS=(
        package
        -f "configuration/packaging/meta/nfpm.$name.yaml"
        --target "$OUTPUT_DIR"
    )
    nfpm "${COMMON_ARGS[@]}" --packager deb
    # nfpm "${COMMON_ARGS[@]}" --packager rpm
    # nfpm "${COMMON_ARGS[@]}" --packager apk
}

get_package_arch() {
    case "$1" in
        x86_64-unknown-linux-musl) pkg_arch=amd64 ;;
        aarch64-unknown-linux-musl) pkg_arch=arm64 ;;
        armv7-unknown-linux-musleabihf) pkg_arch=arm7 ;;
        arm-unknown-linux-musleabihf) pkg_arch=arm6 ;;
        *)
            echo "Unknown package architecture. value=$1" >&2
            exit 1
            ;;
    esac
    echo "$pkg_arch"
}

PACKAGES=(
    c8y-configuration-plugin
    c8y-firmware-plugin
    c8y-log-plugin
    c8y-remote-access-plugin
    tedge-agent
    tedge-apt-plugin
    tedge-mapper
    tedge-watchdog
    tedge
)

for name in "${PACKAGES[@]}"; do
    build_package "$name" "x86_64-unknown-linux-musl"
    build_package "$name" "aarch64-unknown-linux-musl"
    build_package "$name" "armv7-unknown-linux-musleabihf"
    build_package "$name" "arm-unknown-linux-musleabihf"
done

build_meta_package "tedge-full"
build_meta_package "tedge-minimal"

echo "Successfully created packages" >&2
