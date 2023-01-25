# Testing Setup

This page will guide you how to install the pre-requisites to run the python based system/integration tests.

## Pre-requiresites

Before you can run the tests you need to install the pre-requisites:

* docker
* python3 (>=3.10)
* pip3
* nodejs (>=18)

It is assumed that you are running on either MacOS or Linux. If you are a Windows users then use WSL 2 and follow the **Debian/Ubuntu** instructions, or just use the dev container option (which requries docker which again can be run under WSL 2).

### Option 1: Installing the dependencies yourself

1. Install python3 (>= 3.8)
    
    Follow the [python instructions](https://www.python.org/downloads/), or

    **MacOS (using homebrew)**

    ```sh
    brew install python@3.10
    ```

    **Debian/Ubuntu**

    ```sh
    sudo apt-get install python3 python3-pip
    ```

2. Install nodejs (>=17)

    Follow the [nodejs instructions](https://nodejs.org/en/), or

    **MacOS (using homebrew)**

    ```sh
    brew install nodejs
    ```

    **Debian/Ubuntu**

    ```
    curl -fsSL https://deb.nodesource.com/setup_18.x | bash - \
    && apt-get install -y nodejs
    ```

3. Install docker

    **MacOS (using homebrew)**

    ```sh
    brew install docker docker-credential-helper
    ```

    **Debian/Ubuntu**

    Follow the [Docker install instructions](https://docs.docker.com/engine/install/ubuntu/) to install the docker ce engine (not Docker Desktop!!!)

### Option 2: Using the project's dev container

The dev container provides an easy to use (batteries included) approach where python3 is already installed. This option does require you to have docker and docker-compose installed, however that is it.

Checkout the [dev container instructions](./docs/DEV_CONTAINER.md) for more details.

## Getting started

1. Navigate to the Robot Framework folder

    ```sh
    cd tests/RobotFramework
    ```

2. Run the setup script which will create the python virtual environment and install the dependencies

    ```sh
    ./bin/setup.sh
    ```

3. Follow the console instructions and edit the `.env` file which was created by the `./bin/setup.sh` script

4. Switch to the new python interpreter (the one with `.venv` in the name)

    **Note: VSCode users**
    
    Open the `tasks.py` file, then select the python interpretter in the bottom right hand corner. Then enter the following location of python:

    ```sh
    tests/RobotFramework/.venv/bin/python3
    ```

    If you are not using a devcontainer then add the following to your workspace settings `.vscode/settings.json` file.

    ```json
    {
        "python.defaultInterpreterPath": "${workspaceFolder}/tests/RobotFramework/.venv/bin/python3",
        "robot.language-server.python": "${workspaceFolder}/tests/RobotFramework/.venv/bin/python3",
        "robot.python.executable": "${workspaceFolder}/tests/RobotFramework/.venv/bin/python3",
        "python.envFile": "${workspaceFolder}/.env"
    }
    ```

    Afterwards it is worthwhile reloading some of the VSCode extension via the Command Pallet

    * `Python: Restart Language Server`
    * `Robot Framework: Clear caches and restart Robot Framework`

5. On the console, activate the environment (if it is not already activated)

    ```sh
    pipenv shell
    ```

6. Run the tests

    ```sh
    invoke test
    ```

    Or you can run robot directly

    ```sh
    robot --outputdir output ./tests
    ```

# Viewing the test reports and logs

The reports and logs are best viewed using a web browser. This can be easily done setting up a quick local webserver using the following instructions.

1. Change to the robot framework directory (if you have not already done so)

    ```sh
    cd tests/RobotFramework
    ```

2. Open a console from the root folder of the project, then execute

    ```sh
    python -m http.server 9000 --directory output
    ```

    Or using the task

    ```sh
    invoke reports
    ```

3. Then open up [http://localhost:9000/tests/RobotFramework/output/log.html](http://localhost:9000/tests/RobotFramework/output/log.html) in your browser

## TODO

* Execute Command
    * Support return stdout and stderr in independent streams (optional arguments which shapes the return value)

* How to run a setup script for ssh
    * Confirm
        * All components removed / purged (clean state)
        * tedge already installed and connected

* Docker
    * How to speed up the image process by at least installing tedge (and just using the specially built image for the base, and only doing bootstrapp)
    * How to cleanup after each tests (but also support using 1 image for the suite)

* New commands
    * Convert to Debian arch: arm64 -> aarch64
    * Convert to Rust arch: aarch64 -> arm64
    * Transfer to Device
        * Support a text option, where normal text can be provided instead of a file
            - Save encoding/escaping problems when using printf etc.
    
    * `Device Should Not Exist` - Check that a device with the serial number does not exist (but reference by identity/type)

* How to get ssh credentials for a second device
    * What are the environment variables called? are they indexed, `SSH_CONFIG_1_HOSTNAME` etc..

* Hide sensitive information from the logs (Generic python log filter?): Though github might take care of this

* Install script for using when using ssh
    * How to include environment variables from dotenv file as the ssh environment setting is not enabled by default
    * Need to install the mqtt monitor service to support
    * `Execute Command` support setting environment variables for individual commands and these should not be logged

* Allow docker adapter to reuse existing container (to replicate ssh device)

* Allow installing list of artifacts
    * copy files, then run install script to install manual .deb files

* Create cleanup script

* Export robot libraries html docs

### Limitation?

How to enter text at a prompt launched via `docker exec` or `ssh`?
    * `ssh` seems to support it as the SSHLibrary has the `Write` keyword, but would docker exec work like this?
