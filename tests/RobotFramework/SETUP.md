# Getting started

1. Navigate to the Robot Framework folder

    ```sh
    cd tests/RobotFramework
    ```

2. Run the setup script which will setup the python virtual environment and install the dependencies

    ```sh
    ./bin/setup.sh
    ```

3. Follow the console instructions and edit the `.env` file which was created by the `./bin/setup.sh` script

3. Switch to the new interpreter in VS Code (the one with `venv` in the name)

    **Note: VSCode users**
    
    You will have to open up a python file before VSCode will show the python settings. Sometimes reloading the window helps as well.

4. On the console, activate the environment

    ```sh
    source env/bin/activate
    ```

5. Run your first tests

    ```sh
    robot tests
    ```

# TODO

* [ ] Cumulocity
    * [ ] Check parent child relationship
    * [ ] Get managed object and compare name
    * [ ] Send configuration file to device as operation

* [ ] Json comparison
    * [ ] Value matches pattern
    * [ ] Value is equal (support comparing subsections of json)


* [ ] Child devices
    * [ ] Configure child device
    * [ ] Purge child device information from the filesystem
    * [ ] Subscribe to mqtt and then PUT to http server

* Tedge
    * [x] Reconnect tedge
        ```
        sudo tedge disconnect c8y
        sudo tedge connect c8y
        ```
    * [x] Set tedge config

        ```
        sudo tedge config set mqtt.external.bind_address $value
        ```

* Device adapter
    * `List Directories In Directory`
    * [x] Directory is empty / not empty
    * [x] Directory exists / not exists
    * [x] Start/stop/restart systemd service? abstract to service manager?

## References

* https://github.com/joergschultzelutter/robotframework-demorobotlibrary
* https://tech.bertelsmann.com/en/blog/articles/workshop-create-a-robot-framework-keyword-library-with-python
