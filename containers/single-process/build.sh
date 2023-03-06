#!/bin/bash

set -e

SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
pushd "$SCRIPT_DIR" >/dev/null || exit 1

TARGETS=(
    x86_64-unknown-linux-musl
    aarch64-unknown-linux-musl
    armv7-unknown-linux-musleabihf
)

PLATFORMS=(
    linux/amd64
    linux/arm64
    linux/arm/v7
)

INDEX=0
for TARGET in "${TARGETS[@]}"; do

    PLATFORM="${PLATFORMS[$INDEX]}"

    echo "Cleaning existing binaries"
    find ./bin/ -type f \( ! -name ".gitkeep" \) -delete

    echo "Copying binaries from target/$TARGET"
    find "../../target/$TARGET/release" -type f \( \( -name "tedge*" -o -name "c8y*" \) -a ! -name "*.d" \) -depth 1 -exec cp {} ./bin/ \;

    IMAGE="reubenmiller/tedge-component:alpine"
    docker buildx build --platform "$PLATFORM" -t "$IMAGE" -f images/tedge-component.alpine.dockerfile . --push

    IMAGE="reubenmiller/tedge-component:alpine-docker"
    docker buildx build --platform "$PLATFORM" -t "$IMAGE" -f images/tedge-component.alpine-dockercli.dockerfile . --push

    IMAGE="reubenmiller/tedge-mosquitto:2.0.14"
    docker buildx build --platform "$PLATFORM" -t "$IMAGE" -f images/mosquitto.dockerfile . --push

    IMAGE="reubenmiller/community-container-monitor:latest"
    docker buildx build --platform "$PLATFORM" -t "$IMAGE" -f images/community-container-monitor.dockerfile . --push

    INDEX=$((INDEX+1))
done

popd >/dev/null || exit 1
