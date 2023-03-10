# thin-edge.io running under single-process containers

## Pre-requisites

Before you get started, please check that you have the pre-requisites installed on your machine.

* containerization engine (instructions are using `docker`/`docker compose`, however you can use `podman` and `docker-compose`)

## Getting started

You can spin up the docker-compose of the thin-edge.io and all of its components by using the following steps:

1. Open a terminal, and navigate to the folder where the `docker-compose.yaml` file is located

    ```sh
    cd containers/single-process/examples/tedge-isolated
    ```

2. Create the dot env files using the template files

    ```sh
    [ ! -f .env ] && cp env.template .env
    [ ! -f .env.secrets ] && cp env.secrets.template .env.secrets
    ```

    The command above will copy the template if the dot env file does not already exist.

3. Edit the `.env` and `.env.secrets` files

    **file: .env**
    ```sh
    DEVICE_ID=single_process_container_001
    C8Y_BASEURL=mytenant.example.c8y.io
    ```

    **file: .env.secrets**
    ```sh
    C8Y_USER=peter@company.com
    C8Y_PASSWORD="your-password"
    ```

4. Start the project

    ```sh
    docker compose up --build
    ```
    
    Alternatively you can run it in the background using the `-d` option.

    ```sh
    docker compose up --build
    ```

## Inspecting a running container

If you would like to have a look inside the running containers then you can open a shell

```sh
docker compose exec tedge-agent sh
```

Or opening a shell on the `mqtt-broker` container:

```sh
docker compose exec mqtt-broker sh
```

## Troubleshooting connectivity to the tedge http server from within a container

You can check if the tedge http server is reachable from inside a container.

1. Open a shell to the running container, below shows how to connect to the `tedge-configuration-plugin`

    ```sh
    docker compose exec tedge-configuration-plugin bash
    ```

2. From the container's shell, check the connectivity to the `tedge-agent` container by using its alias and a curl command.

    ```sh
    curl http://tedgeapi/somefile_that_does_not_exist
    ```

    **Notes**
    
    The curl command should return `not found`, as you have actually reached the server successfully, it just means that the server does not have a file under the path that you gave it. If the http server is not reachable, then you would get a "host not reachable" error or a "connection refused" error.


## Running the example using images published to docker hub

```sh
docker compose -f ./docker-compose.yaml -f override.cloud.yaml up
```
