
## TODO

* [ ] Conflicting data volumes, e.g. `/etc/tedge/` is not shared fully if the other containers already has the partition, e.g. `/etc/tedge/` created during container build time. This is not so bad, however it is a bit unexpected as the c8y-configuration-plugin was expecting to see the tedge-mosquitto.conf info under `/etc/tedge/mosquitto-conf/` but instead it was empty
    * Look at doing the initialization in a different folder? The at runtime switch to the shared volume?

* [ ] Child configuration management probably won't work, as the container does not know where the tedgeapi endpoint is.
    * This will probably require moving the configuration so it can have independent settings, e.g. using the --config-dir "/plugin", then setting the external http address to "tedgeapi:8000"

## Findings

### Not all services have lock files - solved after refactoring

Lock files seem to be necessary as the client client id

The following services don't have lock files.

* c8y-log-plugin
* c8y-configuration-plugin

Though these services do:

* tedge-agent
* tedge-mapper-*

Though avoiding lock files would be ideal in single-process container environments.


### Components hang if the local mqtt broker does not exist - #1797

The following components seem to hang if the broker does not exist. There is very little log output to help determine what is going on, just the tokio trace messages about polling.

* `c8y-log-plugin`
* `c8y-configuration-plugin`
* `c8y-firmware-plugin`

**Resolution**

If the broker does not exist after x seconds, then the components should give up and exit.


## Usage of linux users

### Binaries require an explicit user, this does not makes sense for isolated binaries

Managing the linux users in multiple containers is cumbersome

* The service user should be configurable
* The process should not fail due to a missing user as the process should be able to be started by any user as this enable maximum flexibility for the user to restrict or open the permissions of that user. E.g. the maintainer can choose to give the binary root access or not.


### Sudo is still a hard requirement for some components

* tedge-agent relies on this when running the sm-plugins. It will fail with a vague reason that a file does not exist...but there is no mention that `sudo` is missing.

```sh
sm-agent: plugin_sm::plugin_manager: An error occurred while trying to run: /etc/tedge/sm-plugins/container: No such file or directory (os error 2)
```

Sudo is less relevant when running in a container, as the container provides the isolation.

**Questions**

* the plugin interface is called with `sudo`, though should it only be used if the eui is not already 0? Otherwise it is just adding a pointless dependency.
* does whole `sudo` thing becomes less relevant with mqtt based plugins, as the access to the endpoints is restricted via the api endpoints/authentication of them.
* Does sudo really need to be used, or are software management plugins trusted enough as they can only be installed via a sudo users?

**Affected files**

* crates/core/plugin_sm/src/plugin.rs (Software management plugins)
* crates/core/plugin_sm/src/plugin_manager.rs (Generic )
* crates/core/tedge_agent/src/agent.rs (for access to the restart command)
    * 
    * For the restart commands, can't we just use a installation file, rather than configuration,? Can't the restart command just be restricted to root access anyway? A wrapper script could be created which has root executable writes, and it should not be editable by non-root users.

**Resolution**

* Skip running a command with `sudo` if the effective user id is already set to 0
* Ignore/raise a warning if any configuration files which contain executable commands have the wrong file permissions (e.g. not 644)



## Discussion points


### Tedge root topic structure should be configurable

The tedge root topic `tedge` should be configurable, to allow for multiple thin-edges on the same broker. This would allow customizing `tedge-manufacturer/measurements` and `tedge-customer/measurements` style topics for different purposes (though it assumes that you can define ACL rules to isolate the two topics).


**Multi thin-edge tenancy on a single broker?**

* Allow the root thin-edge topic to be configurable, this would align multiple thin-edge.io to use the same mqtt broker
* This would also request using the root topic in the MQTT client ids in each of the clients when connection to avoid clients rejecting each other when two clients try to connect using the same id.


### Add mqtt/http interfaces for the following functionality

File based plugins/interfaces have the limitation that they need to be installed in the `tedge-agent` container in order for them to be accessible, and also 

* Software management
* Log file retrieval
* Configuration management for components (not just devices) so it can support components that don't have access to the same filesystem where the agent is running.
* Custom operation handling (e.g. c8y-remote-access-plugin, command plugin)

**Resolution**

