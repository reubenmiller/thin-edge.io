
## Building a multi-platform image

These instructions are a best guess, and have not been tested on all setups. If you are having problems please checkout the official [docker documentation](https://docs.docker.com/build/building/multi-platform/) for multi-platform builds.

1. Open a console and change to the following directory

    ```sh
    cd containers/multi-process
    ```

2. Log into the container register (by default this is dockerhub). You only need to do this step once

    ```sh
    docker login
    ```

3. Build/push the multi-platform images

    ```sh
    docker buildx create --name multiarch --driver docker-container --use
    IMAGE="reubenmiller/tedge-device:0.9.0-218-gd8bd3b33-9"
    docker buildx build --platform "linux/amd64,linux/arm64,linux/arm/v7" -t "$IMAGE" -f images/debian.dockerfile . --push
    ```

    If you have problems with the buildx then you can try to delete context and recreate it

    ```
    docker buildx rm --name multiarch || true
    ```

    Alternatively, if you have having problems with an error like `Error: multiple platforms feature is currently not supported for docker driver`, then try building the images for different platforms one-by-one.

    ```sh
    IMAGE="reubenmiller/tedge-device:0.9.0-218-gd8bd3b33-9"
    platforms=(linux/amd64 linux/arm64 linux/arm/v7)
    for platform in "${platforms[@]}"; do
        echo -e "$platform"
        docker buildx build --platform "$platform" -t "$IMAGE" -f images/debian.dockerfile . --push
    done
    ```

## Using docker compose / podman-compose

Note: You should be able to use `podman-compose` instead of `docker compose` if you so desire.

1. Open a console and change directory to the docker compose file

    ```sh
    cd containers/multi-process/examples/cloud
    ```

2. Start the project

    ```sh
    docker compose up -d
    ```

3. Bootstrap the device (this only needs to be done once, as the data will persist across containers)

    ```sh
    docker compose exec tedge bootstrap.sh
    ```

    You will be prompted for the information.

    If you wish to run this without interaction, then use the `--no-prompt` option, though this will require you to provide all of the cloud connection information as arguments (or environment variables).

    ```sh
    docker compose exec tedge \
        bootstrap.sh \
            --no-prompt \
            --c8y-user "myuser@company.com" \
            --c8y-url "mytenant.cumulocity.com" \
            --c8y-password "myS3cur&Ya3v0d" \
            --random
    ```

    You can set a specific device using the `--device-id` instead of the `--random` flag.
