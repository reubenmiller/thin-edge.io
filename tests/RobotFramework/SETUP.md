# Testing Setup

This page will guide you how to install the pre-requisites to run the python based system/integration tests.

## Pre-requiresites

Before you can run the tests you need to install the pre-requisites (mainly python3, pip and nodejs).

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

    **Tip for MacOS colima users**

    Uncomment the `DOCKER_HOST` variable and replace the `<username>` with your username. This is required to inorder for the runner to find docker.

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