* Add support for MQTT/HTTP based communication to get log files from different components and/or child devices.
* Create way that a component can subscribe for specific operations (though it should be limited to who receives the request to prevent a multiple clients reacting to the same request)
    * Question: Should there be only 1 component which will reply to the operation?

        ```
        <namespace>/<main|local-child-id>/<tedge-func>/<type>/<req|rep>/<component-id>
        ```

        **Examples**

        ```sh
        # Request response for a shell handler for the main device
        # m = main
        tedge/m/op/shell/req/shellhandler
        tedge/m/op/shell/res/shellhandler

        # Child types: d = child device, a = 
        # How to add a child addition to a child device?
        # How to prevent name clashes across children
        tedge/d/<child_type>/child01
        tedge/d/child01/op/shell/req/shellhandler
        tedge/a/child01/op/shell/res/shellhandler
        tedge/i/child01/op/shell/res/shellhandler

        tedge/<child>/op/<type>/<component>
        ```

**Software management notes**

Add support for MQTT/HTTP based communication of the sm-software plugins. This allows the software management plugins to be deployed anywhere, and it is more inline with how child devices will be communication with it anyway.

However, it is important to note that "simplicity" of the binary approach should still be preserved. So there might be calls for a port light-weight wrapper over the sm-plugin interface that can easily convert

* One listener that can all multiple plugins, so we don't have a one-listener per plugin (to minimize required resources)

### File based plugins are not very useful, or need to be included in the same container as tedge-agent

File based plugins need to be installed in the `tedge-agent` container in order for them to be accessible.

* shell plugin
* remote-access plugin

Whilst the plugins could be stored in a shared volume, it add more dependencies to the containerization setup.


### Registration of components and their functionality

The only way for a plugin to register support for an operation to the `tedge-agent` is if it has access to the filesystem used by the `tedge-agent`.

**Resolution**

Create mqtt interface to register/unregister support for different operations. This allows the plugin to be hosted independently of the `tedge-agent`. A similar concept could also be used to register/unregister functionality for child devices.


### API to get information about the device

Most of the component currently rely on the `device.id` property to even initialize. The components rely on access to the public device certificate to be able to read the device.id. This dependency on a file means that plugins cannot not be deployed in a distributed manner. 

Options could include:
* API to request the device.id from the `tedge-agent`
* Support setting the device id via environment variable rather than reading it from the public device certificate

Though it seems strange that other components would have to care about the certificate which is only required for cloud communication, so just providing the device id would be good enough, or removing the need for it altogether in each of the components would be even better.


# Tickets


### Not all components supports an external mqtt broker [#1773](https://github.com/thin-edge/thin-edge.io/issues/1773)

The following components assume a local mqtt broker, and don't fully initialize because of it. All of the components still read the mqtt port (`MqttPortSetting`), however they don't the `MqttBindAddressSetting`.

* `c8y-log-plugin`
* `c8y-configuration-plugin`
* `c8y-firmware-plugin`

**Resolution**

* The components should read the broker address from `MqttBindAddressSetting`



### Configure via Environment Variables #1783

Ideally an application should follow a [12-factor application](https://12factor.net/), which allows setting configuration via configuration file or environment files. Setting values via environment variables are especially convenient for containers.

Controlling settings via environment variables allows to spin up new isolated containers with single commands, rather than having to build a configuration file dynamically during runtime.

For example the following toml setting could be mapped to an environment variable:

```toml
[c8y]
url = 'test-ci-runner01.latest.stage.c8y.io'
```

Where a section header is separated via a double underscore `__`.

```sh
C8Y__URL=test-ci-runner01.latest.stage.c8y.io
```

Note this will not work for all of the complex toml configuration, however at least the key configuration such as c8y.url, broker address etc, should work just fine.

### Configurable MQTT broker connection settings #1785

Each component should configurable mqtt broker connection settings, for example the following should be configurable:

* ~Select connection mode to MQTT broker (e.g. username/password or certificate based)~
* ~Username/password configuration (used when connecting via username/password authentication)~
* Certificate paths for private, public and ca certificates (used when connecting via cert base authentication)


### Running `--init` commands fail if `/run/lock` folder does not exist #1802

When building on alpine, the `/run/lock` folder does not exist, and hence any calls to binary which requires a lock file, e.g. `tedge --init` or `tedge-agent --init` will fail in the build steps. It forces the user to have to manually create the `/run/locks` directory during build time.

**Resolution**

There are a few options that could help solve this:

* Add a new configuration option to disable the lock file generation
