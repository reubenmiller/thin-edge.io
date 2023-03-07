#!/bin/bash

set -e

SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
pushd "$SCRIPT_DIR" >/dev/null || exit 1

TARGETS=(
    aarch64-unknown-linux-musl
    x86_64-unknown-linux-musl
    armv7-unknown-linux-musleabihf
)

PLATFORMS=(
    linux/arm64
    linux/amd64
    linux/arm/v7
)

# Install support for other binary formats
docker run --privileged --rm tonistiigi/binfmt --install all >/dev/null
if ! docker buildx inspect mybuilder; then
    # Install a dedicated builder
    docker buildx create --name mybuilder --driver docker-container --bootstrap --use
fi

INDEX=0
for TARGET in "${TARGETS[@]}"; do

    PLATFORM="${PLATFORMS[$INDEX]}"

    echo "Cleaning existing binaries"
    find ./bin/ -type f \( ! -name ".gitkeep" \) -delete

    echo "Building target"
    (cd ../.. && ./ci/build_scripts/build.sh "$TARGET")

    echo "Copying binaries from target/$TARGET"
    find "../../target/$TARGET/release" -type f \( \( -name "tedge*" -o -name "c8y*" \) -a ! -name "*.d" \) -depth 1 -exec cp {} ./bin/ \;

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

    INDEX=$((INDEX+1))
done

popd >/dev/null || exit 1
