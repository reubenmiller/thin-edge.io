#!/bin/bash

set -e

SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
pushd "$SCRIPT_DIR" >/dev/null || exit 1

COPY_ONLY=0
TARGETS=()

usage() {
    echo "

USAGE
    $0 [--target <triple>] [--copy-only]

FLAGS
    --target <triple>   Target triple to build, e.g. aarch64-unknown-linux-musl, x86_64-unknown-linux-musl etc.
    --copy-only         Only copy the binaries, don't build any images

EXAMPLES
    $0 --target aarch64-unknown-linux-musl
    # Build a specific target

    $0 --target aarch64-unknown-linux-musl --copy-only
    # Build a specific target
    " >&2
}

while [ $# -gt 0 ]; do
    case "$1" in
        --copy-only)
            COPY_ONLY=1
            ;;
        --target)
            TARGETS+=("$2")
            shift
            ;;
        --help|-h)
            usage
            exit 0
            ;;
    esac
    shift
done

# Set default targets if nothing has been specified
if [ "${#TARGETS[@]}" -eq 0 ]; then
    TARGETS=(
        aarch64-unknown-linux-musl
        x86_64-unknown-linux-musl
        armv7-unknown-linux-musleabihf
    )
fi

get_platform() {
    case "$1" in
        aarch64-unknown-linux-musl)
            echo "linux/arm64"
            ;;
        x86_64-unknown-linux-musl)
            echo "linux/amd64"
            ;;
        armv7-unknown-linux-musleabihf)
            echo "linux/arm/v7"
            ;;
    esac
}



# Install support for other binary formats
docker run --privileged --rm tonistiigi/binfmt --install all >/dev/null
if ! docker buildx inspect mybuilder; then
    # Install a dedicated builder
    docker buildx create --name mybuilder --driver docker-container --bootstrap --use
fi

for TARGET in "${TARGETS[@]}"; do

    PLATFORM=$(get_platform "$TARGET")

    echo "Cleaning existing binaries"
    find ./bin/ -type f \( ! -name ".gitkeep" \) -delete

    echo "Building target"
    (cd ../.. && ./ci/build_scripts/build.sh "$TARGET")

    echo "Copying binaries from target/$TARGET"
    find "../../target/$TARGET/release" -type f \( \( -name "tedge*" -o -name "c8y*" \) -a ! -name "*.d" \) -depth 1 -exec cp {} ./bin/ \;

    if [ "$COPY_ONLY" = 1 ]; then
        continue
    fi

    IMAGE="reubenmiller/tedge-component:alpine"
    docker buildx build --platform "$PLATFORM" -t "$IMAGE" -f images/tedge-component.alpine.dockerfile . --push

    IMAGE="reubenmiller/tedge-component:alpine-docker"
    docker buildx build --platform "$PLATFORM" -t "$IMAGE" -f images/tedge-component.alpine-dockercli.dockerfile . --push

    # Note: mosquitto image does not support armv7!
    if [ "$PLATFORM" != "linux/arm/v7" ]; then
        IMAGE="reubenmiller/tedge-mosquitto:2.0.14"
        docker buildx build --platform "$PLATFORM" -t "$IMAGE" -f images/mosquitto.dockerfile . --push
    else
        echo
        echo "WARNING: Skipping building mosquitto image as it is not supported on $PLATFORM"
        echo
    fi

    IMAGE="reubenmiller/community-container-monitor:latest"
    docker buildx build --platform "$PLATFORM" -t "$IMAGE" -f images/community-container-monitor.dockerfile . --push
done

popd >/dev/null || exit 1
