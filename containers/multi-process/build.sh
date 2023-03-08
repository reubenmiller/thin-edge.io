#!/bin/bash

set -e

SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
pushd "$SCRIPT_DIR" >/dev/null || exit 1

# Install support for other binary formats
docker run --privileged --rm tonistiigi/binfmt --install all >/dev/null
if ! docker buildx inspect mybuilder; then
    # Install a dedicated builder
    docker buildx create --name mybuilder --driver docker-container --bootstrap --use
fi

docker buildx use mybuilder

IMAGE="reubenmiller/tedge-device:0.9.0-218-gd8bd3b33-11"
docker buildx build --platform "linux/amd64,linux/arm64,linux/arm/v7" -t "$IMAGE" -f images/debian.dockerfile . --push
