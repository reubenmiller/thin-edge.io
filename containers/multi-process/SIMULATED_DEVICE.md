# Simulated Device

This page details how to create a simulated containerized device which is running thin-edge using systemd. The container requires `privileged` mode to allow systemd to function under the hood. This is not intended for usage in a production environment.

## Starting a simulated device

1. Pull the image from docker hub

    ```sh
    docker pull reubenmiller/tedge-device:0.9.0-218-gd8bd3b33-11
    ```

2. Start the container

    ```sh
    docker run -d --env DEVICE_ID=tedge-poc001 --privileged reubenmiller/tedge-device:0.9.0-218-gd8bd3b33-11
    ```

    Note down the container id on the console

3. Go into the container

    ```sh
    docker exec -it <container_id> bash
    ```

    Where the `<container_id>` is the id of the container from the previous step.

4. Generate a CSR

    ```sh
    tedge-cert.sh csr --out-csr /tmp/device.csr
    ```

    Do something with the `/tmp/device.csr` file, e.g. send it to the PKI provider.

5. Update the certificate

    ```sh
    tedge-cert.sh set --c8y-url example.cumulocity.io --certificate /tmp/cert/device.pem
    ```

## Troubleshooting

On unexpected problems, try reconnecting the mapper.

```sh
tedge reconnect c8y
```


## Building your own container

If you need to add your own dependencies to the container image, then just create a new `Dockerfile` and reference the same docker image as above. You can then install any dependencies that you need. Leave the `ENTRYPOINT` and `CMD` settings as is, and everything should just work.

The base image is using debian (`debian:11-slim`), and `systemd` is installed and enabled, so any services should be configured to startup using `systemd`.

Below is an example of a custom `Dockerfile`:

```dockerfile
FROM reubenmiller/tedge-device:0.9.0-218-gd8bd3b33-11

# Install other applications
RUN apt-get update \
    && apt-get install -y curl

# Copy any local apps that you need
COPY ./bin/local-app /usr/bin/
```

Then build the image

```sh
docker build -t mycustomimage .
```

Then you can use it in the same way as before, but use your image name instead.

```sh
docker run -d --env DEVICE_ID=tedge-poc001 --privileged mycustomimage
```
