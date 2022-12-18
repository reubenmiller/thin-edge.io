# Getting started

1. Navigate to the Robot Framework folder

    ```sh
    cd tests/RobotFramework
    ```

2. Run the setup script which will create the python virtual environment and install the dependencies

    ```sh
    ./bin/setup.sh
    ```

3. Follow the console instructions and edit the `.env` file which was created by the `./bin/setup.sh` script

4. Switch to the new interpreter in VS Code (the one with `venv` in the name)

    **Note: VSCode users**
    
    You will have to open up a python file before VSCode will show the python settings. Sometimes reloading the window helps if you are having problems.

5. On the console, activate the environment

    ```sh
    source env/bin/activate
    ```

6. Run your first tests

    ```sh
    robot tests
    ```

# TODO

* [ ] Cumulocity
    * [x] Check parent child relationship
    * [ ] Get managed object and compare name
    * [ ] Send configuration file to device as operation

* [ ] Json comparison (however the `JsonLibrary` can be used in the meantime)
    * [ ] Value matches pattern
    * [ ] Value is equal (support comparing subsections of json)

* [ ] Child devices
    * [ ] Configure child device
    * [ ] Purge child device information from the filesystem
    * [ ] Subscribe to mqtt and then PUT to http server


## References

* https://github.com/joergschultzelutter/robotframework-demorobotlibrary
* https://tech.bertelsmann.com/en/blog/articles/workshop-create-a-robot-framework-keyword-library-with-python
